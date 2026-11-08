//! Copy buttons: copy image, copy path

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui
            .button("📋 Copy Image")
            .on_hover_text("Shortcut: Ctrl+C")
            .clicked()
        {
            app.copy_image_to_clipboard();
        }
        ui.add_space(8.0);
        if ui
            .button("📋 Copy Path")
            .on_hover_text("Shortcut: Ctrl+Shift+C")
            .clicked()
        {
            app.copy_path_to_clipboard();
        }
    });
    ui.add_space(4.0);
}