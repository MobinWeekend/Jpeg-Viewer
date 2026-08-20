use super::types::{LoadingState, ViewerApp};
use crate::archive::{scan_7z, scan_rar, scan_zip};
use crate::helpers::{get_extension, is_supported_image};
use crate::app::constants::{ARCHIVE_EXT, IMAGE_EXT};
use crate::image_entry::ImageEntry;
use arboard::Clipboard;
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
        self.set_file_type_detection(None); // Clear file extension detection
        self.set_loading_state(LoadingState::Idle);

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

    fn stop_slideshow(&mut self) {
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            let _ = self.settings_manager.update(|settings| {
                settings.slideshow_enabled = false;
            });
        }
    }

    pub fn open_file_dialog(&mut self) {
        // Stop slideshow when opening a file
        self.stop_slideshow();
        let mut extensions = Vec::new();
        extensions.extend_from_slice(IMAGE_EXT);
        extensions.extend_from_slice(ARCHIVE_EXT);
        // Create the dialog with file picker
        let dialog = rfd::FileDialog::new().add_filter("Images and Archives", &extensions);

        self.dialog_open = true;
        let path = dialog.clone().pick_file();
        self.dialog_open = false;

        if let Some(path) = path {
            self.open_path(path);
        }
    }
    pub fn open_folder_dialog(&mut self) {
        self.stop_slideshow();

        self.dialog_open = true;
        let path = rfd::FileDialog::new().pick_folder();
        self.dialog_open = false;

        if let Some(path) = path {
            self.open_path(path);
        }
    }

    pub fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
    }

    pub fn save_window_state(&mut self, ctx: &egui::Context) {
        // Get the outer position (top‑left corner of the whole window)
        // and the inner size (client/content area).
        let (pos, size) = ctx.input(|i| {
            let outer_pos = i.viewport().outer_rect.map(|rect| rect.min);
            let inner_size = i.viewport().inner_rect.map(|rect| rect.size());
            (outer_pos, inner_size)
        });

        // Only save if both are available and not fullscreen.
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if !is_fullscreen {
            if let (Some(pos), Some(size)) = (pos, size) {
                if size.x > 100.0 && size.y > 100.0 {
                    let pos_arr = [pos.x, pos.y];
                    let size_arr = [size.x, size.y];
                    if let Err(e) = self.settings_manager.update_window_state(pos_arr, size_arr) {
                        eprintln!("Failed to save window state: {}", e);
                    }
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
                self.file_type_detection = None; // Clear file extension detection
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
        self.set_loading_state(LoadingState::Idle);

        // Reset preload state
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
        self.set_file_type_detection(None); // Clear file extension detection

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

    pub fn copy_path_to_clipboard(&self) {
        if let Some(path) = &self.current_image_path {
            let path_str = path.display().to_string();

            let mut clipboard = Clipboard::new().expect("Failed to open clipboard");

            println!("Copied path: {}", path_str);

            if let Err(e) = clipboard.set_text(path_str) {
                eprintln!("Failed to copy path to clipboard: {}", e);
            }
        } else {
            println!("No image path to copy");
        }
    }

    pub fn copy_image_to_clipboard(&mut self) {
        // Get the current image.
        let image = if let Some(path) = &self.current_image_path {
            match crate::loader::load_full_resolution(path.clone()) {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("Failed to load image for clipboard: {}", e);
                    return;
                }
            }
        } else if let Some(gif) = &self.gif_animation {
            match gif.get_current_frame_ref() {
                Some(frame) => image::DynamicImage::ImageRgba8(frame.clone()),
                None => {
                    eprintln!("GIF has no current frame");
                    return;
                }
            }
        } else {
            println!("No image to copy");
            return;
        };

        // Convert to RGBA8.
        //
        // arboard expects raw RGBA pixels, so there is no need
        // to encode the image to PNG ourselves.
        let rgba = image.to_rgba8();

        let width = rgba.width() as usize;
        let height = rgba.height() as usize;

        let mut clipboard = match Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(e) => {
                eprintln!("Failed to open clipboard: {}", e);
                return;
            }
        };

        let image_data = arboard::ImageData {
            width,
            height,
            bytes: rgba.into_raw().into(),
        };

        match clipboard.set_image(image_data) {
            Ok(_) => {
                println!("Image copied to clipboard");
            }
            Err(e) => {
                eprintln!("Failed to copy image to clipboard: {}", e);
            }
        }
    }
}
