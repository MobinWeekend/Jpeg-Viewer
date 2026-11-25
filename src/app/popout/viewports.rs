use super::*;

impl PopoutManager {
    pub(crate) fn sync_active_animation(&mut self, tab_id: u64, source: Option<&GifAnimation>) {
        let Some(window_state) = self.windows.get(&tab_id) else {
            return;
        };
        let Ok(mut window) = window_state.lock() else {
            return;
        };
        let Some(source) = source else {
            window.animation = None;
            window.last_frame_index = None;
            return;
        };

        let replace = window
            .animation
            .as_ref()
            .is_none_or(|current| current.frame_count() != source.frame_count());
        if replace {
            let mut animation = source.clone();
            animation.is_playing = true;
            animation.last_update = Instant::now();
            window.animation = Some(animation);
            window.last_frame_index = None;
        } else if let Some(animation) = window.animation.as_mut()
            && (animation.speed_multiplier - source.speed_multiplier).abs() > f32::EPSILON
        {
            animation.set_speed(source.speed_multiplier);
        }
    }

    /// Register deferred pop-out viewports. Their callbacks continue rendering
    /// and animating while the root application is minimized/occluded, unlike
    /// immediate viewports whose lifetime depended on the root update pass.
    pub(crate) fn render_viewports(&mut self, ctx: &egui::Context) {
        for viewport_id in self.pending_close.drain(..) {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
        }
        if self
            .lifecycle_repaint_until
            .is_some_and(|until| Instant::now() < until)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            self.lifecycle_repaint_until = None;
        }

        // A close button in a deferred child cannot directly borrow this manager.
        // It sets a flag in the shared window state; harvest those flags next root pass.
        let closed = self
            .windows
            .iter()
            .filter_map(|(tab_id, state)| {
                state
                    .lock()
                    .ok()
                    .filter(|window| window.close_requested)
                    .map(|window| (*tab_id, preset_from_window(&window)))
            })
            .collect::<Vec<_>>();
        if !closed.is_empty() {
            for (tab_id, preset) in closed {
                self.windows.remove(&tab_id);
                self.presets.insert(tab_id, preset);
            }
            self.mark_lifecycle_repaint();
        }

        let states = self
            .windows
            .iter()
            .map(|(tab_id, state)| (*tab_id, Arc::clone(state)))
            .collect::<Vec<_>>();
        let (parent_focused, root_pointer) = ctx.input(|input| {
            let pointer = input
                .pointer
                .hover_pos()
                .zip(input.viewport().outer_rect.map(|rect| rect.min))
                .map(|(pointer, origin)| pointer + origin.to_vec2());
            (input.viewport().focused.unwrap_or(false), pointer)
        });
        let wants_pointer_position = states.iter().any(|(_, state)| {
            state.lock().is_ok_and(|window| {
                window.click_through && (window.pass_near_enabled || window.pass_hover_enabled)
            })
        });
        let pointer_screen_pos = if wants_pointer_position {
            crate::platform::pointer::global_position(ctx).or(root_pointer)
        } else {
            root_pointer
        };
        for (tab_id, window_state) in states {
            let (viewport_id, builder) = match window_state.lock() {
                Ok(mut window) => {
                    // Keep all pop-outs' Pass opacity triggers synchronized,
                    // even while their owning Image tab is not the active tab.
                    window.pointer_screen_pos = pointer_screen_pos;
                    (
                        window.viewport_id,
                        window.viewport_builder(tab_id, parent_focused),
                    )
                }
                Err(_) => continue,
            };
            // An AlwaysOnTop pop-out otherwise remains above the main window
            // even after the user activates the main app. Yield topmost status
            // while the parent is focused, then restore it when focus leaves.
            if let Ok(mut window) = window_state.lock() {
                let level = if window.stay_on_top && !parent_focused {
                    egui::WindowLevel::AlwaysOnTop
                } else if window.stay_on_bottom {
                    egui::WindowLevel::AlwaysOnBottom
                } else {
                    egui::WindowLevel::Normal
                };
                if window.applied_window_level != Some(level) {
                    ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::WindowLevel(level));
                    window.applied_window_level = Some(level);
                }
            }
            let callback_state = Arc::clone(&window_state);
            let parent_ctx = ctx.clone();
            ctx.show_viewport_deferred(viewport_id, builder, move |viewport_ctx, _class| {
                let Ok(mut window) = callback_state.lock() else {
                    return;
                };
                if window.close_requested {
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                #[cfg(target_os = "windows")]
                if !window.dwm_transparency_applied {
                    window.dwm_transparency_applied =
                        enable_popout_dwm_transparency(tab_id, window.viewport_generation);
                }
                if window.native_window_effects_applied != Some(window.remove_os_window_effects)
                    && apply_popout_native_window_effects(
                        tab_id,
                        window.viewport_generation,
                        window.remove_os_window_effects,
                    )
                {
                    window.native_window_effects_applied = Some(window.remove_os_window_effects);
                }
                if !window.native_owner_detached {
                    window.native_owner_detached =
                        detach_popout_owner(tab_id, window.viewport_generation);
                }
                if window.stay_on_bottom {
                    viewport_ctx.request_repaint_after(Duration::from_millis(350));
                    if window.last_bottom_enforcement.elapsed() >= Duration::from_millis(350) {
                        enforce_bottom_z_order(tab_id, window.viewport_generation);
                        window.last_bottom_enforcement = Instant::now();
                    }
                }
                let animated = window.ball_mode || window.dvd_mode;
                let lifecycle_repaint = window
                    .lifecycle_repaint_until
                    .is_some_and(|until| Instant::now() < until);
                if !lifecycle_repaint {
                    window.lifecycle_repaint_until = None;
                }
                let should_close = egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(viewport_ctx, |ui| window.render(ui, tab_id))
                    .inner;
                // The child has its own repaint loop, but a minimized parent can
                // otherwise remain asleep after the compositor restores it. Keep
                // a low-rate wake scheduled on the root context while physics is
                // running; the root's minimized fast path remains lightweight.
                if lifecycle_repaint {
                    // Wake both independent UI contexts at frame cadence during
                    // the short open/close transition window.
                    viewport_ctx.request_repaint_after(Duration::from_millis(16));
                    parent_ctx.request_repaint_after(Duration::from_millis(16));
                }
                if animated {
                    // Movement modes must keep the child viewport repainting even
                    // when a physics update exits early (for example, unavailable
                    // monitor bounds or a temporary native drag).
                    viewport_ctx.request_repaint_after(Duration::from_millis(16));
                    parent_ctx.request_repaint_after(Duration::from_millis(250));
                }
                if should_close {
                    window.close_requested = true;
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    parent_ctx.request_repaint();
                }
            });
        }
    }
}
