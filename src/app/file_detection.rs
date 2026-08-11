use super::types::{ViewerApp, FileTypeDetection, CachedImage};
use crate::image_entry::ImageEntry;

impl ViewerApp {
    /// Detect the actual file type of the currently displayed image.
    /// Only works for regular file entries (not archives).
    pub fn detect_current_file_type(&mut self) {
        let path = match self.current_image_path.clone() {
            Some(p) => p,
            None => {
                self.file_type_detection = None;
                return;
            }
        };

        // Only for file entries
        if let Some(entry) = self.image_entries.get(self.current_index) {
            if !matches!(entry, ImageEntry::File(_)) {
                self.file_type_detection = None;
                return;
            }
        } else {
            self.file_type_detection = None;
            return;
        }

        // Use infer's free function to detect from path
        let kind = match infer::get_from_path(&path) {
            Ok(Some(k)) => k,
            _ => {
                self.file_type_detection = None;
                return;
            }
        };

        let detected_ext = kind.extension().to_string();
        let current_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let mismatch = match current_ext {
            Some(ref ext) => ext != &detected_ext,
            None => true, // missing extension
        };

        self.file_type_detection = Some(FileTypeDetection {
            detected_extension: detected_ext,
            current_extension: current_ext,
            mismatch,
        });
    }

    /// Apply the suggested rename to the current file.
    pub fn apply_rename_suggestion(&mut self) {
        // Take the detection, but only if there is a mismatch
        let detection = match self.file_type_detection.take() {
            Some(d) if d.mismatch => d,
            _ => return,
        };

        // Clone the detected extension so we can restore the detection on failure
        let new_extension = detection.detected_extension.clone();

        let path = match self.current_image_path.clone() {
            Some(p) => p,
            None => {
                // Put detection back
                self.file_type_detection = Some(detection);
                return;
            }
        };

        // Ensure it's a file entry
        if let Some(entry) = self.image_entries.get(self.current_index) {
            if !matches!(entry, ImageEntry::File(_)) {
                self.file_type_detection = Some(detection);
                return;
            }
        } else {
            self.file_type_detection = Some(detection);
            return;
        }

        let new_path = path.with_extension(&new_extension);
        if new_path == path {
            // No change needed
            self.file_type_detection = Some(detection);
            return;
        }

        if new_path.exists() {
            eprintln!("Target file already exists: {:?}", new_path);
            // Keep detection so user can see the suggestion
            self.file_type_detection = Some(detection);
            return;
        }

        match std::fs::rename(&path, &new_path) {
            Ok(_) => {
                println!("Renamed {:?} to {:?}", path, new_path);

                // Update image entry
                if let Some(entry) = self.image_entries.get_mut(self.current_index) {
                    if let ImageEntry::File(p) = entry {
                        *p = new_path.clone();
                    }
                }
                self.current_image_path = Some(new_path.clone());

                // Update cache: replace old ID with new ID using current texture
                let old_id = format!("file:{}", path.display());
                let new_id = format!("file:{}", new_path.display());
                if let Some(texture) = &self.texture {
                    self.image_cache.pop(&old_id);
                    let cached = CachedImage {
                        texture: texture.clone(),
                        is_gif: self.is_gif,
                        is_preview: self.is_preview,
                        index: self.current_index,
                    };
                    self.image_cache.put(new_id, cached);
                }

                // Detection is now outdated; clear it
                self.file_type_detection = None;
                self.update_window_title(&eframe::egui::Context::default());
            }
            Err(e) => {
                eprintln!("Failed to rename: {}", e);
                // Restore the detection so the user can try again
                self.file_type_detection = Some(detection);
            }
        }
    }
}