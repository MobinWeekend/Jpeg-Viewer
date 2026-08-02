use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Check for error first - clone the error to avoid borrowing issues
            let error = self.image_error.clone();
            
            if let Some(error) = error {
                // Display error message
                ui.centered_and_justified(|ui| {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new("⚠️ Image too large! :(")
                        .size(32.0)
                        .color(egui::Color32::RED)
                        .strong());
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(&error).size(16.0));
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Try navigating to another image.").size(14.0));
                });
                
                // Add retry button outside the centered_and_justified closure
                ui.add_space(10.0);
                if ui.button("Retry").clicked() {
                    self.image_error = None;
                    self.load_current_image_with_cache();
                }
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
                    // GIF is loading but texture not ready yet
                    if self.b_is_loading {
                        ui.centered_and_justified(|ui| {
                            ui.label("Loading GIF...");
                        });
                    } else if self.texture.is_none() && self.is_gif {
                        ui.centered_and_justified(|ui| {
                            ui.label("Loading GIF frame...");
                        });
                    }
                } else {
                    // No image loaded, show logo and instructions
                    if self.b_is_loading {
                        ui.centered_and_justified(|ui| {
                            ui.label("Loading image...");
                        });
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
            let response = ui.allocate_rect(
                ui.available_rect_before_wrap(),
                egui::Sense::drag()
            );

            // Paint the image - we need to get the texture again here
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
        });
    }
}