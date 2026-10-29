//! Settings file location, open button, and read/write error status.

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("💾 Settings File").size(15.0), |ui| {
        ui.add_space(4.0);

        let path = app.settings_manager.path();

        // Path display
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Location:").strong());
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(path.display().to_string())
                    .color(ui.visuals().weak_text_color())
                    .monospace(),
            );
        });
        ui.add_space(6.0);

        // Open button + reload status
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("📂 Open Settings File").size(13.0))
                        .min_size(egui::vec2(180.0, 28.0)),
                )
                .on_hover_text("Open the settings .ini file in your default editor")
                .clicked()
            {
                open_path(&path);
            }
        });
        ui.add_space(6.0);

        // Error / status display
        if let Some(err) = app.settings_manager.last_error() {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("⚠ {}", err)).color(ui.visuals().error_fg_color));
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("✅ Settings loaded successfully")
                        .color(ui.visuals().text_color()),
                );
            });
        }

        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Reset settings").strong());
        super::advanced::reset_all::render(app, ui, ctx);
        ui.add_space(4.0);
    });
}

/// Open the settings file with the OS default handler.
fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
