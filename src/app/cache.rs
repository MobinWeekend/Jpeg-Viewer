use super::texture_mipmap::{
    SharedMipmapState, is_mipmap_state_pending, start_mipmap_upload,
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
        // Keep in-progress work attached to the cache entry. When navigation
        // moves away, the worker can continue preparing this neighbour in the
        // background instead of being discarded with `current_mipmap_state`.
        let mipmap_state = self.current_mipmap_state.clone();

        // Don't put the same texture/state pair back into the LRU every frame
        // just because the mip callback has progressed or completed.
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

    /// Ensure a completed current mipmap state is represented in the cache.
    /// In-progress states are cached too, allowing neighbouring images to finish
    /// preparing while they are not currently displayed.
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

            // Background preparation is scheduled centrally so there is only one
            // speculative mip job at a time. Reuse any state already attached to
            // this cached texture; if it has none, the scheduler will prioritize
            // it once the current background job has finished.
            let current_mipmap_state = cached.mipmap_state.clone();

            self.texture = Some(cached.texture.clone());
            self.texture_image_id = Some(image_id.clone());
            self.current_mipmap_state = current_mipmap_state.clone();
            // Preserve incomplete work when the texture becomes current. The main
            // viewer callback will continue uploading it with foreground priority.
            cached.mipmap_state = current_mipmap_state.clone();
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
                .or_else(|| {
                    // Do not launch another expensive mip worker while a cached
                    // neighbour is already being prepared. The background
                    // scheduler will prioritize this current image as soon as
                    // that worker completes. Non-cacheable images still start
                    // immediately because they have no cache entry to schedule.
                    if !should_cache || !self.has_pending_mipmap_work() {
                        Some(start_mipmap_upload(width, height, Arc::clone(&rgba)))
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        if should_cache {
            if let Some(image_id) = image_id {
                let cached = CachedImage {
                    texture: texture.clone(),
                    texture_filter,
                    mipmap_state: mipmap_state.clone(),
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

    /// Add a background-preloaded image to the GPU cache. Its base texture is
    /// uploaded immediately; mip preparation is scheduled later, one cached
    /// neighbour at a time, by re-decoding the source on a worker. This avoids
    /// retaining a full decoded RGBA buffer for every neighbour in the radius.
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


impl ViewerApp {
    /// True while any current, inspect, or cached texture still has mip levels
    /// waiting to be generated or uploaded. This bounds speculative work to one
    /// texture at a time and prevents cache-radius preloading from spawning a
    /// burst of full-image mip workers.
    pub(crate) fn has_pending_mipmap_work(&self) -> bool {
        self.current_mipmap_state
            .as_ref()
            .is_some_and(is_mipmap_state_pending)
            || self
                .inspect_mipmap_state
                .as_ref()
                .is_some_and(is_mipmap_state_pending)
            || self.image_cache.iter().any(|(_, cached)| {
                cached
                    .mipmap_state
                    .as_ref()
                    .is_some_and(is_mipmap_state_pending)
            })
    }
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
