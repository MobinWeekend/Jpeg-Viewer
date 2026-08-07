use super::types::{CachedImage, LoadedImage, ViewerApp};
use crate::image_entry::ImageEntry;
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
                };
                self.image_cache.put(image_id, cached);
            }
        }
    }

    pub fn load_from_cache(&mut self, index: usize) -> bool {
        // Check if this is a GIF - we don't cache GIFs
        if let Some(entry) = self.image_entries.get(index) {
            let is_gif = match entry {
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

            // GIFs are not cached - return false to force loading
            if is_gif {
                return false;
            }
        }

        // Get the image ID once and store it
        let image_id = match self.get_image_id(index) {
            Some(id) => id,
            None => return false,
        };

        // Check if the image is in cache
        if let Some(cached) = self.image_cache.get(&image_id) {
            // Check if the index matches
            if cached.index != index {
                // Index mismatch - need to update it
                let cached_clone = cached.clone();
                
                // Remove the old entry
                self.image_cache.pop(&image_id);
                
                // Re-insert with correct index
                let updated = CachedImage {
                    index,
                    ..cached_clone
                };
                self.image_cache.put(image_id.clone(), updated);
                
                // Now get the updated entry
                if let Some(updated_cached) = self.image_cache.get(&image_id) {
                    self.texture = Some(updated_cached.texture.clone());
                    self.is_gif = updated_cached.is_gif;
                    self.is_preview = updated_cached.is_preview;
                    self.b_fit_to_window = true;
                    self.b_is_loading = false;
                    return true;
                }
                return false;
            }
            
            // Index matches - use it directly
            self.texture = Some(cached.texture.clone());
            self.is_gif = cached.is_gif;
            self.is_preview = cached.is_preview;
            self.b_fit_to_window = true;
            self.b_is_loading = false;
            return true;
        }
        false
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
            let cached = CachedImage {
                texture,
                is_gif: matches!(loaded_image, LoadedImage::Animated(_, _)),
                is_preview: matches!(loaded_image, LoadedImage::Animated(_, true)),
                index,
            };
            self.image_cache.put(image_id, cached);
        }
    }

    pub fn is_index_cached(&self, index: usize) -> bool {
        // Safety check: ensure index is valid
        if index >= self.image_entries.len() {
            return false;
        }

        // GIFs are never cached
        if let Some(entry) = self.image_entries.get(index) {
            let is_gif = match entry {
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
                return false;
            }
        }

        if let Some(image_id) = self.get_image_id(index) {
            self.image_cache.contains(&image_id)
        } else {
            false
        }
    }
}