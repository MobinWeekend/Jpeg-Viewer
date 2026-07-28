use super::types::{LoadedImage, ViewerApp};
use crate::image_entry::ImageEntry;
use rayon::spawn;
use std::path::PathBuf;
use std::sync::mpsc::channel;

impl ViewerApp {
    pub fn load_image(&mut self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            self.current_directory = Some(parent.to_path_buf());

            let files = crate::loader::load_directory_images(parent);

            if let Some(index) = files.iter().position(|p| p == &path) {
                let entries = files.into_iter().map(ImageEntry::File).collect();
                self.set_image_entries(entries, index);
            }
        }
    }

    pub fn load_current_image(&mut self) {
        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        } else {
            self.current_image_path = None;
        }

        self.b_is_loading = true;
        self.b_is_loading_full = false;
        self.is_preview = false;
        self.gif_animation = None;
        self.is_gif = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;

        self.b_fit_to_window = true;
        let (tx, rx) = channel();

        // Check if this is a GIF
        let is_gif = match &entry {
            ImageEntry::File(path) => {
                if let Some(ext) = path.extension() {
                    ext.eq_ignore_ascii_case("gif")
                } else {
                    false
                }
            }
            ImageEntry::Zip(zip) => zip.name.to_lowercase().ends_with(".gif"),
            ImageEntry::S7z(s7z) => s7z.name.to_lowercase().ends_with(".gif"),
            ImageEntry::Rar(rar) => rar.name.to_lowercase().ends_with(".gif"),
        };

        if is_gif {
            // For GIFs: First load preview frame immediately, then full GIF in background
            let entry_clone = entry.clone();
            let (preview_tx, preview_rx) = channel();
            
            // Load preview frame immediately
            spawn(move || {
                let preview = match entry_clone {
                    ImageEntry::File(path) => {
                        crate::loader::load_gif_preview(path)
                            .map(|g| LoadedImage::Animated(g, true))
                    }
                    ImageEntry::Zip(zip) => {
                        crate::loader::load_zip_gif_preview(zip)
                            .map(|g| LoadedImage::Animated(g, true))
                    }
                    ImageEntry::S7z(s7z) => {
                        crate::loader::load_7z_gif_preview(s7z)
                            .map(|g| LoadedImage::Animated(g, true))
                    }
                    ImageEntry::Rar(rar) => {
                        crate::loader::load_rar_gif_preview(rar)
                            .map(|g| LoadedImage::Animated(g, true))
                    }
                };
                if let Some(img) = preview {
                    let _ = preview_tx.send(img);
                }
            });

            // Set up receiver for preview
            self.receiver = Some(preview_rx);
            
            // Start loading full GIF in background
            let entry_clone2 = entry.clone();
            let (full_tx, full_rx) = channel();
            spawn(move || {
                let full = match entry_clone2 {
                    ImageEntry::File(path) => {
                        crate::loader::load_gif(path)
                            .map(|g| LoadedImage::Animated(g, false))
                    }
                    ImageEntry::Zip(zip) => {
                        crate::loader::load_zip_gif(zip)
                            .map(|g| LoadedImage::Animated(g, false))
                    }
                    ImageEntry::S7z(s7z) => {
                        crate::loader::load_7z_gif(s7z)
                            .map(|g| LoadedImage::Animated(g, false))
                    }
                    ImageEntry::Rar(rar) => {
                        crate::loader::load_rar_gif(rar)
                            .map(|g| LoadedImage::Animated(g, false))
                    }
                };
                if let Some(img) = full {
                    let _ = full_tx.send(img);
                }
            });
            
            self.full_gif_receiver = Some(full_rx);
            
        } else {
            // Non-GIF: load full resolution directly
            spawn(move || {
                let loaded = match entry {
                    ImageEntry::File(path) => {
                        crate::loader::load_full_resolution(path).map(LoadedImage::Static)
                    }
                    ImageEntry::Zip(zip) => {
                        crate::loader::load_zip_image(zip).map(LoadedImage::Static)
                    }
                    ImageEntry::S7z(s7z) => {
                        crate::loader::load_7z_image(s7z).map(LoadedImage::Static)
                    }
                    ImageEntry::Rar(rar) => {
                        crate::loader::load_rar_image(rar).map(LoadedImage::Static)
                    }
                };

                if let Some(img) = loaded {
                    let _ = tx.send(img);
                }
            });

            self.receiver = Some(rx);
        }
    }

    pub fn load_current_image_with_cache(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.b_is_loading = false;
            return;
        }
        self.load_current_image();
    }

    pub fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.image_entries = entries;
        self.current_index = current_index;
        self.preload_origin = current_index;
        self.delta_threshold = ((self.cache_radius as f32 * 3.0 / 5.0).round() as usize).max(1);
        self.b_fit_to_window = true;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.load_current_image();
    }

    pub fn load_directory(&mut self, path: &PathBuf) {
        self.current_directory = Some(path.clone());
        let files = crate::loader::load_directory_images(path);
        if files.is_empty() {
            println!("No images found in directory: {:?}", path);
            return;
        }
        let entries = files.into_iter().map(ImageEntry::File).collect();

        self.set_image_entries(entries, 0);
        self.zoom = 1.0;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
    }
}