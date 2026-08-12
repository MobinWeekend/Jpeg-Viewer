use super::types::{FileTypeDetection, LoadingState, ViewerApp};
use crate::image_entry::ImageEntry;
use std::path::PathBuf;

impl ViewerApp {
    /// Check if two extensions are functionally equivalent (e.g., jpg ↔ jpeg)
    fn are_extensions_equivalent(a: &str, b: &str) -> bool {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        if a == b {
            return true;
        }
        // Add groups of equivalent extensions
        match (a.as_str(), b.as_str()) {
            ("jpg", "jpeg") | ("jpeg", "jpg") => true,
            ("tif", "tiff") | ("tiff", "tif") => true,
            // Add others if needed (e.g., "jpe", "jfif"? but they are less common)
            _ => false,
        }
    }

    /// Set the current file type detection and also update the cache entry.
    /// This ensures the cache always holds the most recent generation.
    pub fn set_file_type_detection(&mut self, detection: Option<FileTypeDetection>) {
        // Clone the detection so we can modify its generation if needed
        let detection = detection.map(|mut d| {
            // Always stamp with the current generation
            d.generation = self.preload_generation;
            d.index = self.current_index;
            d
        });

        // Update the global field
        self.file_type_detection = detection.clone();

        // If we have a detection, store it in the cache for this image
        if let Some(det) = &detection {
            if let Some(image_id) = self.get_image_id(self.current_index) {
                if let Some(cached) = self.image_cache.get_mut(&image_id) {
                    cached.file_type_detection = Some(det.clone());
                }
            }
        }
    }

    /// Detect file type using cache if available, otherwise detect from disk.
    pub fn detect_current_file_type(&mut self) {
        // Get the cached detection first, then let the cache borrow end.
        let cached_detection = self.get_image_id(self.current_index).and_then(|image_id| {
            self.image_cache
                .get(&image_id)
                .and_then(|cached| cached.file_type_detection.clone())
        });

        if let Some(detection) = cached_detection {
            println!("[DETECT] Using cached detection for current image");

            self.set_file_type_detection(Some(detection));
            return;
        }

        // No cached detection — detect from disk.
        println!("[DETECT] No cached detection, detecting from disk");
        self.detect_from_disk();
    }

    /// Detect the actual file type from disk.
    pub fn detect_from_disk(&mut self) {
        let path = match self.current_image_path.clone() {
            Some(p) => p,
            None => {
                self.set_file_type_detection(None);
                return;
            }
        };

        let index = self.current_index;
        let generation = self.preload_generation;

        // Only for file entries
        if let Some(entry) = self.image_entries.get(index) {
            if !matches!(entry, ImageEntry::File(_)) {
                self.set_file_type_detection(None);
                return;
            }
        } else {
            self.set_file_type_detection(None);
            return;
        }

        let kind = match infer::get_from_path(&path) {
            Ok(Some(k)) => k,
            _ => {
                self.set_file_type_detection(None);
                return;
            }
        };

        let detected_ext = match kind.extension().to_lowercase().as_str() {
            "jpeg" | "jpg" => "jpg".to_string(),
            "tif" | "tiff" => "tiff".to_string(),
            other => other.to_string(),
        };

        let current_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let mismatch = match current_ext {
            Some(ref ext) => !Self::are_extensions_equivalent(ext, &detected_ext),
            None => true,
        };

        // Navigation changed while detection was running?
        if index != self.current_index || generation != self.preload_generation {
            return;
        }

        let detection = FileTypeDetection {
            detected_extension: detected_ext,
            current_extension: current_ext,
            mismatch,
            index: self.current_index,
            generation: self.preload_generation, // will be overwritten by set_file_type_detection
        };

        // Use the central setter to stamp correct index/generation and update cache
        self.set_file_type_detection(Some(detection));
    }

    /// Generate a unique filename by adding a number suffix if the file exists
    fn get_unique_filename(&self, base_path: &PathBuf, extension: &str) -> PathBuf {
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();

        let parent = base_path.parent().unwrap_or(std::path::Path::new(""));

        let mut counter = 1;
        let mut new_path = parent.join(format!("{}.{}", stem, extension));

        while new_path.exists() {
            new_path = parent.join(format!("{} ({}).{}", stem, counter, extension));
            counter += 1;
        }

        new_path
    }

    /// Apply the suggested rename to the current file.
    pub fn apply_rename_suggestion(&mut self) {
        // Take the detection only if it belongs to the current image
        // and current navigation generation.
        let detection = match self.file_type_detection.take() {
            Some(detection)
                if detection.mismatch
                    && detection.index == self.current_index
                    && detection.generation == self.preload_generation =>
            {
                detection
            }
            _ => return,
        };

        // Keep the detected extension so we can restore the detection
        // if anything fails.
        let new_extension = detection.detected_extension.clone();

        let path = match self.current_image_path.clone() {
            Some(path) => path,
            None => {
                self.file_type_detection = Some(detection);
                return;
            }
        };

        // Rename suggestions only apply to normal filesystem files.
        match self.image_entries.get(self.current_index) {
            Some(ImageEntry::File(_)) => {}
            _ => {
                self.file_type_detection = Some(detection);
                return;
            }
        }

        // Build the new filename.
        let base_path = path.with_extension("");
        let new_path = self.get_unique_filename(&base_path, &new_extension);

        // Nothing to do.
        if new_path == path {
            self.file_type_detection = Some(detection);
            return;
        }

        match std::fs::rename(&path, &new_path) {
            Ok(_) => {
                println!("Renamed {:?} to {:?}", path, new_path);

                // Invalidate any async work belonging to the old file.
                self.preload_generation = self.preload_generation.wrapping_add(1);

                // Cache IDs.
                let old_id = format!("file:{}", path.display());

                // Remove the old file from the cache.
                self.image_cache.pop(&old_id);

                // Update ImageEntry.
                if let Some(ImageEntry::File(p)) = self.image_entries.get_mut(self.current_index) {
                    *p = new_path.clone();
                }

                // Update current path.
                self.current_image_path = Some(new_path.clone());

                // Reset image state.
                self.gif_animation = None;
                self.is_gif = false;
                self.is_preview = false;

                self.full_image_receiver = None;
                self.full_gif_receiver = None;
                self.receiver = None;

                self.image_error = None;
                self.set_file_type_detection(None);

                // Reset viewing state.
                self.zoom = 1.0;
                //self.pan = egui::Vec2::ZERO;
                self.b_zoom_used = false;
                self.b_fit_to_window = true;
                self.image_rect = None;

                // Start loading the renamed file.
                self.set_loading_state(LoadingState::Loading);
                self.load_current_image();

                println!("File renamed and reloading: {:?}", new_path);

                self.update_window_title(&eframe::egui::Context::default());
            }

            Err(e) => {
                eprintln!("Failed to rename {:?}: {}", path, e);

                // Rename failed, so keep the valid suggestion.
                self.file_type_detection = Some(detection);
            }
        }
    }
    /*
    /// Update the file type detection in the cache for the current image
    /// Call this whenever file_type_detection is updated:
    pub fn update_cache_detection(&mut self) {
        if let Some(image_id) = self.get_image_id(self.current_index) {
            if let Some(cached) = self.image_cache.get_mut(&image_id) {
                cached.file_type_detection = self.file_type_detection.clone();
            }
        }
    }
     */
}
