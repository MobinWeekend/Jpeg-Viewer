mod cache;
mod frame_limiter;
pub(super) mod reset_all;
mod virtual_texture;
pub(super) mod window;

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    crate::app::settings_menu::section(ui, "advanced", crate::app::ui::SvgIcon::Performance, "Advanced", false, |ui| {
        ui.add_space(4.0);
        virtual_texture::render(app, ui);
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        cache::render(app, ui);
        frame_limiter::render(app, ui, ctx);
        ui.separator();
        ui.add_space(4.0);
        crate::app::settings_menu::general::render_thumbnail_workers(app, ui);

        ui.add_space(4.0);
    });
}
