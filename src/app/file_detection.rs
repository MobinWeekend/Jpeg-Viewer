// src/app/file_detection.rs

use super::types::{FileTypeDetection, LoadingState, ViewerApp};
use crate::decoder::format_detection::detect_format;
use crate::image_entry::ImageEntry;
use std::path::PathBuf;

impl ViewerApp {
    /// Set the current file type detection and update the cached entry.
    ///
    /// The detection is always associated with the current image index
    /// and preload generation.
    pub fn set_file_type_detection(&mut self, detection: Option<FileTypeDetection>) {
        let detection = detection.map(|mut detection| {
            detection.index = self.current_index;
            detection.generation = self.preload_generation;
            detection
        });

        self.file_type_detection = detection.clone();

        if let Some(detection) = detection {
            if let Some(image_id) = self.get_image_id(self.current_index) {
                if let Some(cached) = self.image_cache.get_mut(&image_id) {
                    cached.file_type_detection = Some(detection);
                }
            }
        }
    }

    /// Detect the current file type, using the cached result when available.
    pub fn detect_current_file_type(&mut self) {
        let cached_detection = self.get_image_id(self.current_index).and_then(|image_id| {
            self.image_cache
                .get(&image_id)
                .and_then(|cached| cached.file_type_detection.clone())
        });

        if let Some(detection) = cached_detection {
            // Cached detections may belong to an older navigation generation.
            if detection.index == self.current_index
                && detection.generation == self.preload_generation
            {
                self.set_file_type_detection(Some(detection));
                return;
            }
        }

        self.detect_from_disk();
    }

    /// Detect the actual file format from disk.
    ///
    /// This only applies to regular filesystem images. Archive entries are
    /// intentionally ignored because they do not have a filesystem filename
    /// that can be renamed.
    pub fn detect_from_disk(&mut self) {
        let path = match self.current_image_path.clone() {
            Some(path) => path,
            None => {
                self.set_file_type_detection(None);
                return;
            }
        };

        let index = self.current_index;
        let generation = self.preload_generation;

        // Only regular filesystem files can have their extensions corrected.
        match self.image_entries.get(index) {
            Some(ImageEntry::File(_)) => {}
            _ => {
                self.set_file_type_detection(None);
                return;
            }
        }

        // Read the file and inspect its magic bytes.
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.set_file_type_detection(None);
                return;
            }
        };

        let detected_format = match detect_format(&bytes) {
            Some(format) => format,
            None => {
                self.set_file_type_detection(None);
                return;
            }
        };

        // Keep the actual extension from the filename so the UI can display
        // what the user currently has.
        let current_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        // ImageFormat owns the extension knowledge, so there is no need for
        // a separate jpg/jpeg or tif/tiff equivalence table.
        let mismatch = match current_extension.as_deref() {
            Some(extension) => !detected_format.matches_extension(extension),
            None => true,
        };

        // The file may have changed while we were reading it.
        if index != self.current_index || generation != self.preload_generation {
            return;
        }

        let detection = FileTypeDetection {
            detected_format,
            current_extension,
            mismatch,
            index,
            generation,
        };

        self.set_file_type_detection(Some(detection));
    }

    /// Generate a unique filename by adding a numbered suffix if necessary.
    fn get_unique_filename(&self, base_path: &PathBuf, extension: &str) -> PathBuf {
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");

        let parent = base_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));

        let mut counter = 1;

        let mut new_path = parent.join(format!("{stem}.{extension}"));

        while new_path.exists() {
            new_path = parent.join(format!("{stem} ({counter}).{extension}"));
            counter += 1;
        }

        new_path
    }

    /// Apply the detected format as the file's extension.
    pub fn apply_rename_suggestion(&mut self) {
        let detection = match self.file_type_detection.take() {
            Some(detection)
                if detection.mismatch
                    //&& detection.index == self.current_index
                    //&& detection.generation == self.preload_generation
                    =>
            {
                detection
            }
            _ => return,
        };

        let new_extension = match detection.detected_format.extensions().first() {
            Some(extension) => *extension,
            None => {
                self.file_type_detection = Some(detection);
                return;
            }
        };

        let path = match self.current_image_path.clone() {
            Some(path) => path,
            None => {
                self.file_type_detection = Some(detection);
                return;
            }
        };

        // Rename suggestions only apply to regular filesystem files.
        match self.image_entries.get(self.current_index) {
            Some(ImageEntry::File(_)) => {}
            _ => {
                self.file_type_detection = Some(detection);
                return;
            }
        }

        let base_path = path.with_extension("");
        let new_path = self.get_unique_filename(&base_path, new_extension);

        if new_path == path {
            self.file_type_detection = Some(detection);
            return;
        }

        match std::fs::rename(&path, &new_path) {
            Ok(()) => {
                println!("Renamed {:?} to {:?}", path, new_path);

                // Invalidate asynchronous work associated with the old file.
                self.preload_generation = self.preload_generation.wrapping_add(1);

                // Remove the old filesystem entry from the cache.
                let old_id = format!("file:{}", path.display());
                self.image_cache.pop(&old_id);

                // Update the ImageEntry.
                if let Some(ImageEntry::File(entry_path)) =
                    self.image_entries.get_mut(self.current_index)
                {
                    *entry_path = new_path.clone();
                }

                self.current_image_path = Some(new_path.clone());

                // Reset image/loading state.
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
                self.b_zoom_used = false;
                self.b_fit_to_window = true;
                self.image_rect = None;

                // Reload the renamed image.
                self.set_loading_state(LoadingState::Loading);
                self.load_current_image();

                println!("File renamed and reloading: {:?}", new_path);

                // TODO: Prefer passing the real application context here
                // rather than constructing a temporary default context.
                self.update_window_title(&eframe::egui::Context::default());
            }

            Err(error) => {
                eprintln!("Failed to rename {:?}: {}", path, error);

                // Keep the suggestion visible after a failed rename.
                self.file_type_detection = Some(detection);
            }
        }
    }

    /// Get the current file size as a human-readable string.
    pub fn get_file_size_string(&self) -> String {
        // Regular filesystem file.
        if let Some(path) = &self.current_image_path {
            if let Ok(metadata) = std::fs::metadata(path) {
                return Self::format_file_size(metadata.len());
            }
        }

        // Archive entry.
        if let Some(entry) = self.image_entries.get(self.current_index) {
            match entry {
                ImageEntry::Zip(zip) => {
                    if let Ok(file) = std::fs::File::open(&zip.archive_path) {
                        if let Ok(mut archive) = zip::ZipArchive::new(file) {
                            if let Ok(entry) = archive.by_index(zip.entry_index) {
                                return Self::format_file_size(entry.size());
                            }
                        }
                    }
                }

                ImageEntry::S7z(_) | ImageEntry::Rar(_) => {
                    // Size is not currently available without inspecting
                    // the archive.
                }

                ImageEntry::File(_) => {}
            }
        }

        String::new()
    }

    /// Format a byte count as a human-readable file size.
    fn format_file_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;

        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{bytes} B")
        }
    }
}
