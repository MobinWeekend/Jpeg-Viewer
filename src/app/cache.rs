use super::types::{CachedImage, LoadedImage, ViewerApp};
use crate::app::constants::MAX_TILE_SIZE;
use eframe::egui;
use image::GenericImageView;

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

        if let Some(cached) = self.image_cache.get(&image_id).cloned() {
            if cached.index != index {
                let mut updated_cached = cached.clone();
                updated_cached.index = index;
                if let Some(detection) = &mut updated_cached.file_type_detection {
                    detection.index = index;
                }
                self.image_cache.pop(&image_id);
                self.image_cache.put(image_id.clone(), updated_cached);
                if let Some(updated_cached) = self.image_cache.get(&image_id).cloned() {
                    self.texture = Some(updated_cached.texture.clone());
                    self.is_gif = updated_cached.is_gif;
                    self.is_preview = updated_cached.is_preview;
                    self.b_fit_to_window = true;
                    // Restore file type detection from cache
                    self.file_type_detection = updated_cached.file_type_detection.clone();
                    //self.detect_current_file_type(); // Verify/update detection
                    return true;
                }
                return false;
            }
            self.texture = Some(cached.texture);
            self.is_gif = cached.is_gif;
            self.is_preview = cached.is_preview;
            self.b_fit_to_window = true;

            // Restore file type detection from cache
            self.file_type_detection = cached.file_type_detection.clone();

            //self.detect_current_file_type(); // Verify/update detection
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
        // Safety check: ensure index is valid
        if index >= self.image_entries.len() {
            return;
        }

        // Don't cache GIFs
        if matches!(loaded_image, LoadedImage::Animated(_, _)) {
            return;
        }

        if index == self.current_index {
            if let Some(detection) = &self.file_type_detection {
                if detection.mismatch {
                    // Don't cache files with wrong extension – they need fresh detection every time.
                    return;
                }
            }
        }

        // Skip caching if the image would be handled by virtual texturing
        if let LoadedImage::Static(img) = &loaded_image {
            let (width, height) = img.dimensions();
            let settings = self.settings_manager.get();
            let threshold = settings.virtual_texture_threshold;
            let use_virtual = width > threshold
                || height > threshold
                || width > MAX_TILE_SIZE
                || height > MAX_TILE_SIZE;

            if use_virtual {
                // Large image – return pending virtual texture
                return; // skip caching for large images
            }
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

                // Size validation - skip caching if too large
                let settings = self.settings_manager.get();
                let threshold = settings.virtual_texture_threshold;

                if width > threshold
                    || height > threshold
                    || width > MAX_TILE_SIZE
                    || height > MAX_TILE_SIZE
                {
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
                let options = self.get_texture_options();
                if let Some(frame) = frame {
                    let size = [frame.width() as usize, frame.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());
                    Some(ctx.load_texture(&format!("cache_{}", image_id), color, options))
                } else {
                    None
                }
            }
            LoadedImage::VirtualPending(_, _, _) => None,
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
