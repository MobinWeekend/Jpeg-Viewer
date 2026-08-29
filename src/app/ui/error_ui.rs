use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_error_ui(&mut self, ui: &mut egui::Ui, error: &str) {
        ui.style_mut().interaction.selectable_labels = false;
        let available_rect = ui.available_rect_before_wrap();
        let response = ui.interact(
            available_rect,
            ui.id().with("error area"),
            egui::Sense::click_and_drag(),
        );
        self.handle_image_mouse_input(ui.ctx(), &response);

        super::overlay::overlay_area(
            "This is an error",
            egui::Align2::CENTER_CENTER,
            egui::vec2(0.0, 0.0),
        )
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(ui.visuals().window_fill())
                .stroke(ui.visuals().window_stroke())
                .corner_radius(egui::CornerRadius::same(16))
                .inner_margin(egui::Margin::same(32))
                .show(ui, |ui| {
                    ui.style_mut().visuals.override_text_color = None;
                    ui.label(egui::RichText::new(":(").size(56.0));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Well, this is awkward...").size(13.0));
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Failed to Load Image!")
                                .size(32.0)
                                .strong(),
                        );
                        ui.add_space(16.0);

                        egui::Frame::NONE
                            .fill(ui.visuals().window_fill())
                            .stroke(ui.visuals().window_stroke())
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(12, 12))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(error).size(14.0).monospace());
                            });

                        ui.add_space(28.0);

                        ui.horizontal(|ui| {
                            ui.add_space(20.0);

                            let retry_button =
                                egui::Button::new(egui::RichText::new("↻  Retry").size(15.0))
                                    .min_size(egui::vec2(120.0, 44.0))
                                    .corner_radius(10);

                            if ui.add(retry_button).clicked() {
                                self.image_error = None;
                                self.load_current_image_with_cache();
                            }

                            ui.add_space(12.0);

                            let skip_button =
                                egui::Button::new(egui::RichText::new("Skip").size(15.0))
                                    .min_size(egui::vec2(120.0, 44.0))
                                    .corner_radius(10);

                            if ui.add(skip_button).clicked() {
                                self.navigate_images(ui.ctx(), 1);
                            }

                            ui.add_space(20.0);
                        });

                        ui.add_space(8.0);
                    });
                });
        });
    }
}
