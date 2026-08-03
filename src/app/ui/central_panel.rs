// src/app/ui/central_panel.rs
use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Check for error first - clone the error to avoid borrowing issues
            let error = self.image_error.clone();

            if let Some(error) = error {
                self.render_error_ui(ui, &error);
                return;
            }

            // First, check if we need to calculate fit - do this BEFORE borrowing texture
            if self.b_fit_to_window {
                if let Some(texture) = &self.texture {
                    let available = ui.available_size();
                    self.calculate_fit_zoom(texture.size_vec2(), available);
                    self.b_fit_to_window = false;
                }
            }

            // Now check if we have a texture
            let texture_size = if let Some(texture) = &self.texture {
                texture.size_vec2()
            } else {
                // No texture, handle loading states
                if self.is_gif {
                    if self.b_is_loading {
                        self.render_loading_ui(ui, "Loading GIF...");
                    } else if self.texture.is_none() && self.is_gif {
                        self.render_loading_ui(ui, "Loading GIF frame...");
                    }
                } else {
                    if self.b_is_loading {
                        self.render_loading_ui(ui, "Loading image...");
                    } else {
                        self.render_welcome_ui(ctx, ui);
                    }
                }
                return;
            };

            // Now we have texture_size and can render the image
            let center = ui.available_rect_before_wrap().center();
            let image_rect = self.get_image_rect(texture_size, center);

            // Allocate the full available space for interaction
            let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());

            // Paint the image
            if let Some(texture) = &self.texture {
                let painter = ui.painter();
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    egui::Color32::WHITE,
                );
            }

            // Draw loading indicator overlay for GIFs
            if self.is_gif && self.is_preview {
                self.draw_gif_loading_overlay(&ui.painter(), &response);
            }

            // Handle mouse input on the response
            self.handle_image_mouse_input(ctx, &response);

            // Show image counter at bottom
            if !self.image_entries.is_empty() {
                self.render_image_counter(ui);
            }
        });
    }

    fn render_error_ui(&mut self, ui: &mut egui::Ui, error: &str) {
        ui.centered_and_justified(|ui| {
            ui.add_space(40.0);

            // Error icon
            ui.label(egui::RichText::new("🖼️").size(64.0));
            ui.add_space(16.0);

            ui.label(
                egui::RichText::new("Failed to Load Image")
                    .size(28.0)
                    .color(egui::Color32::from_rgb(255, 200, 200))
                    .strong(),
            );
            ui.add_space(8.0);

            ui.label(
                egui::RichText::new(error)
                    .size(16.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                // Use .min_size() on the button directly
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("🔄 Retry").size(16.0))
                            .min_size(egui::vec2(100.0, 36.0)),
                    )
                    .clicked()
                {
                    self.image_error = None;
                    self.load_current_image_with_cache();
                }

                ui.add_space(8.0);

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("⏭️ Skip").size(16.0))
                            .min_size(egui::vec2(100.0, 36.0)),
                    )
                    .clicked()
                {
                    self.navigate_images(ui.ctx(), 1);
                }
            });
        });
    }

    fn render_loading_ui(&mut self, ui: &mut egui::Ui, message: &str) {
        let rect = ui.available_rect_before_wrap();
        let center = rect.center();

        let painter = ui.painter();

        // Background
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_premultiplied(20, 20, 30, 200),
        );

        // Loading spinner
        let spinner_size = 48.0;

        // Draw spinner animation
        let time = ui.input(|i| i.time);
        let angle = (time * 3.0) as f32;
        let radius = (spinner_size * 0.35) as f32;
        let segments = 8;

        for i in 0..segments {
            let angle_offset = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let alpha = ((0.3 + 0.7 * ((time as f32 * 2.0 + angle_offset).sin() * 0.5 + 0.5))
                * 255.0) as u8;
            let x = center.x + radius * (angle + angle_offset).cos();
            let y = center.y + radius * (angle + angle_offset).sin();
            let size = 6.0;
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(size, size)),
                3.0,
                egui::Color32::from_rgba_premultiplied(100, 150, 255, alpha),
            );
        }

        // Loading text
        let font_id = egui::FontId::proportional(18.0);
        let galley = painter.layout(
            message.to_string(),
            font_id,
            egui::Color32::LIGHT_GRAY,
            f32::INFINITY,
        );
        let text_pos = egui::pos2(
            center.x - galley.rect.width() / 2.0,
            center.y + spinner_size / 2.0 + 20.0,
        );
        painter.galley(text_pos, galley, egui::Color32::LIGHT_GRAY);
    }

    fn render_image_counter(&self, ui: &mut egui::Ui) {
        let total = self.image_entries.len();
        if total > 0 {
            let text = format!("{}/{}", self.current_index + 1, total);
            let font_id = egui::FontId::proportional(14.0);
            let galley = ui.painter().layout(
                text,
                font_id,
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 150),
                f32::INFINITY,
            );

            let rect = ui.available_rect_before_wrap();
            let pos = egui::pos2(
                rect.right() - galley.rect.width() - 20.0,
                rect.bottom() - 40.0,
            );

            // Background pill
            let bg_rect = galley
                .rect
                .translate(egui::Vec2::new(pos.x, pos.y))
                .expand(10.0);
            ui.painter().rect_filled(
                bg_rect,
                20.0,
                egui::Color32::from_rgba_premultiplied(0, 0, 0, 150),
            );

            ui.painter().galley(pos, galley, egui::Color32::WHITE);
        }
    }
}
