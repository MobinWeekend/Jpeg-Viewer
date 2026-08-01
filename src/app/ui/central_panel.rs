use crate::app::types::ViewerApp;
use crate::shortcuts::{ViewerCommand, handle_mouse};
use eframe::egui;

impl ViewerApp {
    pub fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                let texture_size = texture.size_vec2();
                let available = ui.available_size();

                // Only apply fit if b_fit_to_window is true AND not an extreme aspect ratio
                if self.b_fit_to_window {
                    // Check if we should actually apply fit
                    let should_fit = if let Some(texture) = &self.texture {
                        let tex_size = texture.size_vec2();
                        // Check if current texture has extreme aspect ratio
                        // We store this info or check dynamically
                        let width = tex_size.x;
                        let height = tex_size.y;
                        let ratio = if width > height {
                            height / width
                        } else {
                            width / height
                        };
                        // Only apply fit if ratio is >= 0.1
                        ratio >= 0.1
                    } else {
                        true
                    };

                    if should_fit {
                        let zoom_x = available.x / texture_size.x;
                        let zoom_y = available.y / texture_size.y;
                        let fit_zoom = zoom_x.min(zoom_y).min(1.0);
                        self.zoom = fit_zoom;
                        self.pan = egui::Vec2::ZERO;
                        self.b_fit_to_window = false;
                    } else {
                        // For extreme ratios, ensure zoom is 1.0
                        self.zoom = 1.0;
                        self.pan = egui::Vec2::ZERO;
                        self.b_fit_to_window = false;
                    }
                }

                let display_size = texture_size * self.zoom;

                // Center the image in the available space
                let center = ui.available_rect_before_wrap().center();
                let image_rect =
                    egui::Rect::from_center_size(center + self.pan * self.zoom, display_size);

                // Allocate the full available space for interaction
                let response =
                    ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());

                // Paint the image centered in the allocated space using ui.painter()
                let painter = ui.painter();
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    egui::Color32::WHITE,
                );

                // Draw loading indicator overlay for GIFs
                if self.is_gif && self.is_preview {
                    let painter = ui.painter();
                    let text = "Loading GIF...".to_string();
                    let font_id = egui::FontId::proportional(20.0);
                    let galley = painter.layout(text, font_id, egui::Color32::WHITE, f32::INFINITY);
                    let rect = galley.rect;

                    let bg_rect = rect.expand(15.0);
                    painter.rect_filled(
                        bg_rect,
                        8.0,
                        egui::Color32::from_rgba_premultiplied(0, 0, 0, 200),
                    );

                    painter.galley(
                        egui::pos2(
                            response.rect.center().x - rect.width() / 2.0,
                            response.rect.center().y - rect.height() / 2.0,
                        ),
                        galley,
                        egui::Color32::WHITE,
                    );
                }

                // Handle mouse input on the response
                for command in handle_mouse(
                    ctx,
                    &self.input_bindings,
                    response.hovered(),
                    self.b_ctrl_invert,
                ) {
                    self.handle_command(ctx, command);
                }

                // Check for middle drag separately from other drags
                if response.hovered() {
                    let mid_dragging = ctx.input(|i| {
                        i.pointer.button_down(egui::PointerButton::Middle)
                            && i.pointer.delta().length() > 0.0
                    });

                    if mid_dragging {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }

                // Then handle left/right drags
                if response.dragged() {
                    let (left_down, right_down, delta) = ctx.input(|i| {
                        (
                            i.pointer.button_down(egui::PointerButton::Primary),
                            i.pointer.button_down(egui::PointerButton::Secondary),
                            i.pointer.delta(),
                        )
                    });

                    // Only handle left/right drags, skip middle
                    if left_down && !right_down {
                        self.pan += delta / self.zoom;
                        // ... pan logic
                    }

                    if right_down && !left_down {
                        self.zoom += delta.y * -0.005;
                        self.zoom = self.zoom.clamp(0.005, 50.0);
                        self.b_zoom_used = true;
                    }
                }
            } else if self.is_gif {
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
                if self.b_is_loading {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading image...");
                    });
                } else {
                    // No image loaded, show logo and instructions
                    let available = ui.available_height();
                    let content_height = 256.0 + 16.0 + 60.0;

                    ui.add_space((available - content_height).max(0.0) * 0.5);

                    ui.vertical_centered(|ui| {
                        if let Some(icon) = &self.logo_texture {
                            ui.image((icon.id(), egui::vec2(256.0, 256.0)));
                            ui.add_space(16.0);
                        }

                        ui.allocate_ui_with_layout(
                            egui::vec2(510.0, 32.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.horizontal_centered(|ui| {
                                    ui.label(egui::RichText::new("Press ").size(24.0));

                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("Ctrl+O").size(24.0).strong(),
                                            )
                                            .min_size(egui::vec2(80.0, 36.0)),
                                        )
                                        .clicked()
                                    {
                                        self.handle_command(ctx, ViewerCommand::OpenFile);
                                    }
                                    ui.label(
                                        egui::RichText::new(" or drag and drop a photo, folder ")
                                            .size(24.0),
                                    );
                                });
                            },
                        );
                        ui.label(
                            egui::RichText::new(
                                "or a .zip, .7z, or .rar archive containing your photos.",
                            )
                            .size(24.0),
                        );
                    });
                }
            }
        });
    }
}
