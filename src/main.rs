mod app;
mod loader;
mod settings;

use app::ViewerApp;

fn main() -> eframe::Result<()> {

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "JPEG Viewer",
        options,
        Box::new(|_| Ok(Box::new(ViewerApp::default()))),
    )
}