use super::texture_mipmap::{
    SharedMipmapState, start_mipmap_upload, start_mipmap_upload_from_entry,
};
use super::types::{CachedImage, LoadedImage, ViewerApp};
use crate::app::constants::MAX_TILE_SIZE;
use crate::imaging::core::DecodedImage;
use eframe::egui;
use std::sync::Arc;

/// Result of consuming the current decoded static image. The decoded RGBA buffer
/// is shared with background mip generation and the decorative-backdrop worker;
/// it is never cloned into a second full-size allocation on the UI thread.
pub(crate) struct PreparedStaticImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    pub texture: egui::TextureHandle,
}

impl ViewerApp {
    pub fn get_image_id(&self, index: usize) -> Option<String> {
        self.image_entries.get(index).map(|entry| entry.get_id())
    }

    pub fn cache_current_image(&mut self) {
        // Don't cache GIFs - they take too much time or extension-mismatched
        // images whose type must be checked again on the next open.
        if self.is_gif || self.current_file_type_mismatch() {
            return;
        }

        let Some(image_id) = self.get_image_id(self.current_index) else {
            return;
        };
        // Navigation intentionally keeps the outgoing texture visible until the
        // new decode completes. Never cache that placeholder under the incoming
        // index (for example, an outgoing image's mip worker may finish meanwhile).
        if self.texture_image_id.as_deref() != Some(image_id.as_str()) {
            return;
        }
        let Some(texture) = self.texture.as_ref() else {
            return;
        };
        let texture_filter = self.settings_manager.get().texture_filter.clone();
        let mipmap_state = ready_mipmap_state(&self.current_mipmap_state);

        // Once a ready texture/state pair is in the cache, don't put the same
        // resource back into the LRU every frame just because the mip callback
        // has completed.
        if self.image_cache.peek(&image_id).is_some_and(|cached| {
            cached.texture.id() == texture.id()
                && cached.texture_filter == texture_filter
                && same_mipmap_state(&cached.mipmap_state, &mipmap_state)
        }) {
            return;
        }

        self.image_cache.put(
            image_id,
            CachedImage {
                texture: texture.clone(),
                texture_filter,
                mipmap_state,
                is_gif: false,
                is_preview: self.is_preview,
                index: self.current_index,
                file_type_detection: self.file_type_detection.clone(),
            },
        );
    }

    /// Promote a completed mipmap state into the GPU texture cache. Incomplete
    /// workers are intentionally kept only by the currently displayed image;
    /// navigation drops them rather than pinning large pending mip buffers in
    /// every cached neighbour.
    pub(crate) fn cache_ready_current_mipmap(&mut self) {
        let ready = self.current_mipmap_state.as_ref().is_some_and(|state| {
            state.lock().map(|state| state.is_ready()).unwrap_or(false)
        });
        if ready {
            self.cache_current_image();
        }
    }

    pub fn load_from_cache(&mut self, index: usize) -> bool {
        let image_id = match self.get_image_id(index) {
            Some(id) => id,
            None => return false,
        };

        if let Some(mut cached) = self.image_cache.get(&image_id).cloned() {
            let active_filter = self.settings_manager.get().texture_filter.as_str();
            if cached.texture_filter != active_filter {
                // TextureOptions are applied when the GPU texture is created.
                // Reusing a handle made with another filter would silently keep
                // the old filtering mode active.
                self.image_cache.pop(&image_id);
                return false;
            }

            if cached.index != index {
                cached.index = index;
                if let Some(detection) = &mut cached.file_type_detection {
                    detection.index = index;
                }
            }

            // Preload workers intentionally cache only the base texture to keep
            // CPU memory bounded. If this entry becomes current with Mipmap
            // selected, decode it again on a worker while its cached base texture
            // is displayed immediately.
            let mut current_mipmap_state = cached.mipmap_state.clone();
            if active_filter == "mipmap" && current_mipmap_state.is_none() {
                let entry = self.image_entries.get(index).cloned();
                if let Some(entry) = entry {
                    current_mipmap_state = Some(start_mipmap_upload_from_entry(
                        entry,
                        self.settings_manager.get().virtual_texture_threshold,
                    ));
                }
            }

            self.texture = Some(cached.texture.clone());
            self.texture_image_id = Some(image_id.clone());
            self.current_mipmap_state = current_mipmap_state.clone();
            // Keep incomplete mip workers attached only to the displayed image.
            // A pending decode can hold a large RGBA buffer; if the user navigates
            // away, dropping the current state should stop the worker promptly.
            cached.mipmap_state = ready_mipmap_state(&current_mipmap_state);
            self.mark_win98_dialup_texture_ready(true);
            self.is_gif = cached.is_gif;
            self.is_preview = cached.is_preview;
            self.b_fit_to_window = true;
            self.file_type_detection = cached.file_type_detection.clone();
            self.image_cache.put(image_id, cached);
            return true;
        }
        false
    }

    pub fn is_index_cached(&self, index: usize) -> bool {
        if index >= self.image_entries.len() {
            return false;
        }
        if let Some(image_id) = self.get_image_id(index) {
            self.image_cache.peek(&image_id).is_some_and(|cached| {
                cached.texture_filter.as_str()
                    == self.settings_manager.get().texture_filter.as_str()
            })
        } else {
            false
        }
    }

    /// Create or reuse the canonical texture for an image that has just become
    /// current. This path may attach a mip worker because the decoded source
    /// pixels are already available and are being consumed exactly once.
    pub(crate) fn prepare_current_static_image(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        image: DecodedImage,
    ) -> PreparedStaticImage {
        let width = image.width;
        let height = image.height;
        let rgba = Arc::new(image.data);
        let texture_filter = self.settings_manager.get().texture_filter.clone();
        let image_id = self.get_image_id(index);

        let should_cache = index < self.image_entries.len()
            && !self.current_file_type_mismatch()
            && !self.is_virtual_texture_size(width, height);

        let existing = if should_cache {
            image_id
                .as_deref()
                .and_then(|id| self.image_cache.get(id).cloned())
                .filter(|cached| cached.texture_filter == texture_filter)
        } else {
            None
        };

        if let (Some(id), Some(_)) = (image_id.as_deref(), existing.as_ref()) {
            // Touch the entry in the LRU without changing its texture handle.
            let _ = self.image_cache.get(id);
        } else if let Some(id) = image_id.as_deref() {
            self.image_cache.pop(id);
        }

        let texture = if let Some(cached) = existing.as_ref() {
            cached.texture.clone()
        } else {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                rgba.as_slice(),
            );
            ctx.load_texture(
                format!("image_{}", image_id.as_deref().unwrap_or("current")),
                color,
                self.get_texture_base_options(),
            )
        };

        // From this point the returned texture represents this exact entry,
        // even when its base texture came from the preload cache.
        self.texture_image_id = image_id.clone();

        let mipmap_state = if texture_filter == "mipmap" {
            existing
                .as_ref()
                .and_then(|cached| cached.mipmap_state.clone())
                .or_else(|| Some(start_mipmap_upload(width, height, Arc::clone(&rgba))))
        } else {
            None
        };

        if should_cache {
            if let Some(image_id) = image_id {
                let cached = CachedImage {
                    texture: texture.clone(),
                    texture_filter,
                    mipmap_state: ready_mipmap_state(&mipmap_state),
                    is_gif: false,
                    is_preview: false,
                    index,
                    file_type_detection: self.file_type_detection.clone(),
                };
                self.image_cache.put(image_id, cached);
            }
        }

        self.current_mipmap_state = mipmap_state.clone();
        PreparedStaticImage {
            width,
            height,
            rgba,
            texture,
        }
    }

    /// Add a background-preloaded image to the GPU cache. Mipmap preparation is
    /// deliberately deferred until this entry is selected, avoiding retention
    /// of a full decoded RGBA buffer for every neighbour in the preload radius.
    pub fn add_to_cache(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        loaded_image: &LoadedImage,
    ) {
        if index >= self.image_entries.len() || index == self.current_index {
            return;
        }

        let img = match loaded_image {
            LoadedImage::Static(img) => img,
            LoadedImage::Animated(_, _) | LoadedImage::VirtualPending(_, _, _) => return,
        };
        if self.is_virtual_texture_size(img.width(), img.height()) {
            return;
        }

        let Some(image_id) = self.get_image_id(index) else {
            return;
        };
        let texture_filter = self.settings_manager.get().texture_filter.clone();
        if self
            .image_cache
            .peek(&image_id)
            .is_some_and(|cached| cached.texture_filter == texture_filter)
        {
            return;
        }
        self.image_cache.pop(&image_id);

        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.data(),
        );
        let texture = ctx.load_texture(
            format!("image_cache_{image_id}"),
            color,
            self.get_texture_base_options(),
        );
        self.image_cache.put(
            image_id,
            CachedImage {
                texture,
                texture_filter,
                mipmap_state: None,
                is_gif: false,
                is_preview: false,
                index,
                file_type_detection: None,
            },
        );
    }

    fn current_file_type_mismatch(&self) -> bool {
        self.file_type_detection
            .as_ref()
            .is_some_and(|detection| detection.mismatch)
    }

    fn is_virtual_texture_size(&self, width: u32, height: u32) -> bool {
        let threshold = self.settings_manager.get().virtual_texture_threshold;
        width > threshold
            || height > threshold
            || width > MAX_TILE_SIZE
            || height > MAX_TILE_SIZE
    }
}


fn ready_mipmap_state(state: &Option<SharedMipmapState>) -> Option<SharedMipmapState> {
    state.as_ref().and_then(|state| {
        state
            .lock()
            .map(|guard| guard.is_ready())
            .unwrap_or(false)
            .then(|| Arc::clone(state))
    })
}

fn same_mipmap_state(
    left: &Option<SharedMipmapState>,
    right: &Option<SharedMipmapState>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}
