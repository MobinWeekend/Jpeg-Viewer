use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Open button
                if ui.button("📂 Open").clicked() {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }

                ui.add_space(10.0);

                // Zoom display
                ui.label(format!(
                    "Zoom: {}%",
                    (self.zoom * 100.0).round().max(1.0) as i32
                ));

                ui.add_space(10.0);

                // Cache info (compact)
                let total_images = self.image_entries.len();
                let cached_count = self.image_cache.len();
                let cache_range = self.get_cache_range();
                let target_count = (cache_range * 2 + 1).min(total_images);
                ui.label(format!(
                    "📦 {}/{}",
                    cached_count, target_count
                ));

                ui.add_space(10.0);

                // GIF controls (compact)
                if self.is_gif {
                    if let Some(gif) = &mut self.gif_animation {
                        if gif.is_animated() {
                            ui.add_space(10.0);
                            if ui.button(if gif.is_playing { "⏸" } else { "▶" }).clicked() {
                                gif.toggle_play();
                            }

                            ui.label(format!(
                                " {}/{}",
                                gif.get_current_frame_index() + 1,
                                gif.frame_count()
                            ));

                            if self.is_preview {
                                ui.label("⏳");
                            }

                            ui.add_space(10.0);
                        }
                    }
                }

                // Scroll zoom indicator
                if !self.b_ctrl_invert {
                    ui.label("🔍 Scroll to navigate | Ctrl+Scroll to zoom");
                } else {
                    ui.label("🔍 Scroll to zoom | Ctrl+Scroll to navigate");
                }

                ui.add_space(10.0);

                // Settings button
                if ui.button("⚙️ Settings").clicked() {
                    self.toggle_settings_menu();
                }

                ui.add_space(5.0);

                // Loading indicator
                if self.b_is_loading_full {
                    ui.label("⏳ Loading...");
                }
            });
        });
    }
}