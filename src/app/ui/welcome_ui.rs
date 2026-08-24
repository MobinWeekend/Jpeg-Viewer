/// Draw the welcome/placeholder UI when no image is loaded
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
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
            // For later work:
            //use super::settings_menu::general::general_settings;
            //general_settings(self, ui, ctx);
        });
        // Same mouse behavior as image view
        self.handle_image_mouse_input(ctx, &response);
        //use crate::app::ui::helpers::render_drag_area;
        //render_drag_area(ctx, ui);
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
                ui.style_mut().interaction.selectable_labels = false;
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
