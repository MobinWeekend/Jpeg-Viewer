//! General settings: invert scroll, texture filter, startup fullscreen

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("📋 General").size(15.0), |ui| {
        ui.add_space(4.0);

        // Invert scroll zoom
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let response = ui.checkbox(&mut app.b_ctrl_invert, "Invert Scroll Zoom");
            if response.changed() {
                let _ = app.settings_manager.update(|settings| {
                    settings.b_ctrl_invert = app.b_ctrl_invert;
                });
            }
            ui.add_space(4.0);
            ui.label("(Ctrl+Scroll to zoom)");
        });
        ui.add_space(4.0);

        // Texture Filter
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Texture Filter:");
            ui.add_space(8.0);
            let mut filter = app.settings_manager.get().texture_filter.clone();
            egui::ComboBox::from_label("")
                .selected_text(&filter)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");
                    ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");
                    ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
                });

            let current_filter = app.settings_manager.get().texture_filter.clone();
            if filter != current_filter {
                let _ = app.settings_manager.update(|settings| {
                    settings.texture_filter = filter.clone();
                });
                if !app.image_entries.is_empty() {
                    // Force reload of virtual texture if present
                    app.virtual_texture = None;
                    app.vt_progress = None;
                    app.vt_total_tiles = 0;
                    app.virtual_texture_thread = None;
                    app.load_current_image_with_cache();
                }
            }
        });
        ui.add_space(4.0);

        ui.separator();
        ui.add_space(4.0);

        // Startup fullscreen
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut start_fs = app.settings_manager.get().start_fullscreen;
            if ui
                .checkbox(&mut start_fs, "Start in Fullscreen Mode")
                .changed()
            {
                let _ = app.settings_manager.update(|settings| {
                    settings.start_fullscreen = start_fs;
                });
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Note: When started in fullscreen, Escape will close the app.");
        });
        ui.add_space(4.0);
    });
}