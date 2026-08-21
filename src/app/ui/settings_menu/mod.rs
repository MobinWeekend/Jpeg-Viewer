//! Settings menu UI

mod copy_buttons;
mod file_info;
mod general;
mod slideshow;
mod advanced;
mod helpers;

use crate::app::types::ViewerApp;
use eframe::egui;

/// Render the settings window.
pub fn render_settings_menu(app: &mut ViewerApp, ctx: &egui::Context) {
    let mut open = app.show_settings_menu;
    egui::Window::new("Settings")
        .title_bar(true)
        .collapsible(false)
        .resizable(true)
        .default_size([420.0, 600.0])
        .min_size([350.0, 450.0])
        .max_size([600.0, 900.0])
        .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 42.0))
        .open(&mut open)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(4.0);

                    copy_buttons::render(app, ui);
                    file_info::render(app, ui);
                    general::render(app, ui, ctx);
                    slideshow::render(app, ui, ctx);
                    advanced::render(app, ui, ctx);

                    ui.add_space(8.0);
                });
        });
    app.show_settings_menu = open;
}

/// Toggle settings menu visibility.
pub fn toggle_settings_menu(app: &mut ViewerApp) {
    app.show_settings_menu = !app.show_settings_menu;
}