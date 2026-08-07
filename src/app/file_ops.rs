use super::types::ViewerApp;
use crate::archive::{scan_7z, scan_rar, scan_zip};
use crate::helpers::{ARCHIVE_EXT, IMAGE_EXT, get_extension, is_supported_image};
use crate::image_entry::ImageEntry;
use eframe::egui;
use std::path::PathBuf;

impl ViewerApp {
    pub fn open_path(&mut self, path: PathBuf) {
        self.b_zoom_used = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.b_is_loading_full = false;

        // Reset preload state – discard all pending tasks and invalidate results.
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false; // ensure caching is re-enabled

        if path.is_dir() {
            self.load_directory(&path);
            return;
        }

        match get_extension(&path).as_deref() {
            Some("zip") => {
                self.set_image_entries(scan_zip(&path), 0);
            }
            Some("7z") => {
                self.set_image_entries(scan_7z(&path), 0);
            }
            Some("rar") => {
                self.set_image_entries(scan_rar(&path), 0);
            }
            Some(_) if is_supported_image(&path) => {
                self.load_image(path);
            }
            _ => {
                println!("Unsupported file: {:?}", path);
            }
        }
    }

    pub fn open_file_dialog(&mut self) {
        let mut extensions = Vec::new();
        extensions.extend_from_slice(IMAGE_EXT);
        extensions.extend_from_slice(ARCHIVE_EXT);

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images and Archives", &extensions)
            .pick_file()
        {
            self.open_path(path);
        }
    }

    pub fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
    }

    pub fn save_window_state(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let pos = [rect.min.x, rect.min.y];
            let size = [rect.width(), rect.height()];

            let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            if !is_fullscreen && size[0] > 100.0 && size[1] > 100.0 {
                if let Err(e) = self.settings_manager.update_window_state(pos, size) {
                    eprintln!("Failed to save window state: {}", e);
                }
            }
        }
    }

    pub fn delete_current_image(&mut self) {
        if self.image_entries.is_empty() {
            return;
        }

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        let file_path = match entry {
            ImageEntry::File(path) => path,
            _ => {
                println!("Cannot delete images inside archives directly");
                return;
            }
        };

        if !file_path.exists() {
            println!("File no longer exists on disk");
            self.image_entries.remove(self.current_index);
            if self.current_index >= self.image_entries.len() {
                self.current_index = 0;
            }
            if !self.image_entries.is_empty() {
                self.load_current_image();
            } else {
                self.texture = None;
            }
            return;
        }

        match trash::delete(&file_path) {
            Ok(_) => {
                println!("Moved to trash: {:?}", file_path);
                self.image_entries.remove(self.current_index);
                if self.image_entries.is_empty() {
                    self.texture = None;
                    self.current_index = 0;
                } else if self.current_index >= self.image_entries.len() {
                    self.current_index = self.image_entries.len() - 1;
                    self.load_current_image();
                } else {
                    self.load_current_image();
                }
                self.b_fit_to_window = true;
                self.image_rect = None;
                self.b_zoom_used = false;

                // Reset preload state
                self.image_cache.clear();
                self.preloading_indices.clear();
                self.preload_tasks.clear();
                self.preload_workers = 0;
                self.preload_generation = self.preload_generation.wrapping_add(1);
                self.should_stop_caching = false;
            }
            Err(e) => {
                eprintln!("Failed to move to trash: {}", e);
            }
        }
    }

    pub fn load_dropped_files(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        self.b_zoom_used = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.b_is_loading_full = false;

        // Reset preload state
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;

        // Filter to only supported images
        let image_paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| {
                if path.is_file() {
                    crate::helpers::is_supported_image(path)
                } else {
                    false
                }
            })
            .collect();

        if image_paths.is_empty() {
            println!("No supported images found in dropped files");
            return;
        }

        // Store the current directory from the first file
        if let Some(parent) = image_paths.first().and_then(|p| p.parent()) {
            self.current_directory = Some(parent.to_path_buf());
        }

        // Convert to ImageEntries
        let entries: Vec<ImageEntry> = image_paths.into_iter().map(ImageEntry::File).collect();

        // Set image entries starting at index 0
        self.set_image_entries(entries, 0);
        self.zoom = 1.0;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        // Cache already cleared above; no need to clear again.

        println!("Loaded {} dropped image(s)", self.image_entries.len());
    }
}