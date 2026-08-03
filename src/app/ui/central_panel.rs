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

            // Check if we have a texture
            let has_texture = self.texture.is_some();

            if !has_texture {
                // No texture, handle loading states
                if self.is_gif {
                    if self.b_is_loading {
                        self.render_loading_ui(ui, "Loading GIF...");
                    } else {
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
            }

            // We have a texture - render the image
            let texture = self.texture.as_ref().unwrap();
            let texture_size = texture.size_vec2();
            let center = ui.available_rect_before_wrap().center();
            let image_rect = self.get_image_rect(texture_size, center);

            // Allocate the full available space for interaction
            let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());

            // Paint the image
            let painter = ui.painter();
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                egui::Color32::WHITE,
            );

            // Draw loading indicator overlay for GIF preview
            if self.is_gif && self.is_preview {
                let overlay_rect = ui.available_rect_before_wrap();
                
                // Semi-transparent overlay
                painter.rect_filled(
                    overlay_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 128),
                );
                
                // Draw loading UI directly with painter (no allocation)
                self.draw_loading_overlay(&painter, overlay_rect, "Loading full GIF...");
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
                    .strong(),
            );
            ui.add_space(8.0);

            ui.label(egui::RichText::new(error).size(16.0));
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
        // Calculate content size
        let spinner_size = 48.0;
        let text_height = 24.0;
        let padding = 32.0;
        let content_width = 200.0;
        let content_height = spinner_size + 16.0 + text_height + padding * 2.0;

        // Get the available rect
        let available = ui.available_rect_before_wrap();
        let center = available.center();

        // Create the content rect centered in available space
        let rect = egui::Rect::from_center_size(center, egui::vec2(content_width, content_height));

        // Allocate the space
        let _response = ui.allocate_rect(rect, egui::Sense::hover());

        let painter = ui.painter();

        // Background only around the content
        painter.rect_filled(rect, 12.0, ui.style().visuals.panel_fill);


        // Loading spinner
        let time = ui.input(|i| i.time);
        let angle = (time * 3.0) as f32;
        let radius = (spinner_size * 0.35) as f32;
        let segments = 8;
        let spinner_center = egui::pos2(center.x, center.y - (text_height / 2.0 + 8.0));

        for i in 0..segments {
            let angle_offset = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let alpha = ((0.3 + 0.7 * ((time as f32 * 2.0 + angle_offset).sin() * 0.5 + 0.5))
                * 255.0) as u8;
            let x = spinner_center.x + radius * (angle + angle_offset).cos();
            let y = spinner_center.y + radius * (angle + angle_offset).sin();
            let size = 6.0;
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(size, size)),
                3.0,
                egui::Color32::from_rgba_premultiplied(100, 150, 255, alpha),
            );
        }

        // Loading text
        let font_id = egui::FontId::proportional(18.0);
        let text_color = ui.style().visuals.text_color();
        let galley = painter.layout(message.to_string(), font_id, text_color, f32::INFINITY);
        let text_pos = egui::pos2(
            center.x - galley.rect.width() / 2.0,
            center.y + spinner_size / 2.0 + 16.0 - (text_height / 2.0 + 8.0),
        );
        painter.galley(text_pos, galley, text_color);
    }

    /// Draw loading overlay directly with painter (no UI allocation)
    fn draw_loading_overlay(&self, painter: &egui::Painter, rect: egui::Rect, message: &str) {
        let spinner_size = 48.0;
        let text_height = 24.0;
        let padding = 32.0;
        let content_width = 200.0;
        let content_height = spinner_size + 16.0 + text_height + padding * 2.0;

        let center = rect.center();

        // Create the content rect centered in available space
        let content_rect = egui::Rect::from_center_size(
            center,
            egui::vec2(content_width, content_height),
        );

        // Background only around the content
        painter.rect_filled(
            content_rect,
            12.0,
            painter.ctx().style().visuals.panel_fill,
        );


        // Loading spinner
        let time = painter.ctx().input(|i| i.time);
        let angle = (time * 3.0) as f32;
        let radius = (spinner_size * 0.35) as f32;
        let segments = 8;
        let spinner_center = egui::pos2(center.x, center.y - (text_height / 2.0 + 8.0));

        for i in 0..segments {
            let angle_offset = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let alpha = ((0.3 + 0.7 * ((time as f32 * 2.0 + angle_offset).sin() * 0.5 + 0.5))
                * 255.0) as u8;
            let x = spinner_center.x + radius * (angle + angle_offset).cos();
            let y = spinner_center.y + radius * (angle + angle_offset).sin();
            let size = 6.0;
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(size, size)),
                3.0,
                egui::Color32::from_rgba_premultiplied(100, 150, 255, alpha),
            );
        }

        // Loading text
        let font_id = egui::FontId::proportional(18.0);
        let text_color = painter.ctx().style().visuals.text_color();
        let galley = painter.layout(message.to_string(), font_id, text_color, f32::INFINITY);
        let text_pos = egui::pos2(
            center.x - galley.rect.width() / 2.0,
            center.y + spinner_size / 2.0 + 16.0 - (text_height / 2.0 + 8.0),
        );
        painter.galley(text_pos, galley, text_color);
    }

    fn render_image_counter(&self, ui: &mut egui::Ui) {
        let total = self.image_entries.len();
        if total > 0 {
            let text = format!("{}/{}", self.current_index + 1, total);
            let font_id = egui::FontId::proportional(14.0);
            let galley = ui.painter().layout(
                text,
                font_id,
                ui.style().visuals.text_color(),
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
                ui.style().visuals.panel_fill,
            );

            ui.painter().galley(pos, galley, ui.style().visuals.text_color());
        }
    }
}