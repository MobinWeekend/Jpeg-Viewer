//! Settings file location, open-folder action, and read/write error status.

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    ui.vertical(|ui| {
        ui.add_space(4.0);

        let path = app.settings_manager.path();
        let folder = path.parent().unwrap_or(path);

        ui.label(egui::RichText::new("INI file").strong());
        ui.add(
            egui::Label::new(
                egui::RichText::new(path.display().to_string())
                    .color(ui.visuals().weak_text_color())
                    .monospace(),
            )
            .wrap(),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            crate::app::ui::paint_svg_icon(ui, icon_rect, crate::app::ui::SvgIcon::OpenFolder, ui.visuals().text_color());
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Open Settings Folder").size(13.0))
                        .min_size(egui::vec2(190.0, 28.0)),
                )
                .on_hover_text("Open the folder containing settings.ini in your file manager")
                .clicked()
                && let Err(error) = open::that(folder)
            {
                eprintln!("Could not open settings folder {}: {error}", folder.display());
            }
        });
        ui.add_space(6.0);

        if let Some(err) = app.settings_manager.last_error() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("⚠ {err}"))
                        .color(ui.visuals().error_fg_color),
                );
            });
        } else {
            ui.label(
                egui::RichText::new("✓ Settings loaded successfully").weak(),
            );
        }

    });
}
