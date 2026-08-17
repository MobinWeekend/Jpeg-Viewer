#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod archive;
mod gif;
mod helpers;
mod image_entry;
mod loader;
mod settings;
mod shortcuts;

use app::ViewerApp;
use eframe::egui;
//use fontdb::{Database, Family, Source};

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get();

    let icon = image::load_from_memory(include_bytes!("../assets/icon.ico"))
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
        .with_min_inner_size(egui::vec2(600.0, 600.0));

    if let Some(pos) = app_settings.window_pos {
        viewport = viewport.with_position(pos);
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
        Box::new(|_cc| {
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

            if let Some(path) = std::env::args().nth(1) {
                app.open_path(path.into());
            }

            Ok(Box::new(app))
        }),
    )
}
