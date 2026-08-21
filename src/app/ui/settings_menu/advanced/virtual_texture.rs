use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("⚙ Virtual texture Settings").size(13.0).strong());

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Tile Size:");
        ui.add_space(8.0);
        let mut tile = app.settings_manager.get().tile_size;
        egui::ComboBox::from_label("")
            .selected_text(format!("{}px", tile))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut tile, 128, "128px");
                ui.selectable_value(&mut tile, 256, "256px");
                ui.selectable_value(&mut tile, 512, "512px");
                ui.selectable_value(&mut tile, 1024, "1024px");
                ui.selectable_value(&mut tile, 2048, "2048px");
            });
        if tile != app.settings_manager.get().tile_size {
            let _ = app.settings_manager.update(|settings| {
                settings.tile_size = tile;
            });
            if !app.image_entries.is_empty() {
                app.virtual_texture = None;
                app.vt_progress = None;
                app.vt_total_tiles = 0;
                app.virtual_texture_thread = None;
                app.load_current_image_with_cache();
            }
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("VT Threshold:");
        ui.add_space(8.0);
        let mut threshold = app.settings_manager.get().virtual_texture_threshold;
        if ui
            .add(egui::Slider::new(&mut threshold, 4096..=16384).text("px"))
            .changed()
        {
            if threshold != app.settings_manager.get().virtual_texture_threshold {
                let _ = app.settings_manager.update(|settings| {
                    settings.virtual_texture_threshold = threshold;
                });
                if !app.image_entries.is_empty() {
                    app.virtual_texture = None;
                    app.vt_progress = None;
                    app.vt_total_tiles = 0;
                    app.virtual_texture_thread = None;
                    app.load_current_image_with_cache();
                }
            }
        }
    });
    ui.add_space(8.0);
}