//! General settings:
//! - Invert scroll zoom
//! - Texture filter
//! - Theme
//! - Startup fullscreen

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("📋 General").size(15.0), |ui| {
        general_settings(app, ui, ctx);
    });
}

pub fn general_settings(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.add_space(4.0);

    render_invert_scroll_zoom(app, ui);
    ui.add_space(4.0);

    render_texture_filter(app, ui);
    ui.add_space(4.0);

    render_theme(app, ui, ctx);
    ui.add_space(4.0);

    ui.separator();
    ui.add_space(4.0);

    render_startup_fullscreen(app, ui);
    ui.add_space(4.0);
}

// Invert Scroll Zoom
fn render_invert_scroll_zoom(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        let response = ui.checkbox(&mut app.b_ctrl_invert, "Invert Scroll Zoom");

        if response.changed() {
            let value = app.b_ctrl_invert;

            let _ = app.settings_manager.update(|settings| {
                settings.b_ctrl_invert = value;
            });
        }

        ui.add_space(4.0);
        ui.label(if app.b_ctrl_invert {
            "(Ctrl+Scroll to navigate)"
        } else {
            "(Ctrl+Scroll to zoom)"
        });
    });
}

// Texture Filter
fn render_texture_filter(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Texture Filter:");
        ui.add_space(8.0);

        let current_filter = app.settings_manager.get().texture_filter.clone();
        let mut filter = current_filter.clone();

        egui::ComboBox::from_id_salt("texture_filter")
            .selected_text(texture_filter_label(&filter))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");

                ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");

                ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
            });

        if filter != current_filter {
            let _ = app.settings_manager.update(|settings| {
                settings.texture_filter = filter;
            });
            reload_current_image_for_filter(app);
        }
    });
}

fn texture_filter_label(filter: &str) -> &'static str {
    match filter {
        "nearest" => "Nearest (fast)",
        "linear" => "Linear (smooth)",
        "mipmap" => "Mipmap (best)",
        _ => "Unknown",
    }
}

fn reload_current_image_for_filter(app: &mut ViewerApp) {
    if app.image_entries.is_empty() {
        return;
    }

    app.virtual_texture = None;
    app.vt_progress = None;
    app.vt_total_tiles = 0;
    app.virtual_texture_thread = None;

    app.load_current_image_with_cache();
}

// Theme
fn render_theme(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Theme:");
        ui.add_space(8.0);

        // Read current theme from settings (not from ctx directly)
        let current_theme = app.settings_manager.get().theme_preference.clone();
        let mut theme = current_theme.clone();

        egui::ComboBox::from_id_salt("theme")
            .selected_text(theme_label_str(&theme))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut theme, "system".to_string(), "Follow System");
                ui.selectable_value(&mut theme, "light".to_string(), "Light");
                ui.selectable_value(&mut theme, "dark".to_string(), "Dark");
            });

        if theme != current_theme {
            // Update settings
            let _ = app.settings_manager.update(|settings| {
                settings.theme_preference = theme.clone();
            });

            // Apply to egui context
            let theme_pref = match theme.as_str() {
                "light" => egui::ThemePreference::Light,
                "dark" => egui::ThemePreference::Dark,
                _ => egui::ThemePreference::System,
            };
            ctx.set_theme(theme_pref);
        }
    });
}

fn theme_label_str(theme: &str) -> &'static str {
    match theme {
        "system" => "Follow System",
        "light" => "Light",
        "dark" => "Dark",
        _ => "Unknown",
    }
}

// Startup Fullscreen
fn render_startup_fullscreen(app: &mut ViewerApp, ui: &mut egui::Ui) {
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
    ui.add_space(8.0);
    ui.label("Note: When started in fullscreen, Escape will close the app.");
}
