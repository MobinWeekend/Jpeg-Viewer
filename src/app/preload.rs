use super::types::{CachedImage, LoadedImage, PreloadTask, ViewerApp};
use crate::image_entry::ImageEntry;
use eframe::egui;
use rayon::spawn;
use std::num::NonZeroUsize;
use std::sync::mpsc::channel;
use std::time::Instant;

impl ViewerApp {
    pub fn preload_adjacent_images(&mut self, ctx: &egui::Context) {
        // Check if caching should be stopped
        if self.should_stop_caching {
            return;
        }

        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        if len <= 1 {
            return;
        }

        // Calculate delta as 3/5 of radius, minimum 1
        let delta_threshold = ((self.cache_radius as f32 * 3.0 / 5.0).round() as usize).max(1);
        self.delta_threshold = delta_threshold;

        // Check if user has stopped navigating or gone through the range
        let should_update_origin = if let Some(timer) = self.navigation_timer {
            // If user has paused for more than the pause duration, update origin
            if timer.elapsed() >= self.navigation_pause_duration {
                true
            } else {
                // Check if user has gone through the entire delta range
                let dist = (self.current_index as i32 - self.preload_origin as i32).abs();
                dist >= delta_threshold as i32
            }
        } else {
            // First navigation - update origin
            true
        };

        if should_update_origin {
            // Reset the navigation timer
            self.navigation_timer = None;
            
            // Update origin to current index
            self.preload_origin = self.current_index;
            self.clean_cache_outside_radius();
        }

        // Determine which indices to preload
        let mut indices_to_preload = Vec::new();

        // Always preload current image first (highest priority)
        if !self.preloading_indices.contains(&self.current_index) {
            indices_to_preload.push(self.current_index);
        }

        // Preload delta range (3/5 of radius) around the origin
        for offset in 1..=delta_threshold {
            let fwd_idx = (self.preload_origin + offset) % len;
            if !self.is_index_cached(fwd_idx) && !self.preloading_indices.contains(&fwd_idx) {
                indices_to_preload.push(fwd_idx);
            }

            let bwd_idx = (self.preload_origin + len - offset) % len;
            if !self.is_index_cached(bwd_idx) && !self.preloading_indices.contains(&bwd_idx) {
                indices_to_preload.push(bwd_idx);
            }
        }

        // After current image and delta range are loading,
        // also preload the rest of the full radius range (lower priority)
        for offset in (delta_threshold + 1)..=self.cache_radius {
            let fwd_idx = (self.preload_origin + offset) % len;
            if !self.is_index_cached(fwd_idx) && !self.preloading_indices.contains(&fwd_idx) {
                indices_to_preload.push(fwd_idx);
            }

            let bwd_idx = (self.preload_origin + len - offset) % len;
            if !self.is_index_cached(bwd_idx) && !self.preloading_indices.contains(&bwd_idx) {
                indices_to_preload.push(bwd_idx);
            }
        }

        // Limit concurrent preloading
        let max_concurrent: usize = 4;
        let current_tasks = self.preload_tasks.len();
        let available_slots =
            max_concurrent.saturating_sub(current_tasks);
        
        if available_slots > 0 && !indices_to_preload.is_empty() {
            let to_load: Vec<_> = indices_to_preload
                .into_iter()
                .take(available_slots)
                .collect();

            for idx in to_load {
                self.start_preload_task(idx);
            }
        }

        ctx.request_repaint();
    }

    pub fn reset_navigation_timer(&mut self) {
        // Reset the timer when user navigates
        self.navigation_timer = Some(Instant::now());
    }

    fn start_preload_task(&mut self, idx: usize) {
        if self.should_stop_caching {
            return;
        }

        if let Some(entry) = self.image_entries.get(idx).cloned() {
            self.preloading_indices.insert(idx);

            let (tx, rx) = channel();

            spawn(move || {
                let loaded = match entry {
                    ImageEntry::File(path) => {
                        if let Some(ext) = path.extension() {
                            if ext.eq_ignore_ascii_case("gif") {
                                crate::loader::load_gif_preview(path)
                                    .map(|g| LoadedImage::Animated(g, true))
                            } else {
                                crate::loader::load_full_resolution(path)
                                    .map(LoadedImage::Static)
                            }
                        } else {
                            crate::loader::load_full_resolution(path)
                                .map(LoadedImage::Static)
                        }
                    }
                    ImageEntry::Zip(zip) => {
                        if zip.name.to_lowercase().ends_with(".gif") {
                            crate::loader::load_zip_gif_preview(zip)
                                .map(|g| LoadedImage::Animated(g, true))
                        } else {
                            crate::loader::load_zip_image(zip).map(LoadedImage::Static)
                        }
                    }
                    ImageEntry::S7z(s7z) => {
                        if s7z.name.to_lowercase().ends_with(".gif") {
                            crate::loader::load_7z_gif_preview(s7z)
                                .map(|g| LoadedImage::Animated(g, true))
                        } else {
                            crate::loader::load_7z_image(s7z).map(LoadedImage::Static)
                        }
                    }
                    ImageEntry::Rar(rar) => {
                        if rar.name.to_lowercase().ends_with(".gif") {
                            crate::loader::load_rar_gif_preview(rar)
                                .map(|g| LoadedImage::Animated(g, true))
                        } else {
                            crate::loader::load_rar_image(rar).map(LoadedImage::Static)
                        }
                    }
                };

                if let Some(img) = loaded {
                    let _ = tx.send(img);
                }
            });

            self.preload_tasks.push(PreloadTask {
                index: idx,
                receiver: rx,
            });
        }
    }

    fn clean_cache_outside_radius(&mut self) {
        if self.should_stop_caching {
            return;
        }

        let len = self.image_entries.len();
        if len == 0 {
            return;
        }

        let mut keep_indices = std::collections::HashSet::new();
        
        for offset in 0..=self.cache_radius {
            let idx = (self.preload_origin + offset) % len;
            keep_indices.insert(idx);
            let idx = (self.preload_origin + len - offset) % len;
            keep_indices.insert(idx);
        }

        // ALWAYS keep the current image in cache regardless of range
        keep_indices.insert(self.current_index);

        let mut to_remove = Vec::new();
        for (id, _) in self.image_cache.iter() {
            if let Some(index) = self.get_index_from_id(id) {
                if !keep_indices.contains(&index) {
                    to_remove.push(id.clone());
                }
            }
        }

        for id in to_remove {
            self.image_cache.pop(&id);
        }
    }

    fn get_index_from_id(&self, id: &str) -> Option<usize> {
        for (idx, entry) in self.image_entries.iter().enumerate() {
            if entry.get_id() == id {
                return Some(idx);
            }
        }
        None
    }

    pub fn process_preload_tasks(&mut self, ctx: &egui::Context) {
        if self.should_stop_caching {
            // Clear all pending tasks
            self.preload_tasks.clear();
            self.preloading_indices.clear();
            return;
        }

        let mut completed_indices = Vec::new();
        let mut completed_images = Vec::new();

        for task in &mut self.preload_tasks {
            if let Ok(loaded_image) = task.receiver.try_recv() {
                completed_indices.push(task.index);
                completed_images.push((task.index, loaded_image));
            }
        }

        self.preload_tasks
            .retain(|task| !completed_indices.contains(&task.index));

        for idx in &completed_indices {
            self.preloading_indices.remove(idx);
        }

        for (idx, loaded_image) in completed_images {
            self.add_to_cache(ctx, idx, loaded_image);
        }

        if !self.image_entries.is_empty() && !self.b_is_loading {
            self.preload_adjacent_images(ctx);
        }
    }

    pub fn update_cache_radius(&mut self, new_radius: usize) {
        let radius = new_radius.max(1).min(100);
        if radius != self.cache_radius {
            self.cache_radius = radius;
            self.delta_threshold = ((radius as f32 * 3.0 / 5.0).round() as usize).max(1);
            
            let new_size = (radius * 2 + 1).max(3);
            self.max_cache_size = new_size;
            
            if let Some(non_zero) = NonZeroUsize::new(new_size) {
                let mut new_cache = lru::LruCache::new(non_zero);
                let entries: Vec<(String, CachedImage)> = self.image_cache
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (key, value) in entries {
                    new_cache.put(key, value);
                }
                self.image_cache = new_cache;
            }
            
            self.preload_origin = self.current_index;
            self.preloading_indices.clear();
            self.preload_tasks.clear();
            
            self.clean_cache_outside_radius();
            self.cache_current_image();
        }
    }

    pub fn get_cache_range(&self) -> usize {
        self.cache_radius
    }

    pub fn stop_caching(&mut self) {
        self.should_stop_caching = true;
        // Clear all pending tasks
        self.preload_tasks.clear();
        self.preloading_indices.clear();
        // Clear cache
        self.image_cache.clear();
    }
}