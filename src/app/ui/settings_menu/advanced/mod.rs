mod virtual_texture;
mod cache;
mod frame_limiter;
mod window;
mod reset_all;

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("⚙ Advanced").size(15.0), |ui| {
        ui.add_space(4.0);

        virtual_texture::render(app, ui);
        cache::render(app, ui);
        frame_limiter::render(app, ui, ctx);
        window::render(app, ui, ctx);
        reset_all::render(app, ui, ctx);

        ui.add_space(4.0);
    });
}