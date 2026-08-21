//! File info section

use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use crate::app::ui::settings_menu::helpers::format_file_size;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.collapsing(egui::RichText::new("📂 File Info").size(15.0), |ui| {
        ui.add_space(4.0);

        let Some(entry) = app.image_entries.get(app.current_index) else {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("No image loaded").strong());
            });
            return;
        };

        let (file_name, location, file_size) = match entry {
            ImageEntry::File(path) => {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let size = std::fs::metadata(path)
                    .ok()
                    .map(|meta| format_file_size(meta.len()))
                    .unwrap_or_else(|| "N/A".to_string());
                (name, path.clone(), size)
            }
            ImageEntry::Zip(zip) => {
                let size = std::fs::metadata(&zip.archive_path)
                    .ok()
                    .map(|meta| format_file_size(meta.len()))
                    .unwrap_or_else(|| "N/A".to_string());
                (zip.name.clone(), zip.archive_path.clone(), size)
            }
            ImageEntry::S7z(s7z) => {
                let size = std::fs::metadata(&s7z.archive_path)
                    .ok()
                    .map(|meta| format_file_size(meta.len()))
                    .unwrap_or_else(|| "N/A".to_string());
                (s7z.name.clone(), s7z.archive_path.clone(), size)
            }
            ImageEntry::Rar(rar) => {
                let size = std::fs::metadata(&rar.archive_path)
                    .ok()
                    .map(|meta| format_file_size(meta.len()))
                    .unwrap_or_else(|| "N/A".to_string());
                (rar.name.clone(), rar.archive_path.clone(), size)
            }
        };

        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("File:").strong());
            ui.add_space(8.0);
            ui.label(&file_name);
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Size:").strong());
            ui.add_space(8.0);
            ui.label(&file_size);
        });
        ui.add_space(4.0);

        let path_display = location.display().to_string();
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Path:").strong());
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(&path_display)
                    .color(egui::Color32::LIGHT_GRAY)
                    .monospace(),
            );
        });
        ui.add_space(4.0);
    });
}