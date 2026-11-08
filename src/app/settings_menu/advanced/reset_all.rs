use crate::app::types::ViewerApp;
use eframe::egui;
use std::time::Duration;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("⚠ Reset All Settings to Default").size(13.0),
                )
                .min_size(egui::vec2(220.0, 32.0)),
            )
            .clicked()
        {
            let default = crate::settings::AppSettings::default();
            let _ = app.settings_manager.update(|settings| {
                *settings = default;
            });
            app.load_frame_limiter_settings();
            app.load_slideshow_settings();
            app.b_ctrl_invert = app.settings_manager.get().b_ctrl_invert;
            app.cache_radius = app.settings_manager.get().cache_radius;
            app.cache_delta_factor = app.settings_manager.get().cache_delta_factor;
            app.max_cache_task = app.settings_manager.get().max_cache_task;
            app.navigation_pause_duration = Duration::from_millis(
                app.settings_manager.get().navigation_pause_ms,
            );
            app.image_cache.clear();
            app.preloading_indices.clear();
            app.preload_tasks.clear();
            if !app.image_entries.is_empty() {
                app.load_current_image_with_cache();
            }
            app.update_window_title(ctx);
        }
    });
    ui.add_space(4.0);
}