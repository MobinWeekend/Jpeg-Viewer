#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod loader;
mod settings;
mod archive;
mod image_entry;
mod helpers;
mod shortcuts;

use app::ViewerApp;

fn main() -> eframe::Result<()> {

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "JPEG Viewer",
        options,
        Box::new(|_| Ok(Box::new(ViewerApp::default()))),
    )
}