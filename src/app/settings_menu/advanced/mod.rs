mod cache;
mod frame_limiter;
pub(super) mod reset_all;
mod virtual_texture;
pub(super) mod window;

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.vertical(|ui| {
        ui.add_space(4.0);
        crate::app::settings_menu::settings_search_anchor(ui, "advanced_virtual_texture", |ui| {
            virtual_texture::render(app, ui);
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        crate::app::settings_menu::settings_search_anchor(ui, "advanced_cache", |ui| {
            cache::render(app, ui);
        });
        ui.separator();
        crate::app::settings_menu::settings_search_anchor(ui, "advanced_frame_limiter", |ui| {
            frame_limiter::render(app, ui, ctx);
        });
        ui.separator();
        ui.add_space(4.0);
        crate::app::settings_menu::settings_search_anchor(ui, "advanced_thumbnail_workers", |ui| {
            crate::app::settings_menu::general::render_thumbnail_workers(app, ui);
        });

        ui.add_space(4.0);
    });
}
