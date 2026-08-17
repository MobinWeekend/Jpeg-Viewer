// src/app/ui/input_handlers.rs
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    /// Calculate the fit zoom for the image
    pub fn calculate_fit_zoom(&mut self, texture_size: egui::Vec2, available: egui::Vec2) -> bool {
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
        let zoom_x = available.x / texture_size.x;
        let zoom_y = available.y / texture_size.y;
        if should_fit {
            let fit_zoom = zoom_x.min(zoom_y).min(1.0);
            self.zoom = fit_zoom;
            self.pan = egui::Vec2::ZERO;
            true // Applied fit all sides
        } else {
            // For extreme ratios
            let max_fit_zoom = zoom_x.max(zoom_y).min(1.0);
            self.zoom = max_fit_zoom;
            let snap_to_top = (texture_size.y / 2.0) - (available.y / 2.0) / max_fit_zoom;
            self.pan = egui::Vec2::new(0.0, snap_to_top);
            false // fit to one side
        }
    }

    /// Get the image rectangle centered in available space
    pub fn get_image_rect(&self, texture_size: egui::Vec2, center: egui::Pos2) -> egui::Rect {
        let display_size = texture_size * self.zoom;
        egui::Rect::from_center_size(center + self.pan * self.zoom, display_size)
    }

    /// Draw the welcome/placeholder UI when no image is loaded
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let available_rect = ui.available_rect_before_wrap();

        // Full welcome area interaction
        let response = ui.interact(
            available_rect,
            ui.id().with("welcome_area"),
            egui::Sense::click_and_drag(),
        );

        // Welcome content
        let available = available_rect.height();
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
                            egui::RichText::new(" or drag and drop a photo, folder ").size(24.0),
                        );
                    });
                },
            );

            ui.label(
                egui::RichText::new("or a .zip, .7z, or .rar archive containing your photos.")
                    .size(24.0),
            );
        });

        // Same mouse behavior as image view
        self.handle_image_mouse_input(ctx, &response);
    }

    /// Render keyboard shortcut help overlay
    pub fn render_shortcut_help(&self, ctx: &egui::Context) {
        egui::Window::new("Keyboard Shortcuts")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::new(0.0, -20.0))
            .frame(egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(0, 0, 0, 92),
                corner_radius: egui::CornerRadius::same(8),
                outer_margin: egui::Margin::ZERO,
                inner_margin: egui::Margin::symmetric(16, 12),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Keys").size(18.0));
                    ui.add_space(8.0);

                    let shortcuts = [
                        ("◀/▶", "Navigate"),
                        ("F11/F", "Fullscreen"),
                        ("+/-", "Zoom"),
                        ("Tab", "Settings"),
                        ("F1", "Help"),
                    ];

                    for (i, (key, action)) in shortcuts.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("·").color(egui::Color32::LIGHT_BLUE));
                            ui.add_space(4.0);
                        }
                        ui.colored_label(egui::Color32::LIGHT_GREEN, *key);
                        ui.colored_label(egui::Color32::WHITE, *action);
                    }
                });
            });
    }
}
