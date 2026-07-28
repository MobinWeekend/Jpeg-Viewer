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
    let image = image::load_from_memory(include_bytes!("../assets/icon.ico"))
        .expect("Failed to load icon")
        .into_rgba8();

    let icon = egui::IconData {
        rgba: image.into_raw(),
        width: 128,
        height: 128,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon),
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