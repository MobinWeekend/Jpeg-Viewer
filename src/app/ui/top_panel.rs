use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }

                ui.label(format!(
                    "Zoom: {}%",
                    (self.zoom * 100.0).round().max(1.0) as i32
                ));

                // Cache radius control
                ui.add_space(10.0);
                ui.label("Radius:");
                let mut radius = self.cache_radius;
                if ui
                    .add(
                        egui::DragValue::new(&mut radius)
                            .range(1..=100)
                            .speed(1)
                            .prefix(" "),
                    )
                    .changed()
                {
                    if radius != self.cache_radius {
                        self.update_cache_radius(radius);
                        // Save to settings
                        let _ = self.settings_manager.update(|settings| {
                            settings.cache_radius = radius;
                        });
                        // Trigger preload with new radius
                        if !self.image_entries.is_empty() && !self.b_is_loading {
                            self.preload_adjacent_images(ctx);
                        }
                    }
                }
                ui.label(" | ");

                // Show cache info with delta info
                let total_images = self.image_entries.len();
                let cached_count = self.image_cache.len();
                let cache_range = self.get_cache_range();
                let target_count = (cache_range * 2 + 1).min(total_images);
                ui.label(format!(
                    "Cache: {}/{} (r:{}, Δ:{})",
                    cached_count, target_count, self.cache_radius, self.delta_threshold
                ));

                // GIF controls
                if self.is_gif {
                    if let Some(gif) = &mut self.gif_animation {
                        // Check if it's an animated GIF (more than 1 frame)
                        if gif.is_animated() {
                            ui.add_space(10.0);
                            ui.label("GIF:");

                            if ui.button(if gif.is_playing { "⏸" } else { "▶" }).clicked() {
                                gif.toggle_play();
                            }

                            ui.label("Speed:");
                            let mut speed = gif.speed_multiplier;
                            let speed_slider = egui::Slider::new(&mut speed, 0.1..=10.0)
                                .logarithmic(true)
                                .text("x")
                                .smallest_positive(0.1)
                                .step_by(0.01);

                            if ui.add(speed_slider).changed() {
                                gif.set_speed(speed);
                            }

                            ui.label(format!(
                                "Frame {}/{}",
                                gif.get_current_frame_index() + 1,
                                gif.frame_count()
                            ));

                            // Show loading message for GIF preview
                            if self.is_preview {
                                ui.label("⏳ Loading GIF...");
                            }

                            ui.add_space(10.0);
                            ui.label("|");
                        } else {
                            // Single frame GIF or still loading
                            ui.add_space(10.0);
                            ui.label("GIF:");

                            // Show loading message for GIF preview
                            if self.is_preview {
                                ui.label("⏳ Loading GIF...");
                            } else {
                                ui.label("Static");
                            }

                            ui.add_space(10.0);
                            ui.label("|");
                        }
                    }
                }

                if ui
                    .checkbox(&mut self.b_ctrl_invert, "Scroll Zoom")
                    .changed()
                {
                    let _ = self.settings_manager.update(|settings| {
                        settings.b_ctrl_invert = self.b_ctrl_invert;
                    });
                }

                //My very first option! :)
                if !self.b_ctrl_invert {
                    ui.label(" | Scroll to navigate & ctrl + Scroll to Zoom | ");
                } else {
                    ui.label(" | ctrl + Scroll to navigate & Scroll to Zoom | ");
                }

                //ui for quality of filtering
                ui.add_space(10.0);
                ui.label("Filter:");

                // Get current filter value
                let mut filter = self.settings_manager.get().texture_filter.clone();

                // Show combo box
                egui::ComboBox::from_label("")
                    .selected_text(&filter)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");
                        ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");
                        ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
                    });

                // Check if filter changed by comparing with stored value
                let current_filter = self.settings_manager.get().texture_filter.clone();
                if filter != current_filter {
                    // Save the new setting
                    let _ = self.settings_manager.update(|settings| {
                        settings.texture_filter = filter.clone();
                    });
                    // Reload current image to apply the new filter
                    if !self.image_entries.is_empty() {
                        self.load_current_image_with_cache();
                    }
                }

                // Loading indicator for full resolution
                if self.b_is_loading_full {
                    ui.label("Loading...");
                }
            });
        });
    }
}
