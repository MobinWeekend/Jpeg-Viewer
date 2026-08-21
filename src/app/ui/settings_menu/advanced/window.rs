use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("🪟 Window").size(13.0).strong());
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let mut show_titlebar = app.settings_manager.get().show_titlebar;
        if ui
            .checkbox(&mut show_titlebar, "Show Title Bar - WIP")
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.show_titlebar = show_titlebar;
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(show_titlebar));
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(egui::RichText::new("🔄 Reset Window Position").size(13.0))
                    .min_size(egui::vec2(160.0, 28.0)),
            )
            .clicked()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.window_pos = None;
            });
        }
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(egui::RichText::new("🔄 Reset Window Size").size(13.0))
                    .min_size(egui::vec2(160.0, 28.0)),
            )
            .clicked()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.window_size = None;
            });
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}
