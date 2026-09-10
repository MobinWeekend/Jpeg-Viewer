use crate::app::aspect_ratio::AspectRatio;
use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use chrono::{DateTime, Local};
use eframe::egui;
use std::time::SystemTime;

// ========== File Metadata UI ==========
impl ViewerApp {
    /// Render file metadata for the current image.
    /// Works for both regular filesystem files and archive entries.

    pub fn file_metadata_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let response = ui.button("Info");

        if response.clicked() {
            self.file_metadata_open = !self.file_metadata_open;
        }

        if !self.file_metadata_open {
            return;
        }

        egui::Area::new(egui::Id::new("file_metadata_popup"))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    self.file_metadata_content(ui);
                });
            });
    }

    /*
       fn file_metadata_ui(&self, ui: &mut egui::Ui) {
           ui.collapsing("File Metadata", |ui| {
               egui::Frame::group(ui.style()).show(ui, |ui| {
                   self.file_metadata_content(ui);
               });
           });
       }
    */
    fn file_metadata_content(&self, ui: &mut egui::Ui) {
        let Some(entry) = self.image_entries.get(self.current_index) else {
            ui.label(egui::RichText::new("No image loaded").size(12.0).weak());
            return;
        };
        /*
               // ===== Name =====
               if let Some(name) = self.display_name(entry) {
                   ui.label(egui::RichText::new("Name").size(11.0).weak());
                   ui.label(egui::RichText::new(name).size(12.0));
                   ui.add_space(4.0);
               }

               // ===== Path / Source =====
               let source_path = match entry {
                   ImageEntry::File(path) => Some(path.clone()),
                   ImageEntry::Zip(zip) => Some(zip.archive_path.clone()),
                   ImageEntry::S7z(s7) => Some(s7.archive_path.clone()),
                   ImageEntry::Rar(rar) => Some(rar.archive_path.clone()),
               };

               if let Some(path) = source_path {
                   let label = match entry {
                       ImageEntry::File(_) => "Path",
                       _ => "Archive",
                   };

                   ui.label(egui::RichText::new(label).size(11.0).weak());
                   ui.label(
                       egui::RichText::new(path.to_string_lossy().to_string())
                           .size(11.0)
                           .weak(),
                   );
                   ui.add_space(4.0);
               }
        */
        // ===== Size =====
        let size = self.get_file_size_string();
        if !size.is_empty() {
            ui.label(egui::RichText::new("Size").size(11.0).weak());
            ui.label(egui::RichText::new(size).size(12.0));
            ui.add_space(4.0);
        }

        // ===== Filesystem-only metadata =====
        if let ImageEntry::File(path) = entry {
            if let Ok(metadata) = std::fs::metadata(path) {
                // Modified
                if let Ok(modified) = metadata.modified() {
                    ui.label(egui::RichText::new("Modified").size(11.0).weak());
                    ui.label(egui::RichText::new(Self::format_system_time(modified)).size(12.0));
                    ui.add_space(4.0);
                }

                // Created
                if let Ok(created) = metadata.created() {
                    ui.label(egui::RichText::new("Created").size(11.0).weak());
                    ui.label(egui::RichText::new(Self::format_system_time(created)).size(12.0));
                    ui.add_space(4.0);
                }

                // Accessed
                if let Ok(accessed) = metadata.accessed() {
                    ui.label(egui::RichText::new("Accessed").size(11.0).weak());
                    ui.label(egui::RichText::new(Self::format_system_time(accessed)).size(12.0));
                    ui.add_space(4.0);
                }

                // Read-only
                ui.label(egui::RichText::new("Read-only").size(11.0).weak());
                ui.label(
                    egui::RichText::new(if metadata.permissions().readonly() {
                        "Yes"
                    } else {
                        "No"
                    })
                    .size(12.0),
                );
                ui.add_space(4.0);
            }
        }

        // ===== Image dimensions =====
        let (w, h) = self.image_dimensions();
        if w > 0 && h > 0 {
            ui.label(egui::RichText::new("Dimensions").size(11.0).weak());
            ui.label(egui::RichText::new(format!("{}×{}", w, h)).size(12.0));
            ui.add_space(4.0);

            // Aspect ratio
            if let Some(label) = AspectRatio::get_label(w, h) {
                ui.label(egui::RichText::new("Aspect Ratio").size(11.0).weak());
                ui.label(egui::RichText::new(label).size(12.0));
                ui.add_space(4.0);
            }
        }

        // ===== Detected format / mismatch =====
        if let Some(detection) = &self.file_type_detection {
            ui.label(egui::RichText::new("Detected Format").size(11.0).weak());
            ui.label(egui::RichText::new(format!("{:?}", detection.detected_format)).size(12.0));
            ui.add_space(4.0);

            if detection.mismatch {
                let current = detection.current_extension.as_deref().unwrap_or("(none)");

                ui.label(
                    egui::RichText::new(format!(
                        "⚠ Extension '{}' does not match detected format",
                        current
                    ))
                    .size(11.0)
                    .color(egui::Color32::YELLOW),
                );
            }
        }
    }

    /*
       /// Return a display name for the given entry.
       fn display_name(&self, entry: &ImageEntry) -> Option<String> {
           match entry {
               ImageEntry::File(path) => path
                   .file_name()
                   .and_then(|n| n.to_str())
                   .map(|s| s.to_string()),
               ImageEntry::Zip(zip) => Some(format!(
                   "{}#{}",
                   zip.archive_path
                       .file_name()
                       .and_then(|n| n.to_str())
                       .unwrap_or("archive.zip"),
                   zip.entry_index,
               )),
               ImageEntry::S7z(s7) => s7
                   .archive_path
                   .file_name()
                   .and_then(|n| n.to_str())
                   .map(|s| s.to_string()),
               ImageEntry::Rar(rar) => rar
                   .archive_path
                   .file_name()
                   .and_then(|n| n.to_str())
                   .map(|s| s.to_string()),
           }
       }
    */
    /// Format a `SystemTime` into a readable local date/time string using chrono.
    fn format_system_time(time: SystemTime) -> String {
        let datetime: DateTime<Local> = time.into();
        datetime.format("%b %d, %Y at %I:%M %p").to_string()
        /*
        // Full date + time
        "%Y-%m-%d %H:%M:%S"        // 2024-01-15 14:30:45
        // With timezone
        "%Y-%m-%d %H:%M:%S %Z"     // 2024-01-15 14:30:45 CET
        // Human readable
        "%b %d, %Y at %I:%M %p"    // Jan 15, 2024 at 02:30 PM
        // Relative (if you want to add it later, use chrono-humanize crate)
         */
    }
}
