#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod archive;
pub mod constants;
mod decoder;
mod gif;
mod helpers;
mod image_core;
mod image_entry;
mod loader;
mod settings;
mod shortcuts;
mod sort;
mod slideshow;
mod ui_theme;

use app::ViewerApp;
use eframe::egui;
//use fontdb::{Database, Family, Source};

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get();

    let icon = image::load_from_memory(include_bytes!("../assets/icons/icon.ico"))
        .expect("Failed to load icon")
        .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let icon = egui::IconData {
        rgba: icon.into_raw(),
        width: icon_width,
        height: icon_height,
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_icon(icon)
        .with_min_inner_size(egui::vec2(512.0, 512.0))
        .with_decorations(app_settings.show_titlebar)
        .with_resizable(true);

    if !app_settings.start_centered {
        if let Some(pos) = app_settings.window_pos {
            viewport = viewport.with_position(pos);
        }
    }

    if let Some(size) = app_settings.window_size {
        viewport = viewport.with_inner_size(size);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "JPEG Viewer",
        options,
        Box::new(|cc| {
            // Keep egui's default fonts for all normal text and use Noto Emoji
            // only as a fallback for glyphs those fonts do not contain.
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "noto_emoji".to_owned(),
                egui::FontData::from_static(include_bytes!("../assets/fonts/NotoEmoji-Regular.ttf")).into(),
            );

            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("noto_emoji".to_owned());
            }

            cc.egui_ctx.set_fonts(fonts);

            /*
            // Set the default font to the system's default sans-serif font
            let mut fonts = egui::FontDefinitions::default();

            // Search installed system fonts
            let mut db = Database::new();
            db.load_system_fonts();

            // Try to find the platform's default sans-serif font
            if let Some(id) = db.query(&fontdb::Query {
                families: &[Family::SansSerif],
                ..Default::default()
            }) {
                if let Some(face) = db.face(id) {
                    match &face.source {
                        Source::File(path) => {
                            if let Ok(data) = std::fs::read(path) {
                                fonts.font_data.insert(
                                    "system".to_owned(),
                                    egui::FontData::from_owned(data).into(),
                                );

                                fonts
                                    .families
                                    .get_mut(&egui::FontFamily::Proportional)
                                    .unwrap()
                                    .insert(0, "system".to_owned());
                            }
                        }
                        _ => {}
                    }
                }
            }

            cc.egui_ctx.set_fonts(fonts);
            */
            // windows only, use Segoe UI as the default font
            /*
            let mut fonts = egui::FontDefinitions::default();

            #[cfg(target_os = "windows")]
            {
                if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
                    fonts.font_data.insert(
                        "Segoe UI".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );

                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .insert(0, "Segoe UI".to_owned());
                }
            }

            cc.egui_ctx.set_fonts(fonts);
            */

            let mut app = ViewerApp::default();

            // Apply saved theme
            let theme_pref = match app.settings_manager.get().theme_preference.as_str() {
                "light" => egui::ThemePreference::Light,
                "dark" => egui::ThemePreference::Dark,
                _ => egui::ThemePreference::System,
            };
            cc.egui_ctx.set_theme(theme_pref);
            {
                let settings = app.settings_manager.get().clone();
                crate::ui_theme::apply_polished_style_with_options(&cc.egui_ctx, &settings.ui_accent_color, settings.ui_scale, settings.ui_text_scale);
            }

            let startup_paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
            if startup_paths.len() == 1 && startup_paths[0].extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq")) {
                app.open_queue_file(&startup_paths[0], &cc.egui_ctx, false);
            } else if startup_paths.len() == 1 {
                app.open_path(startup_paths[0].clone());
            } else if !startup_paths.is_empty() {
                // Process every path passed by the OS (including multiple files dropped on the app icon).
                app.load_dropped_files(startup_paths);
            }

            Ok(Box::new(app))
        }),
    )
}
