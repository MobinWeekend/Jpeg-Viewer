use super::*;

impl PopoutManager {
    /// Render a settings-style right sidebar for the active Image tab. The
    /// structure intentionally mirrors Settings: a docked panel, compact header,
    /// grouped collapsible sections, a single vertical scroll area, and footer.
    /// JVP preset actions remain available even while the native pop-out is closed.
    pub(crate) fn render_settings_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        tab_id: u64,
        texture: Option<egui::TextureHandle>,
        animation: Option<GifAnimation>,
        source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
        image_entry: Option<ImageEntry>,
    ) {
        let mut open_requested = false;
        let mut close_requested = false;
        let mut request_save = false;
        let mut request_load = false;
        let mut close_sidebar = false;
        let monitors = crate::platform::monitors::available_monitors();
        let window_state = self.windows.get(&tab_id).cloned();
        let is_open = window_state.is_some();
        let mut window_guard = window_state.as_ref().and_then(|state| state.lock().ok());
        let mut preset_snapshot = self.presets.get(&tab_id).cloned().unwrap_or_default();
        let status_message = self.status_message.clone();

        ui.horizontal(|ui| {
            ui.heading("Pop-out");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("×")
                    .on_hover_text("Close Pop-out settings")
                    .clicked()
                {
                    close_sidebar = true;
                }
            });
        });
        ui.label(
            egui::RichText::new("Floating window, display, movement and pointer behavior")
                .small()
                .weak(),
        );
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |card| {
            let closed_color = card.visuals().weak_text_color();
            card.horizontal(|card| {
                card.vertical(|status| {
                    status.label(
                        egui::RichText::new(if is_open { "● RUNNING" } else { "○ CLOSED" })
                            .strong()
                            .color(if is_open {
                                egui::Color32::from_rgb(52, 166, 112)
                            } else {
                                closed_color
                            }),
                    );
                    status.label(
                        egui::RichText::new(if is_open {
                            "Separate image window"
                        } else {
                            "Open this image in its own window"
                        })
                        .small()
                        .weak(),
                    );
                });
                card.with_layout(egui::Layout::right_to_left(egui::Align::Center), |card| {
                    if is_open {
                        if card.button("Close").clicked() {
                            close_requested = true;
                        }
                    } else if card
                        .add_enabled(texture.is_some(), egui::Button::new("Open pop-out"))
                        .clicked()
                    {
                        open_requested = true;
                    }
                });
            });
            card.add_space(4.0);
            card.horizontal(|card| {
                let button_width = ((card.available_width() - 4.0) * 0.5).max(72.0);
                if card
                    .add_sized(
                        egui::vec2(button_width, 26.0),
                        egui::Button::new("Save preset"),
                    )
                    .clicked()
                {
                    request_save = true;
                }
                if card
                    .add_sized(
                        egui::vec2(button_width, 26.0),
                        egui::Button::new("Load preset"),
                    )
                    .clicked()
                {
                    request_load = true;
                }
            });
        });
        ui.separator();

        const FOOTER_HEIGHT: f32 = 38.0;
        let scroll_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
        egui::ScrollArea::vertical()
            .id_salt(("popout_settings_scroll", tab_id))
            .auto_shrink([false; 2])
            .max_height(scroll_height)
            .show(ui, |scroll_ui| {
                scroll_ui.add_space(4.0);
                if let Some(window) = window_guard.as_deref_mut() {
                    egui::CollapsingHeader::new(
                        egui::RichText::new("▣  Window").strong(),
                    )
                    .id_salt(("popout_settings_window", tab_id))
                    .default_open(true)
                    .show(scroll_ui, |section| {
                        section.add_space(4.0);
                        crate::ui_theme::switch(
                            section,
                            &mut window.click_through,
                            "Pass clicks through the pop-out (PASS)",
                        );
                        if window.click_through {
                            section.label(
                                egui::RichText::new(
                                    "PASS disables pop-out input, including dragging and hover controls.",
                                )
                                .small()
                                .weak(),
                            );
                        }
                        crate::ui_theme::switch(
                            section,
                            &mut window.show_bounds,
                            "Show window bounds",
                        );
                        if crate::ui_theme::switch(
                            section,
                            &mut window.remove_os_decorations,
                            "Remove OS window decorations",
                        ) {
                            ctx.send_viewport_cmd_to(
                                window.viewport_id,
                                egui::ViewportCommand::Decorations(!window.remove_os_decorations),
                            );
                        }
                        section.label(
                            egui::RichText::new(
                                "Hides the desktop title bar, borders and window buttons. Some compositors may enforce their own decoration policy.",
                            )
                            .small()
                            .weak(),
                        );
                        if crate::ui_theme::switch(
                            section,
                            &mut window.remove_os_window_effects,
                            "Remove OS rounded corners, shadows and borders",
                        ) {
                            window.native_window_effects_applied = None;
                            ctx.request_repaint();
                        }
                        section.label(
                            egui::RichText::new(
                                "Requests square corners and no native shadow, border or system backdrop. Desktop environments may override these hints.",
                            )
                            .small()
                            .weak(),
                        );
                        if crate::ui_theme::switch(
                            section,
                            &mut window.stay_on_top,
                            "Stay on top",
                        ) && window.stay_on_top
                        {
                            window.stay_on_bottom = false;
                        }
                        if crate::ui_theme::switch(
                            section,
                            &mut window.stay_on_bottom,
                            "Stay on bottom",
                        ) && window.stay_on_bottom
                        {
                            window.stay_on_top = false;
                        }

                        let min_scale = window.minimum_size().x / window.base_size.x.max(1.0);
                        let max_scale = (window.maximum_size().x / window.base_size.x.max(1.0))
                            .max(min_scale + 0.01);
                        let size_response = section.add(
                            crate::ui_theme::BorderedSlider::new(
                                egui::Slider::new(
                                    &mut window.pending_size_scale,
                                    min_scale..=max_scale,
                                )
                                .text("Size")
                                .suffix("×")
                                .logarithmic(true),
                            ),
                        );
                        if size_response.changed()
                            && (window.pending_size_scale - window.size_scale).abs() > 0.001
                        {
                            window.size_scale = window.pending_size_scale;
                            let requested_size = window.size_for_scale(window.size_scale);
                            window.size = requested_size;
                            window.expected_native_size = Some((requested_size, Instant::now(), 0));
                            ctx.send_viewport_cmd_to(
                                window.viewport_id,
                                egui::ViewportCommand::InnerSize(requested_size),
                            );
                            ctx.request_repaint();
                        }
                        section.add(
                            crate::ui_theme::BorderedSlider::new(
                                egui::Slider::new(&mut window.opacity_percent, 0.0..=100.0)
                                    .text("Opacity")
                                    .suffix("%"),
                            ),
                        );
                        section.add_space(4.0);
                    });

                    egui::CollapsingHeader::new(
                        egui::RichText::new("▧  Display").strong(),
                    )
                    .id_salt(("popout_settings_display", tab_id))
                    .default_open(true)
                    .show(scroll_ui, |section| {
                        section.add_space(4.0);
                        section.label(egui::RichText::new("Monitor").strong());
                        let selected_monitor_label = match window.monitor_id.as_deref() {
                            None => "Active monitor".to_owned(),
                            Some(id) => monitors
                                .iter()
                                .find(|monitor| monitor.id == id)
                                .map(|monitor| monitor.display_label())
                                .unwrap_or_else(|| format!("Unavailable · {id}")),
                        };
                        let mut selected_monitor = window.monitor_id.clone();
                        egui::ComboBox::from_id_salt(("popout_monitor", tab_id))
                            .selected_text(selected_monitor_label)
                            .width(section.available_width())
                            .show_ui(section, |combo| {
                                combo.selectable_value(
                                    &mut selected_monitor,
                                    None,
                                    "Active monitor",
                                );
                                for monitor in &monitors {
                                    combo.selectable_value(
                                        &mut selected_monitor,
                                        Some(monitor.id.clone()),
                                        monitor.display_label(),
                                    );
                                }
                            });
                        if selected_monitor != window.monitor_id {
                            window.monitor_id = selected_monitor;
                            // Selecting a named display moves the pop-out there.
                            // Active monitor deliberately leaves its position alone
                            // and follows the display it currently overlaps most.
                            if let Some(monitor) = window
                                .monitor_id
                                .as_deref()
                                .and_then(|id| monitors.iter().find(|monitor| monitor.id == id))
                            {
                                window.request_outer_position(
                                    ctx,
                                    monitor.bounds().center() - window.size * 0.5,
                                );
                            }
                            window.last_dvd_tick = Instant::now();
                        }
                        let effective_monitor = if let Some(id) = window.monitor_id.as_deref() {
                            monitors
                                .iter()
                                .find(|monitor| monitor.id == id)
                                .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
                                .or_else(|| monitors.first())
                        } else {
                            let rect = egui::Rect::from_min_size(window.position, window.size);
                            crate::platform::monitors::resolve_default_monitor_at(
                                &monitors,
                                crate::platform::monitors::CURRENT_WINDOW_MONITOR,
                                Some(rect),
                            )
                        };
                        if let Some(monitor) = effective_monitor {
                            section.label(
                                egui::RichText::new(format!(
                                    "Trick bounds: {}",
                                    monitor.display_label()
                                ))
                                .small()
                                .weak(),
                            );
                        }
                        section.label(
                            egui::RichText::new(
                                "Active monitor follows the screen containing most of the pop-out. Drag it to another display and DVD/Ball movement will use that display's bounds.",
                            )
                            .small()
                            .weak(),
                        );

                        let mut angle_degrees = normalize_angle_degrees(
                            window.rotation_radians.to_degrees(),
                        );
                        if section
                            .add(crate::ui_theme::BorderedSlider::new(
                                egui::Slider::new(&mut angle_degrees, -180.0..=180.0)
                                    .text("Rotation")
                                    .suffix("°"),
                            ))
                            .changed()
                        {
                            window.rotation_radians = angle_degrees.to_radians();
                            window.dvd_rotation_target = window.rotation_radians;
                        }
                        section.add_space(4.0);
                    });

                    egui::CollapsingHeader::new(
                        egui::RichText::new("▶  Movement").strong(),
                    )
                    .id_salt(("popout_settings_movement", tab_id))
                    .default_open(true)
                    .show(scroll_ui, |section| {
                        section.add_space(4.0);
                        let movement_supported = PopoutWindow::automatic_window_movement_supported();
                        let mut selected_mode = if window.dvd_mode {
                            1_u8
                        } else if window.ball_mode {
                            2_u8
                        } else {
                            0_u8
                        };
                        let was_dvd = window.dvd_mode;
                        let was_ball = window.ball_mode;
                        section.horizontal_wrapped(|row| {
                            row.selectable_value(&mut selected_mode, 0, "Off");
                            row.add_enabled_ui(movement_supported, |row| {
                                row.selectable_value(&mut selected_mode, 1, "DVD bounce");
                                row.selectable_value(&mut selected_mode, 2, "Ball physics");
                            });
                        });
                        if movement_supported
                            && selected_mode
                                != (if was_dvd { 1 } else if was_ball { 2 } else { 0 })
                        {
                            window.dvd_mode = selected_mode == 1;
                            window.dvd_dragging = false;
                            window.ball_mode = selected_mode == 2;
                            if window.ball_mode && !was_ball {
                                window.ball_throw_dragging = false;
                                window.ball_throw_velocity = egui::Vec2::ZERO;
                                window.ball_velocity = egui::vec2(360.0, -220.0);
                                window.ball_angular_velocity = window.rolling_angular_velocity();
                                window.ball_time_accumulator = 0.0;
                                window.ball_dragging = false;
                            } else if was_ball && !window.ball_mode {
                                window.ball_throw_dragging = false;
                                window.ball_throw_velocity = egui::Vec2::ZERO;
                                window.ball_dragging = false;
                                window.ball_velocity = egui::Vec2::ZERO;
                                window.ball_angular_velocity = 0.0;
                                window.ball_time_accumulator = 0.0;
                            }
                            if window.dvd_mode && !was_dvd {
                                window.last_dvd_tick = Instant::now();
                            }
                            window.next_random_bounce = Instant::now()
                                + Duration::from_secs_f32(window.random_bounce_interval);
                        }
                        if !movement_supported {
                            section.label(
                                egui::RichText::new(
                                    "Automatic movement requires a platform with window-position control. Manual dragging remains available.",
                                )
                                .small()
                                .weak(),
                            );
                        }

                        if window.dvd_mode {
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(&mut window.dvd_speed, 0.1..=4.0)
                                        .text("DVD speed")
                                        .suffix("×"),
                                ),
                            );
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(
                                        &mut window.dvd_rotation_ease,
                                        0.1..=4.0,
                                    )
                                    .text("Rotation easing")
                                    .suffix(" s"),
                                ),
                            );
                            crate::ui_theme::switch(
                                section,
                                &mut window.dvd_circle_collision,
                                "Circular collision path",
                            );
                            section.label(egui::RichText::new("Monitor edge padding").strong());
                            for (index, label) in ["Left", "Top", "Right", "Bottom"]
                                .into_iter()
                                .enumerate()
                            {
                                section.add(
                                    crate::ui_theme::BorderedSlider::new(
                                        egui::Slider::new(
                                            &mut window.dvd_edge_offsets[index],
                                            0.0..=1000.0,
                                        )
                                        .text(label)
                                        .suffix(" pt"),
                                    ),
                                );
                            }
                        } else if window.ball_mode {
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(&mut window.ball_gravity, 0.0..=2400.0)
                                        .text("Gravity")
                                        .suffix(" px/s²"),
                                ),
                            );
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(&mut window.ball_bounciness, 0.0..=1.0)
                                        .text("Bounciness"),
                                ),
                            );
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(&mut window.ball_friction, 0.0..=5.0)
                                        .text("Surface friction"),
                                ),
                            );
                            let was_rotating = window.ball_rotate;
                            if crate::ui_theme::switch(
                                section,
                                &mut window.ball_rotate,
                                "Rotate from movement and impacts",
                            ) && window.ball_rotate && !was_rotating
                            {
                                window.ball_angular_velocity = window.rolling_angular_velocity();
                            }
                            section.label(
                                egui::RichText::new(
                                    "Right-drag the image to throw it. Left-drag moves the pop-out window.",
                                )
                                .small()
                                .weak(),
                            );
                            section.label(
                                egui::RichText::new(
                                    "Surface friction transfers linear motion into spin at collisions.",
                                )
                                .small()
                                .weak(),
                            );
                        } else {
                            section.label(
                                egui::RichText::new("Choose DVD bounce or Ball physics to enable automatic movement.")
                                    .small()
                                    .weak(),
                            );
                        }
                        section.add_space(4.0);
                    });

                    egui::CollapsingHeader::new(
                        egui::RichText::new("↗  Bounce behavior").strong(),
                    )
                    .id_salt(("popout_settings_bounce", tab_id))
                    .show(scroll_ui, |section| {
                        section.add_space(4.0);
                        crate::ui_theme::switch(
                            section,
                            &mut window.random_reflect_on_bounce,
                            "Randomize direction after a bounce",
                        );
                        crate::ui_theme::switch(
                            section,
                            &mut window.random_rotate_on_bounce,
                            "Randomize rotation after a bounce",
                        );
                        let can_random_bounce = window.dvd_mode || window.ball_mode;
                        let was_enabled = window.random_bounce_enabled;
                        section.add_enabled_ui(can_random_bounce || was_enabled, |section| {
                            if crate::ui_theme::switch(
                                section,
                                &mut window.random_bounce_enabled,
                                "Add a timed random impulse",
                            ) {
                                window.next_random_bounce = Instant::now()
                                    + Duration::from_secs_f32(
                                        window.random_bounce_interval.clamp(0.5, 30.0),
                                    );
                            }
                            section.add_enabled(
                                window.random_bounce_enabled,
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(
                                        &mut window.random_bounce_interval,
                                        0.5..=30.0,
                                    )
                                    .text("Interval")
                                    .suffix(" s"),
                                ),
                            );
                        });
                        if !can_random_bounce && !window.random_bounce_enabled {
                            section.label(
                                egui::RichText::new("Enable a movement mode to use timed impulses.")
                                    .small()
                                    .weak(),
                            );
                        }
                        section.add_space(4.0);
                    });

                    egui::CollapsingHeader::new(
                        egui::RichText::new("◉  Pointer effects").strong(),
                    )
                    .id_salt(("popout_settings_pointer", tab_id))
                    .show(scroll_ui, |section| {
                        section.add_space(4.0);
                        crate::ui_theme::switch(
                            section,
                            &mut window.pass_near_enabled,
                            "Fade in when pointer is nearby",
                        );
                        if window.pass_near_enabled {
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(
                                        &mut window.pass_near_distance,
                                        10.0..=500.0,
                                    )
                                    .text("Detection distance")
                                    .suffix(" pt"),
                                ),
                            );
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(
                                        &mut window.pass_near_opacity,
                                        0.0..=100.0,
                                    )
                                    .text("Nearby opacity")
                                    .suffix("%"),
                                ),
                            );
                        }
                        crate::ui_theme::switch(
                            section,
                            &mut window.pass_hover_enabled,
                            "Fade in when pointer is over the image",
                        );
                        if window.pass_hover_enabled {
                            section.add(
                                crate::ui_theme::BorderedSlider::new(
                                    egui::Slider::new(
                                        &mut window.pass_hover_opacity,
                                        0.0..=100.0,
                                    )
                                    .text("Hover opacity")
                                    .suffix("%"),
                                ),
                            );
                        }
                        if !window.click_through
                            && (window.pass_near_enabled || window.pass_hover_enabled)
                        {
                            section.label(
                                egui::RichText::new("Enable PASS above for pointer-triggered opacity effects.")
                                    .small()
                                    .weak(),
                            );
                        }
                        #[cfg(target_os = "linux")]
                        if !crate::platform::pointer::global_position_available()
                            && (window.pass_near_enabled || window.pass_hover_enabled)
                        {
                            section.label(
                                egui::RichText::new("Wayland cannot provide desktop-wide pointer coordinates for these effects.")
                                    .small()
                                    .weak(),
                            );
                        }
                        section.add_space(4.0);
                    });
                } else if is_open {
                    scroll_ui.label("The pop-out state is temporarily unavailable.");
                } else {
                    scroll_ui.label(
                        egui::RichText::new(
                            "Open the pop-out to configure its window, display, movement and pointer effects. You can load a saved preset before opening it.",
                        )
                        .small()
                        .weak(),
                    );
                }

                if let Some(message) = status_message.as_deref() {
                    scroll_ui.separator();
                    scroll_ui.label(
                        egui::RichText::new(message)
                            .color(crate::ui_theme::main_accent_color(ctx))
                            .small(),
                    );
                }
                scroll_ui.add_space(8.0);
            });

        ui.separator();
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(format!("JPEG Viewer v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .weak(),
            );
        });

        if let Some(window) = window_guard.as_deref() {
            preset_snapshot = preset_from_window(window);
        }
        drop(window_guard);

        if is_open {
            self.presets.insert(tab_id, preset_snapshot.clone());
        }
        if close_sidebar {
            self.settings_sidebar_tab = None;
        }
        if close_requested {
            self.close_tab(tab_id);
        } else if open_requested && let Some(texture) = texture {
            self.open_tab(tab_id, texture, animation, source_pixels, ctx);
        }

        if request_save {
            self.save_dialog = Some(SaveDialogState {
                tab_id,
                preset: preset_snapshot,
                image_entry: image_entry.clone(),
                choice: if image_entry.is_some() {
                    SaveImageChoice::Link
                } else {
                    SaveImageChoice::PresetOnly
                },
            });
        }
        if request_load
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("JPEG Viewer Pop-out preset", &["jvp"])
                .pick_file()
        {
            match jvp::load_jvp(&path) {
                Ok(document) => self.load_dialog = Some(LoadDialogState { tab_id, document }),
                Err(error) => self.status_message = Some(format!("Could not load .jvp: {error}")),
            }
        }
        self.render_jvp_dialogs(ctx);
    }
}
