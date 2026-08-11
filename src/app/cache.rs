// src/app/cache.rs
use super::types::{CachedImage, LoadedImage, ViewerApp};
use eframe::egui;

impl ViewerApp {
    pub fn get_image_id(&self, index: usize) -> Option<String> {
        self.image_entries.get(index).map(|entry| entry.get_id())
    }

    pub fn cache_current_image(&mut self) {
        // Don't cache GIFs - they take too much time!
        if self.is_gif {
            return;
        }

        if let Some(texture) = &self.texture {
            if let Some(image_id) = self.get_image_id(self.current_index) {
                let cached = CachedImage {
                    texture: texture.clone(),
                    is_gif: self.is_gif,
                    is_preview: self.is_preview,
                    index: self.current_index,
                    file_type_detection: self.file_type_detection.clone(), // Store detection
                };
                self.image_cache.put(image_id, cached);
            }
        }
    }

    pub fn load_from_cache(&mut self, index: usize) -> bool {
        let image_id = match self.get_image_id(index) {
            Some(id) => id,
            None => return false,
        };

        if let Some(cached) = self.image_cache.get(&image_id) {
            if cached.index != index {
                let cached_clone = cached.clone();
                self.image_cache.pop(&image_id);
                let updated = CachedImage {
                    index,
                    ..cached_clone
                };
                self.image_cache.put(image_id.clone(), updated);
                if let Some(updated_cached) = self.image_cache.get(&image_id) {
                    self.texture = Some(updated_cached.texture.clone());
                    self.is_gif = updated_cached.is_gif;
                    self.is_preview = updated_cached.is_preview;
                    self.b_fit_to_window = true;
                    self.b_is_loading = false;
                    
                    // Restore file type detection from cache
                    self.file_type_detection = updated_cached.file_type_detection.clone();
                    
                    self.detect_current_file_type(); // Verify/update detection
                    return true;
                }
                return false;
            }
            self.texture = Some(cached.texture.clone());
            self.is_gif = cached.is_gif;
            self.is_preview = cached.is_preview;
            self.b_fit_to_window = true;
            self.b_is_loading = false;
            
            // Restore file type detection from cache
            self.file_type_detection = cached.file_type_detection.clone();
            
            self.detect_current_file_type(); // Verify/update detection
            return true;
        }
        false
    }

    pub fn is_index_cached(&self, index: usize) -> bool {
        if index >= self.image_entries.len() {
            return false;
        }
        if let Some(image_id) = self.get_image_id(index) {
            self.image_cache.contains(&image_id)
        } else {
            false
        }
    }

    pub fn add_to_cache(&mut self, ctx: &egui::Context, index: usize, loaded_image: LoadedImage) {
        // Don't cache GIFs
        if matches!(loaded_image, LoadedImage::Animated(_, _)) {
            return;
        }

        // Safety check: ensure index is valid
        if index >= self.image_entries.len() {
            return;
        }

        let image_id = match self.get_image_id(index) {
            Some(id) => id,
            None => return,
        };

        // If we already have this in cache, skip
        if self.image_cache.contains(&image_id) {
            return;
        }

        let texture = match &loaded_image {
            LoadedImage::Static(img) => {
                let rgba = img.to_rgba8();
                let width = rgba.width();
                let height = rgba.height();

                // Add size validation - skip caching if too large
                const MAX_CACHE_SIZE: u32 = 18000;
                const MAX_CACHE_PIXELS: u64 = 250_000_000;

                if width > MAX_CACHE_SIZE || height > MAX_CACHE_SIZE {
                    return;
                }

                let pixels = width as u64 * height as u64;
                if pixels > MAX_CACHE_PIXELS {
                    return;
                }

                let size = [width as usize, height as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());

                let options = self.get_texture_options();
                Some(ctx.load_texture(&format!("cache_{}", image_id), color, options))
            }
            LoadedImage::Animated(gif, _) => {
                // This shouldn't be reached since we skip GIFs above
                let frame = gif.get_current_frame_ref();
                if let Some(frame) = frame {
                    let size = [frame.width() as usize, frame.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());
                    Some(ctx.load_texture(
                        &format!("cache_{}", image_id),
                        color,
                        Default::default(),
                    ))
                } else {
                    None
                }
            }
        };

        if let Some(texture) = texture {
            // Get the current file type detection for this image
            let detection = if index == self.current_index {
                self.file_type_detection.clone()
            } else {
                // For preloaded images, we don't have detection yet - it will be set when loaded
                None
            };
            
            let cached = CachedImage {
                texture,
                is_gif: matches!(loaded_image, LoadedImage::Animated(_, _)),
                is_preview: matches!(loaded_image, LoadedImage::Animated(_, true)),
                index,
                file_type_detection: detection,
            };
            self.image_cache.put(image_id, cached);
        }
    }
}