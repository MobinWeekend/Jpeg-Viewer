use crate::app::types::ViewerApp;
use crate::shortcuts::{ViewerCommand, handle_mouse};
use eframe::egui;

impl ViewerApp {
    /// Handle mouse input on the image response
    pub fn handle_image_mouse_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
    ) {
        // Handle mouse commands (click bindings)
        for command in handle_mouse(
            ctx,
            &self.input_bindings,
            response.hovered(),
            self.b_ctrl_invert,
        ) {
            self.handle_command(ctx, command);
        }

        // Handle middle drag for window dragging
        self.handle_middle_drag(ctx, response);

        // Handle left/right drags for pan and zoom
        self.handle_pan_zoom_drag(ctx, response);
    }

    /// Handle middle mouse drag to drag the window
    fn handle_middle_drag(&mut self, ctx: &egui::Context, response: &egui::Response) {
        if !response.hovered() {
            return;
        }

        let mid_dragging = ctx.input(|i| {
            i.pointer.button_down(egui::PointerButton::Middle)
                && i.pointer.delta().length() > 0.0
        });

        if mid_dragging {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    /// Handle left drag for pan and right drag for zoom
    fn handle_pan_zoom_drag(&mut self, ctx: &egui::Context, response: &egui::Response) {
        if !response.dragged() {
            return;
        }

        let (left_down, right_down, delta) = ctx.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.delta(),
            )
        });

        // Left drag: pan the image
        if left_down && !right_down {
            self.pan += delta / self.zoom;
        }

        // Right drag: zoom
        if right_down && !left_down {
            self.zoom += delta.y * -0.005;
            self.zoom = self.zoom.clamp(0.005, 50.0);
            self.b_zoom_used = true;
        }
    }

    /// Calculate the fit zoom for the image
    pub fn calculate_fit_zoom(
        &mut self,
        texture_size: egui::Vec2,
        available: egui::Vec2,
    ) -> bool {
        // Check if we should actually apply fit
        let should_fit = {
            let width = texture_size.x;
            let height = texture_size.y;
            let ratio = if width > height {
                height / width
            } else {
                width / height
            };
            // Only apply fit if ratio is >= 0.1 (not extreme aspect ratio)
            ratio >= 0.1
        };

        if should_fit {
            let zoom_x = available.x / texture_size.x;
            let zoom_y = available.y / texture_size.y;
            let fit_zoom = zoom_x.min(zoom_y).min(1.0);
            self.zoom = fit_zoom;
            self.pan = egui::Vec2::ZERO;
            true // Applied fit
        } else {
            // For extreme ratios, ensure zoom is 1.0
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
            false // Didn't apply fit, used 1.0 zoom
        }
    }

    /// Get the image rectangle centered in available space
    pub fn get_image_rect(
        &self,
        texture_size: egui::Vec2,
        center: egui::Pos2,
    ) -> egui::Rect {
        let display_size = texture_size * self.zoom;
        egui::Rect::from_center_size(center + self.pan * self.zoom, display_size)
    }

    /// Draw loading overlay for GIF preview
    pub fn draw_gif_loading_overlay(
        &self,
        painter: &egui::Painter,
        response: &egui::Response,
    ) {
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

    /// Draw the welcome/placeholder UI when no image is loaded
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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