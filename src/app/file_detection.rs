use super::types::{ViewerApp, FileTypeDetection};
use crate::image_entry::ImageEntry;
use std::path::PathBuf;

impl ViewerApp {
    /// Detect file type using cache if available, otherwise detect from disk
    pub fn detect_current_file_type(&mut self) {
        // First, try to get detection from cache
        if let Some(image_id) = self.get_image_id(self.current_index) {
            if let Some(cached) = self.image_cache.get(&image_id) {
                if let Some(detection) = &cached.file_type_detection {
                    // Found detection in cache - use it
                    println!("[DETECT] Using cached detection for: {}", image_id);
                    self.file_type_detection = Some(detection.clone());
                    return;
                }
            }
        }
        
        // Not in cache - detect from disk
        println!("[DETECT] No cached detection, detecting from disk");
        self.detect_from_disk();
    }
    
    /// Detect the actual file type from disk (original implementation)
    pub fn detect_from_disk(&mut self) {
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
            None => true,
        };

        self.file_type_detection = Some(FileTypeDetection {
            detected_extension: detected_ext,
            current_extension: current_ext,
            mismatch,
        });
        
        // Also cache this detection if the image is cached
        if let Some(image_id) = self.get_image_id(self.current_index) {
            if let Some(cached) = self.image_cache.get_mut(&image_id) {
                cached.file_type_detection = self.file_type_detection.clone();
            }
        }
    }

    /// Generate a unique filename by adding a number suffix if the file exists
    fn get_unique_filename(&self, base_path: &PathBuf, extension: &str) -> PathBuf {
        let stem = base_path.file_stem()
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

        // Get the new path (with extension change)
        let base_path = path.with_extension("");
        let new_path = self.get_unique_filename(&base_path, &new_extension);
        
        if new_path == path {
            // No change needed
            self.file_type_detection = Some(detection);
            return;
        }

        match std::fs::rename(&path, &new_path) {
            Ok(_) => {
                println!("Renamed {:?} to {:?}", path, new_path);

                // Get the old cache ID before we change anything
                let old_id = format!("file:{}", path.display());
                let _new_id = format!("file:{}", new_path.display());
                
                // 1. Remove old entry from cache
                self.image_cache.pop(&old_id);
                
                // 2. Update the image entry with the new path
                if let Some(entry) = self.image_entries.get_mut(self.current_index) {
                    if let ImageEntry::File(p) = entry {
                        *p = new_path.clone();
                    }
                }
                
                // 3. Update current_image_path
                self.current_image_path = Some(new_path.clone());
                
                // 4. Clear current state
                self.texture = None;
                self.gif_animation = None;
                self.is_gif = false;
                self.is_preview = false;
                self.full_image_receiver = None;
                self.full_gif_receiver = None;
                self.b_is_loading_full = false;
                self.image_error = None;
                self.receiver = None;
                self.file_type_detection = None;
                
                // 5. Reload the image from the new path
                self.b_is_loading = true;
                self.b_fit_to_window = true;
                
                // Use the existing load mechanism
                self.load_current_image();
                
                // 6. The image will be loaded and cached with the correct extension
                // The detection will be updated when it loads
                
                println!("File renamed and reloading: {:?}", new_path);
                self.update_window_title(&eframe::egui::Context::default());
            }
            Err(e) => {
                eprintln!("Failed to rename: {}", e);
                // Restore the detection so the user can try again
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