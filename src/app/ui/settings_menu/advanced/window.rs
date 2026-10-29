use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("Window").strong(), |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut show_titlebar = app.settings_manager.get().show_titlebar;
            if crate::ui_theme::switch(ui, &mut show_titlebar, "Use native window title bar") {
                let _ = app.settings_manager.update(|settings| {
                    settings.show_titlebar = show_titlebar;
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(show_titlebar));
                ctx.request_repaint();
            }
        });

        let current = app.settings_manager.get().custom_window_frame_thickness;
        let mut thickness = current;
        ui.add_enabled_ui(!app.settings_manager.get().show_titlebar, |ui| {
            let response = ui.add(
                crate::ui_theme::BorderedSlider::new(
                    egui::Slider::new(&mut thickness, 2.0..=12.0)
                        .step_by(1.0)
                        .text("Custom frame thickness")
                        .suffix(" px"),
                ),
            );
            if response.changed() {
                let _ = app.settings_manager.update(|settings| {
                    settings.custom_window_frame_thickness = thickness.clamp(2.0, 12.0);
                });
                ctx.request_repaint();
            }
        });
        ui.label(egui::RichText::new("The custom frame provides the resize hit area when native window decorations are disabled.").small().weak());

        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Reset Window Position").size(13.0))
                        .min_size(egui::vec2(150.0, 28.0)),
                )
                .clicked()
            {
                let _ = app.settings_manager.update(|settings| {
                    settings.window_pos = None;
                });
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Reset Window Size").size(13.0))
                        .min_size(egui::vec2(150.0, 28.0)),
                )
                .clicked()
            {
                let _ = app.settings_manager.update(|settings| {
                    settings.window_size = None;
                });
            }
        });
        ui.add_space(4.0);
    });
}
