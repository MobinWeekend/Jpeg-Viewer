use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_settings_menu(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings_menu;
        let mut close_requested = false;
        
        egui::Window::new("")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([400.0, 520.0])
            .min_size([350.0, 400.0])
            .max_size([600.0, 700.0])
            .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 10.0))
            .open(&mut open)
            .show(ctx, |ui| {
                // ========== CUSTOM TITLE BAR ==========
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new("⚙️ Settings").size(18.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        let close_btn = egui::Button::new("✕")
                            .fill(egui::Color32::TRANSPARENT)
                            .frame(false)
                            .sense(egui::Sense::click());
                            
                        if ui
                            .add(close_btn)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            close_requested = true;
                        }
                    });
                });
                
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(6.0);

                // ========== SCROLLABLE CONTENT ==========
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        
                        // ========== GENERAL SETTINGS ==========
                        ui.collapsing(
                            egui::RichText::new("📋 General").size(15.0),
                            |ui| {
                                ui.add_space(4.0);
                                
                                // Scroll Zoom invert
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui
                                        .checkbox(&mut self.b_ctrl_invert, "Invert Scroll Zoom")
                                        .changed()
                                    {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.b_ctrl_invert = self.b_ctrl_invert;
                                        });
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(
                                        egui::Color32::GRAY,
                                        "(Ctrl+Scroll to zoom)"
                                    );
                                });

                                ui.add_space(4.0);

                                // Texture Filter
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Texture Filter:");
                                    ui.add_space(8.0);
                                    let mut filter = self.settings_manager.get().texture_filter.clone();
                                    egui::ComboBox::from_label("")
                                        .selected_text(&filter)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");
                                            ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");
                                            ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
                                        });

                                    let current_filter = self.settings_manager.get().texture_filter.clone();
                                    if filter != current_filter {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.texture_filter = filter.clone();
                                        });
                                        if !self.image_entries.is_empty() {
                                            self.load_current_image_with_cache();
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        );

                        ui.add_space(4.0);

                        // ========== CACHE SETTINGS ==========
                        ui.collapsing(
                            egui::RichText::new("💾 Cache Settings").size(15.0),
                            |ui| {
                                ui.add_space(4.0);
                                
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Cache Radius:");
                                    ui.add_space(8.0);
                                    let mut radius = self.cache_radius;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut radius)
                                                .range(1..=100)
                                                .speed(1)
                                        )
                                        .changed()
                                    {
                                        if radius != self.cache_radius {
                                            self.update_cache_radius(radius);
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.cache_radius = radius;
                                            });
                                            if !self.image_entries.is_empty() && !self.b_is_loading {
                                                self.preload_adjacent_images();
                                            }
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, format!("({})", radius));
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Delta Factor:");
                                    ui.add_space(8.0);
                                    let mut factor = self.cache_delta_factor;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut factor)
                                                .range(0.1..=1.0)
                                                .speed(0.05)
                                        )
                                        .changed()
                                    {
                                        if factor != self.cache_delta_factor {
                                            self.cache_delta_factor = factor;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.cache_delta_factor = factor;
                                            });
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, format!("({:.2})", factor));
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Max Tasks:");
                                    ui.add_space(8.0);
                                    let mut tasks = self.max_cache_task;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut tasks)
                                                .range(1..=10)
                                                .speed(1)
                                        )
                                        .changed()
                                    {
                                        if tasks != self.max_cache_task {
                                            self.max_cache_task = tasks;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.max_cache_task = tasks;
                                            });
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, format!("({})", tasks));
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Preload Throttle:");
                                    ui.add_space(8.0);
                                    let mut throttle = self.settings_manager.get().preload_throttle_ms;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut throttle)
                                                .range(10..=1000)
                                                .speed(10)
                                        )
                                        .changed()
                                    {
                                        if throttle != self.settings_manager.get().preload_throttle_ms {
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.preload_throttle_ms = throttle;
                                            });
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, format!("({} ms)", throttle));
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Navigation Pause:");
                                    ui.add_space(8.0);
                                    let mut pause = self.settings_manager.get().navigation_pause_ms;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut pause)
                                                .range(100..=5000)
                                                .speed(100)
                                        )
                                        .changed()
                                    {
                                        if pause != self.settings_manager.get().navigation_pause_ms {
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.navigation_pause_ms = pause;
                                                self.navigation_pause_duration = std::time::Duration::from_millis(pause);
                                            });
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, format!("({} ms)", pause));
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);

                                // Cache info
                                let total_images = self.image_entries.len();
                                let cached_count = self.image_cache.len();
                                let cache_range = self.get_cache_range();
                                let target_count = (cache_range * 2 + 1).min(total_images);
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        egui::Color32::LIGHT_BLUE,
                                        format!(
                                            "📊 {}/{} images cached (Radius: {})",
                                            cached_count, target_count, self.cache_radius
                                        )
                                    );
                                });

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui
                                        .button(egui::RichText::new("🗑️ Clear Cache").size(13.0))
                                        .clicked()
                                    {
                                        self.image_cache.clear();
                                        self.preloading_indices.clear();
                                        self.preload_tasks.clear();
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        );

                        ui.add_space(4.0);

                        // ========== FRAME LIMITER SETTINGS ==========
                        ui.collapsing(
                            egui::RichText::new("🎮 Frame Limiter").size(15.0),
                            |ui| {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        egui::Color32::GRAY,
                                        "Controls how many frames per second the app renders."
                                    );
                                });
                                ui.add_space(6.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Max FPS:");
                                    ui.add_space(8.0);
                                    let mut max_fps = self.max_fps;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut max_fps)
                                                .range(0.0..=120.0)
                                                .speed(1.0)
                                        )
                                        .changed()
                                    {
                                        if max_fps != self.max_fps {
                                            self.max_fps = max_fps;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.max_fps = max_fps;
                                            });
                                            self.load_frame_limiter_settings();
                                        }
                                    }
                                    ui.add_space(4.0);
                                    if max_fps == 0.0 {
                                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Unlimited");
                                    } else {
                                        ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} FPS", max_fps));
                                    }
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Idle FPS:");
                                    ui.add_space(8.0);
                                    let mut idle_fps = self.idle_fps_limit;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut idle_fps)
                                                .range(0.0..=60.0)
                                                .speed(1.0)
                                        )
                                        .changed()
                                    {
                                        if idle_fps != self.idle_fps_limit {
                                            self.idle_fps_limit = idle_fps;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.idle_fps_limit = idle_fps;
                                            });
                                            self.load_frame_limiter_settings();
                                        }
                                    }
                                    ui.add_space(4.0);
                                    if idle_fps == 0.0 {
                                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Unlimited");
                                    } else {
                                        ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} FPS", idle_fps));
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, "(when idle)");
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Idle Timeout:");
                                    ui.add_space(8.0);
                                    let mut timeout = self.idle_timeout_ms;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut timeout)
                                                .range(100..=10000)
                                                .speed(100)
                                        )
                                        .changed()
                                    {
                                        if timeout != self.idle_timeout_ms {
                                            self.idle_timeout_ms = timeout;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.idle_timeout_ms = timeout;
                                            });
                                            self.load_frame_limiter_settings();
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} ms", timeout));
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, "(when focused)");
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Unfocused Timeout:");
                                    ui.add_space(8.0);
                                    let mut unfocused_timeout = self.unfocused_idle_timeout_ms;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut unfocused_timeout)
                                                .range(50..=5000)
                                                .speed(50)
                                        )
                                        .changed()
                                    {
                                        if unfocused_timeout != self.unfocused_idle_timeout_ms {
                                            self.unfocused_idle_timeout_ms = unfocused_timeout;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.unfocused_idle_timeout_ms = unfocused_timeout;
                                            });
                                            self.load_frame_limiter_settings();
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} ms", unfocused_timeout));
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, "(when unfocused)");
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label("Unfocused Idle FPS:");
                                    ui.add_space(8.0);
                                    let mut unfocused_fps = self.unfocused_idle_fps_limit;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut unfocused_fps)
                                                .range(0.0..=60.0)
                                                .speed(1.0)
                                        )
                                        .changed()
                                    {
                                        if unfocused_fps != self.unfocused_idle_fps_limit {
                                            self.unfocused_idle_fps_limit = unfocused_fps;
                                            let _ = self.settings_manager.update(|settings| {
                                                settings.unfocused_idle_fps_limit = unfocused_fps;
                                            });
                                            self.load_frame_limiter_settings();
                                        }
                                    }
                                    ui.add_space(4.0);
                                    if unfocused_fps == 0.0 {
                                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Unlimited");
                                    } else {
                                        ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} FPS", unfocused_fps));
                                    }
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::GRAY, "(when unfocused)");
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);

                                // Show current state
                                let state_text = if self.is_animating {
                                    "🎬 Animating"
                                } else if self.b_is_loading {
                                    "⏳ Loading"
                                } else if self.is_idle {
                                    "💤 Idle"
                                } else {
                                    "🔄 Active"
                                };
                                
                                let idle_text = if self.idle_fps_limit == 0.0 {
                                    "Unlimited".to_string()
                                } else {
                                    format!("{} FPS", self.idle_fps_limit)
                                };
                                
                                let focus_text = if ctx.input(|i| i.viewport().focused).unwrap_or(false) {
                                    "✅ Focused"
                                } else {
                                    "❌ Unfocused"
                                };
                                
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        egui::Color32::LIGHT_YELLOW,
                                        format!(
                                            "Current: {} | Idle: {} | Focused: {}",
                                            state_text, idle_text, focus_text
                                        )
                                    );
                                });
                                ui.add_space(4.0);
                            }
                        );

                        ui.add_space(4.0);

                        // ========== WINDOW SETTINGS ==========
                        ui.collapsing(
                            egui::RichText::new("🪟 Window").size(15.0),
                            |ui| {
                                ui.add_space(4.0);
                                
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui
                                        .button(egui::RichText::new("🔄 Reset Window Position").size(13.0))
                                        .clicked()
                                    {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.window_pos = None;
                                        });
                                    }
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui
                                        .button(egui::RichText::new("🔄 Reset Window Size").size(13.0))
                                        .clicked()
                                    {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.window_size = None;
                                        });
                                    }
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);

                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    if ui
                                        .button(
                                            egui::RichText::new("⚠️ Reset All Settings to Default")
                                                .color(egui::Color32::from_rgb(255, 150, 150))
                                                .size(13.0)
                                        )
                                        .clicked()
                                    {
                                        let default_settings = crate::settings::AppSettings::default();
                                        let _ = self.settings_manager.update(|settings| {
                                            *settings = default_settings;
                                        });
                                        // Reload all settings
                                        self.load_frame_limiter_settings();
                                        self.b_ctrl_invert = self.settings_manager.get().b_ctrl_invert;
                                        self.cache_radius = self.settings_manager.get().cache_radius;
                                        self.cache_delta_factor = self.settings_manager.get().cache_delta_factor;
                                        self.max_cache_task = self.settings_manager.get().max_cache_task;
                                        self.navigation_pause_duration = std::time::Duration::from_millis(
                                            self.settings_manager.get().navigation_pause_ms
                                        );
                                        // Clear cache and reload
                                        self.image_cache.clear();
                                        self.preloading_indices.clear();
                                        self.preload_tasks.clear();
                                        if !self.image_entries.is_empty() {
                                            self.load_current_image_with_cache();
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        );

                        ui.add_space(8.0);
                    });
            });
            
        // Update the actual state after the window closes
        if close_requested {
            self.show_settings_menu = false;
        } else {
            // The window might have been closed via X button, check the open state
            self.show_settings_menu = open;
        }
    }

    pub fn toggle_settings_menu(&mut self) {
        self.show_settings_menu = !self.show_settings_menu;
    }
}