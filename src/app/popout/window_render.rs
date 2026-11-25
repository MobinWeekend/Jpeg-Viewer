use super::*;

impl PopoutWindow {
    pub(super) fn render(&mut self, ui: &mut egui::Ui, tab_id: u64) -> bool {
        let ctx = ui.ctx().clone();
        self.synchronize_native_size(&ctx);
        self.synchronize_native_position(&ctx);
        if self.click_through {
            self.resize_gesture = None;
        }
        if self.dvd_dragging && (self.click_through || !self.dvd_mode) {
            self.dvd_dragging = false;
            self.last_dvd_tick = Instant::now();
        }
        if self.ball_dragging && (self.click_through || !self.ball_mode) {
            // A mode/input change can happen from the separate settings viewport
            // while the pointer is captured by this one. Never leave a native
            // window drag latched after input has been disabled or Ball is off.
            self.ball_dragging = false;
            self.ball_velocity = egui::Vec2::ZERO;
            self.ball_angular_velocity = 0.0;
            self.ball_time_accumulator = 0.0;
        }
        if self.ball_throw_dragging && (self.click_through || !self.ball_mode) {
            self.ball_throw_dragging = false;
            self.ball_throw_velocity = egui::Vec2::ZERO;
        }
        self.update_pass_opacity(&ctx);
        self.update_dvd_position(&ctx);
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        let viewport_rect = ui.max_rect();
        self.update_animation_texture(&ctx, tab_id);
        let texture = self.display_texture.clone();

        ui.painter().rect_filled(
            viewport_rect,
            egui::CornerRadius::ZERO,
            egui::Color32::TRANSPARENT,
        );
        // The native window is sized to the source aspect ratio. Painting into
        // the full client rect therefore wraps the image without letterboxing.
        // When decoded pixels are available, opacity has already been applied
        // to alpha alone. Keep the tint white so RGB channels are never darkened.
        // Fallback texture paths still use egui's premultiplied tint handling.
        let fallback_tint = if self.source_pixels.is_none() && self.animation.is_none() {
            egui::Color32::from_white_alpha(opacity_to_alpha(self.displayed_opacity_percent))
        } else {
            egui::Color32::WHITE
        };
        self.paint_image(ui, &texture, viewport_rect, fallback_tint);
        if self.show_bounds {
            let color = egui::Color32::from_rgba_unmultiplied(255, 190, 70, 235);
            let rect = viewport_rect.shrink(1.0);
            let painter = ui.painter();
            painter.line_segment([rect.left_top(), rect.right_top()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.right_top(), rect.right_bottom()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.right_bottom(), rect.left_bottom()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.left_bottom(), rect.left_top()], egui::Stroke::new(2.0_f32, color));
        }

        if !self.click_through {
            // Partition the client area into explicit move regions. The previous
            // full-window drag widget overlapped all resize handles; although the
            // code tried to suppress StartDrag by checking the pointer afterward,
            // egui could still capture the pointer for that full-surface widget.
            // Non-overlapping regions make move, resize, and toolbar gestures
            // mutually exclusive before input ownership is assigned.
            let corner = RESIZE_CORNER
                .min(viewport_rect.width() * 0.25)
                .min(viewport_rect.height() * 0.25);
            // On extremely panoramic/portrait images the aspect-preserving
            // window can be shorter than the normal 9-point edge target. Scale
            // edge depth down to the corner size so horizontal/vertical handles
            // still meet cleanly instead of overlapping one another.
            let edge = RESIZE_EDGE.min(corner);
            let left = viewport_rect.left();
            let right = viewport_rect.right();
            let top = viewport_rect.top();
            let bottom = viewport_rect.bottom();
            let center_left = left + corner;
            let center_right = right - corner;
            let center_top = top + corner;
            let center_bottom = bottom - corner;
            let controls_fit = viewport_rect.height() >= 48.0;
            let move_regions = if controls_fit {
                let bar = Self::control_bar_rect(viewport_rect);
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center_above_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, bar.top()),
                        ),
                    ),
                    (
                        "center_left_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.top()),
                            egui::pos2(bar.left(), bar.bottom()),
                        ),
                    ),
                    (
                        "center_right_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(bar.right(), bar.top()),
                            egui::pos2(center_right, bar.bottom()),
                        ),
                    ),
                    (
                        "center_below_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.bottom()),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            } else {
                // Very short aspect-ratio windows cannot physically fit the
                // 30-point hover toolbar. Keep the whole non-resize interior
                // draggable and leave window actions available in the tab panel.
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            };
            for (name, rect) in move_regions {
                if !rect.is_positive() {
                    continue;
                }
                let drag = ui.interact(
                    rect,
                    egui::Id::unique(("jpeg_viewer_popout_move", tab_id, name)),
                    egui::Sense::drag(),
                );

                // Left-drag remains a native move operation. The compositor owns
                // the position while dragging, so no physics position commands
                // are sent until the gesture ends.
                if drag.drag_started_by(egui::PointerButton::Primary) {
                    if self.ball_mode {
                        self.ball_dragging = true;
                        self.ball_velocity = egui::Vec2::ZERO;
                        self.ball_throw_velocity = egui::Vec2::ZERO;
                        self.ball_throw_dragging = false;
                        self.ball_time_accumulator = 0.0;
                    } else if self.dvd_mode {
                        self.dvd_dragging = true;
                    }
                    self.expected_native_position = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    #[cfg(target_os = "linux")]
                    crate::platform::wayland_pointer_recovery::after_native_window_operation(&ctx);
                }
                if self.ball_mode
                    && self.ball_dragging
                    && drag.drag_stopped_by(egui::PointerButton::Primary)
                {
                    self.ball_dragging = false;
                    self.ball_velocity = egui::Vec2::ZERO;
                    self.ball_angular_velocity = 0.0;
                    self.ball_time_accumulator = 0.0;
                    self.last_dvd_tick = Instant::now();
                }
                if self.dvd_mode
                    && self.dvd_dragging
                    && drag.drag_stopped_by(egui::PointerButton::Primary)
                {
                    // Capture the final compositor position before disabling the
                    // drag exception in synchronize_native_position().
                    self.synchronize_native_position(&ctx);
                    self.dvd_dragging = false;
                    self.last_dvd_tick = Instant::now();
                }

                // Right-drag is deliberately not a native window drag: sample
                // movement in the stationary client area, then launch the ball
                // in the same direction when the secondary button is released.
                if self.ball_mode
                    && !self.ball_dragging
                    && self.resize_gesture.is_none()
                    && drag.drag_started_by(egui::PointerButton::Secondary)
                {
                    self.ball_throw_dragging = true;
                    self.ball_throw_velocity = egui::Vec2::ZERO;
                    self.ball_velocity = egui::Vec2::ZERO;
                    self.ball_angular_velocity = 0.0;
                    self.ball_time_accumulator = 0.0;
                }
                let throw_drag_stopped = drag.drag_stopped_by(egui::PointerButton::Secondary);
                if self.ball_mode
                    && self.ball_throw_dragging
                    && (drag.dragged_by(egui::PointerButton::Secondary) || throw_drag_stopped)
                {
                    // Include the final pointer delta on the release frame too;
                    // otherwise a quick flick followed by release can lose its
                    // last movement sample before the launch velocity is applied.
                    let delta = drag.drag_delta();
                    if delta.length_sq() > 0.01 {
                        let frame_delta = ctx
                            .input(|input| input.stable_dt)
                            .clamp(1.0 / 240.0, 0.05);
                        let sample = clamp_vector_length(delta / frame_delta, 2800.0);
                        // A small low-pass filter avoids one-frame pointer-event
                        // spikes while preserving the user's overall throw.
                        self.ball_throw_velocity = self.ball_throw_velocity * 0.35 + sample * 0.65;
                        self.ball_throw_velocity =
                            clamp_vector_length(self.ball_throw_velocity, 2800.0);
                    } else {
                        // A pause before release should reduce the throw instead
                        // of reusing an old peak pointer speed indefinitely.
                        let frame_delta = ctx
                            .input(|input| input.stable_dt)
                            .clamp(1.0 / 240.0, 0.05);
                        self.ball_throw_velocity *= (-frame_delta / 0.12).exp();
                    }
                }
                if self.ball_mode && self.ball_throw_dragging && throw_drag_stopped {
                    self.finish_ball_throw();
                }
                if drag.hovered() && self.ball_mode {
                    drag.clone().on_hover_text(
                        "Left-drag moves the pop-out · Right-drag throws the ball",
                    );
                }
            }
            if self.ball_throw_dragging
                && !ctx.input(|input| input.pointer.button_down(egui::PointerButton::Secondary))
            {
                // Also finish when the pointer is released outside every move
                // region (or the release frame is no longer owned by its widget).
                self.finish_ball_throw();
            }
            if self.ball_dragging
                && !ctx.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
            {
                // Native StartDrag can move the pointer outside this viewport;
                // fall back to global button state so Ball mode cannot stay paused.
                self.ball_dragging = false;
                self.ball_velocity = egui::Vec2::ZERO;
                self.ball_angular_velocity = 0.0;
                self.ball_time_accumulator = 0.0;
                self.last_dvd_tick = Instant::now();
            }
            if self.dvd_dragging
                && !ctx.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
            {
                // Also recover if the native move ends outside this viewport.
                self.synchronize_native_position(&ctx);
                self.dvd_dragging = false;
                self.last_dvd_tick = Instant::now();
            }

            let pointer_hovering_window = ctx
                .pointer_hover_pos()
                .is_some_and(|position| viewport_rect.contains(position));
            // Keep resize hit zones registered while pointer is inside or a drag
            // is active. The hover toolbar appears only while pointer is inside.
            self.render_hover_controls(ui, viewport_rect, tab_id, pointer_hovering_window);
        }
        let close_requested = close_requested || self.close_requested;
        if close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        close_requested
    }

}
