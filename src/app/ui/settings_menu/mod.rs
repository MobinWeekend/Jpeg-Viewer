//! Settings menu UI

mod advanced;
mod copy_buttons;
pub mod general;
mod look;
mod settings_file;
mod slideshow;

use crate::app::types::ViewerApp;
use eframe::egui;

/// Render settings in a docked, resizable sidebar on the right.
pub fn render_settings_menu(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.show_settings_menu {
        return;
    }

    let default_width = app
        .settings_manager
        .get()
        .settings_sidebar_width
        .clamp(260.0, 640.0);
    let response = egui::SidePanel::right("settings_sidebar")
        .resizable(true)
        .default_width(default_width)
        .width_range(260.0..=640.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Close settings")
                        .clicked()
                    {
                        app.show_settings_menu = false;
                    }
                });
            });
            ui.separator();
            // Keep the version visible at the bottom while the settings sections scroll.
            const FOOTER_HEIGHT: f32 = 34.0;
            let scroll_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(scroll_height)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    copy_buttons::render(app, ui);
                    look::render(app, ui, ctx);
                    general::render(app, ui, ctx);
                    slideshow::render(app, ui, ctx);
                    advanced::render(app, ui, ctx);
                    settings_file::render(app, ui, ctx);
                    ui.add_space(8.0);
                });
            ui.separator();
            ui.add_space(3.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("JPEG Viewer v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .weak(),
                );
            });
        });

    app.right_sidebar_actual_width = response.response.rect.width().max(0.0);
    let width = response.response.rect.width().clamp(260.0, 640.0);
    let previous = app.settings_manager.get().settings_sidebar_width;
    if !ctx.input(|input| input.pointer.any_down()) && (width - previous).abs() > 2.0 {
        let _ = app
            .settings_manager
            .update(|settings| settings.settings_sidebar_width = width);
    }
}

/// Toggle settings menu visibility.
pub fn toggle_settings_menu(app: &mut ViewerApp) {
    app.show_settings_menu = !app.show_settings_menu;
    if app.show_settings_menu {
        // Both views use the right dock; preserve the edit session but only
        // display one right-side panel at a time.
        app.show_edit_menu = false;
        app.crop_tool_active = false;
        app.crop_drag_start_image = None;
    }
}
