#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod archive;
mod helpers;
mod image_entry;
mod loader;
mod settings;
mod shortcuts;

use app::ViewerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get();

    let image = image::load_from_memory(include_bytes!("../assets/icon.ico"))
        .expect("Failed to load icon")
        .into_rgba8();

    let icon = egui::IconData {
        rgba: image.into_raw(),
        width: 128,
        height: 128,
    };

    let mut viewport = egui::ViewportBuilder::default().with_icon(icon);

    if let Some(pos) = app_settings.window_pos {
        // Use with_position with outer position
        viewport = viewport.with_position(pos);
    }

    if let Some(size) = app_settings.window_size {
        // Use with_inner_size for the content area
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
            let mut app = ViewerApp::default();

            if let Some(path) = std::env::args().nth(1) {
                app.open_path(path.into());
            }

            Ok(Box::new(app))
        }),
    )
}
