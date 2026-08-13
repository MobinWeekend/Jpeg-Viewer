// preloading governs the range and tasks and duration for preloading
use super::types::{CachedImage, LoadedImage, PreloadTask, ViewerApp};
use crate::image_entry::ImageEntry;
use crate::gif::detection::is_gif_entry;
use crate::gif::loader::{
    load_gif_preview_from_path,
    load_gif_preview_from_zip,
    load_gif_preview_from_7z,
    load_gif_preview_from_rar,
};
use eframe::egui;
use rayon::spawn;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const PRELOAD_TIMEOUT: Duration = Duration::from_secs(15);

impl ViewerApp {
    /// Get the index at a circular offset from origin
    fn get_circular_index(&self, origin: usize, offset: isize) -> usize {
        let len = self.image_entries.len();
        if len == 0 {
            return 0;
        }
        ((origin as isize + offset).rem_euclid(len as isize)) as usize
    }

    /// Build an ordered list of desired indices around the origin.
    /// Priority: current first, then origin, then alternating ±1, ±2, ...
    fn ordered_desired_window(&self) -> Vec<usize> {
        let mut desired = Vec::new();
        let len = self.image_entries.len();
        if len == 0 {
            return desired;
        }

        // Highest priority: current image
        desired.push(self.current_index);

        let mut added = HashSet::new();
        added.insert(self.current_index);

        // Second priority: origin (if different from current)
        if !added.contains(&self.preload_origin) {
            desired.push(self.preload_origin);
            added.insert(self.preload_origin);
        }

        // Then expand outward from origin: +1, -1, +2, -2, ...
        for offset in 1..=self.cache_radius as isize {
            // Forward from origin
            let idx = self.get_circular_index(self.preload_origin, offset);
            if !added.contains(&idx) {
                desired.push(idx);
                added.insert(idx);
            }

            // Backward from origin
            let idx = self.get_circular_index(self.preload_origin, -offset);
            if !added.contains(&idx) {
                desired.push(idx);
                added.insert(idx);
            }
        }

        desired
    }

    /// Build a set of all indices that should be kept in cache.
    /// This includes the origin window PLUS the current image.
    fn desired_set_from_origin(&self) -> HashSet<usize> {
        let mut desired = HashSet::new();
        let len = self.image_entries.len();
        if len == 0 {
            return desired;
        }

        // Include all indices within cache_radius of the preload_origin
        for offset in 0..=self.cache_radius as isize {
            let idx = self.get_circular_index(self.preload_origin, offset);
            desired.insert(idx);
            let idx = self.get_circular_index(self.preload_origin, -offset);
            desired.insert(idx);
        }

        // CRITICAL: Always keep the current image, even if outside the origin window
        desired.insert(self.current_index);

        desired
    }

    /// Check if we've moved far enough from origin to warrant moving it.
    /// This now requires the navigation pause timer to have elapsed.
    fn should_move_origin(&self) -> bool {
        let len = self.image_entries.len();
        if len <= 1 {
            return false;
        }

        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();
        let dist = diff.min(len - diff);

        dist >= self.delta_threshold
    }

    /// Move the preload origin to the current image and invalidate old results.
    fn move_preload_origin(&mut self) {
        self.preload_origin = self.current_index;
        self.navigation_timer = None;
        // Increment generation to discard stale results.
        // We do NOT clear preload_tasks or preloading_indices – old workers
        // are still tracked via preload_workers and remain counted until they finish.
        self.preload_generation = self.preload_generation.wrapping_add(1);
    }

    pub fn preload_adjacent_images(&mut self) {
        if self.should_stop_caching {
            return;
        }

        if self.image_entries.is_empty() || self.image_entries.len() <= 1 {
            return;
        }

        // Step 1: Update origin if we've moved far enough.
        // If we've moved more than twice the radius, force origin move immediately
        // (ignore navigation timer to keep up with fast navigation).
        let len = self.image_entries.len();
        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();
        let dist = diff.min(len - diff);

        if dist >= self.cache_radius * 2 {
            self.move_preload_origin();
        } else if self.should_move_origin() {
            self.move_preload_origin();
        }

        // Step 2: Build the desired window (both ordered and set).
        let desired_set = self.desired_set_from_origin();

        // Step 3: Enforce cache invariant – remove anything outside the desired set.
        self.clean_cache_outside_set(&desired_set);

        // Step 4: Start new preload tasks, respecting throttle and worker limit.
        if let Some(last) = self.last_preload_start {
            let throttle_ms = self.settings_manager.get().preload_throttle_ms;
            if last.elapsed() < Duration::from_millis(throttle_ms) {
                return;
            }
        }

        let max_concurrent = if self.is_loading() {
            self.max_cache_task.min(1)
        } else {
            self.max_cache_task
        } as usize;

        let active_workers = self.preload_workers;
        if active_workers >= max_concurrent {
            return;
        }

        let slots = max_concurrent - active_workers;

        let desired_ordered = self.ordered_desired_window();
        let mut started = 0;

        for idx in desired_ordered {
            if started >= slots {
                break;
            }

            // Skip GIFs – they are never cached
            if let Some(entry) = self.image_entries.get(idx) {
                if is_gif_entry(entry) {
                    continue;
                }
            }

            if self.is_index_cached(idx) {
                continue;
            }

            if self.preloading_indices.contains(&idx) {
                continue;
            }

            self.start_preload_task(idx);
            started += 1;
        }

        if started > 0 {
            self.last_preload_start = Some(Instant::now());
        }
    }

    /// Remove cached images that are outside the desired set.
    /// O(cache_size) because we use the index stored in the cache.
    fn clean_cache_outside_set(&mut self, desired: &HashSet<usize>) {
        if self.should_stop_caching {
            return;
        }

        let mut to_remove = Vec::new();

        for (id, cached) in self.image_cache.iter() {
            if !desired.contains(&cached.index) {
                to_remove.push(id.clone());
            }
        }

        if !to_remove.is_empty() {
            for id in to_remove {
                self.image_cache.pop(&id);
            }
        }
    }

    fn start_preload_task(&mut self, idx: usize) {
        if self.should_stop_caching {
            return;
        }

        if let Some(entry) = self.image_entries.get(idx).cloned() {
            // Mark as loading
            self.preloading_indices.insert(idx);
            self.preload_workers += 1;

            let generation = self.preload_generation;
            let (tx, rx) = channel();

            spawn(move || {
                let result = match entry {
                    ImageEntry::File(path) => {
                        if let Some(ext) = path.extension() {
                            if ext.eq_ignore_ascii_case("gif") {
                                load_gif_preview_from_path(path)
                                    .map(|g| LoadedImage::Animated(g, true))
                                    .ok_or_else(|| "Failed to load GIF preview".to_string())
                            } else {
                                match crate::loader::load_full_resolution(path) {
                                    Ok(img) => Ok(LoadedImage::Static(img)),
                                    Err(err) => Err(err),
                                }
                            }
                        } else {
                            match crate::loader::load_full_resolution(path) {
                                Ok(img) => Ok(LoadedImage::Static(img)),
                                Err(err) => Err(err),
                            }
                        }
                    }
                    ImageEntry::Zip(zip) => {
                        if zip.name.to_lowercase().ends_with(".gif") {
                            load_gif_preview_from_zip(zip)
                                .map(|g| LoadedImage::Animated(g, true))
                                .ok_or_else(|| "Failed to load GIF preview from ZIP".to_string())
                        } else {
                            match crate::loader::load_zip_image(zip) {
                                Ok(img) => Ok(LoadedImage::Static(img)),
                                Err(err) => Err(err),
                            }
                        }
                    }
                    ImageEntry::S7z(s7z) => {
                        if s7z.name.to_lowercase().ends_with(".gif") {
                            load_gif_preview_from_7z(s7z)
                                .map(|g| LoadedImage::Animated(g, true))
                                .ok_or_else(|| "Failed to load GIF preview from 7z".to_string())
                        } else {
                            match crate::loader::load_7z_image(s7z) {
                                Ok(img) => Ok(LoadedImage::Static(img)),
                                Err(err) => Err(err),
                            }
                        }
                    }
                    ImageEntry::Rar(rar) => {
                        if rar.name.to_lowercase().ends_with(".gif") {
                            load_gif_preview_from_rar(rar)
                                .map(|g| LoadedImage::Animated(g, true))
                                .ok_or_else(|| "Failed to load GIF preview from RAR".to_string())
                        } else {
                            match crate::loader::load_rar_image(rar) {
                                Ok(img) => Ok(LoadedImage::Static(img)),
                                Err(err) => Err(err),
                            }
                        }
                    }
                };

                // Send back (index, result, generation)
                let _ = tx.send((idx, result, generation));
            });

            self.preload_tasks.push(PreloadTask {
                receiver: rx,
                index: idx,
                start_time: Instant::now(),
            });
        }
    }

    pub fn process_preload_tasks(&mut self, ctx: &egui::Context) {
        // Even if caching is stopped, we still need to process results
        // to decrement worker counts and clean up.
        let mut completed = Vec::new();
        let mut tasks_to_remove = Vec::new();

        for (task_idx, task) in self.preload_tasks.iter_mut().enumerate() {
            if let Ok((idx, result, generation)) = task.receiver.try_recv() {
                // Worker finished – account for it.
                self.preload_workers = self.preload_workers.saturating_sub(1);
                self.preloading_indices.remove(&idx);

                completed.push((idx, generation, result));
                tasks_to_remove.push(task_idx);
            }
        }

        // Remove completed tasks (reverse order to avoid index shifts).
        for task_idx in tasks_to_remove.into_iter().rev() {
            self.preload_tasks.remove(task_idx);
        }

        // Check for timed-out tasks (stuck workers).
        let mut timed_out = Vec::new();
        for (task_idx, task) in self.preload_tasks.iter().enumerate() {
            if task.start_time.elapsed() > PRELOAD_TIMEOUT {
                // Task is stuck – force cleanup.
                timed_out.push(task_idx);
            }
        }
        for task_idx in timed_out.into_iter().rev() {
            let task = self.preload_tasks.remove(task_idx);
            self.preload_workers = self.preload_workers.saturating_sub(1);
            self.preloading_indices.remove(&task.index);
        }

        // If caching is stopped, don't add anything and just return.
        if self.should_stop_caching {
            if !self.preload_tasks.is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            return;
        }

        // Only cache results that are still wanted.
        let desired_set = self.desired_set_from_origin();

        for (idx, generation, result) in completed {
            // Stale generation – discard.
            if generation != self.preload_generation {
                continue;
            }

            // No longer in desired window – discard.
            if !desired_set.contains(&idx) {
                continue;
            }

            if let Ok(loaded_image) = result {
                self.add_to_cache(ctx, idx, loaded_image);
            }
        }

        // Re-enforce the cache invariant after insertion.
        self.clean_cache_outside_set(&desired_set);

        // If tasks remain, ask for another repaint soon.
        if !self.preload_tasks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub fn reset_navigation_timer(&mut self) {
        self.navigation_timer = Some(Instant::now());
    }

    pub fn update_cache_radius(&mut self, new_radius: usize) {
        let radius = new_radius.max(1).min(100);
        if radius != self.cache_radius {
            self.cache_radius = radius;
            self.delta_threshold =
                ((radius as f32 * self.cache_delta_factor).round() as usize).max(1);

            // Maximum cache size = origin window (+1 extra for current image if outside).
            let desired_capacity = radius * 2 + 2;
            self.max_cache_size = desired_capacity.max(3);

            if let Some(non_zero) = NonZeroUsize::new(self.max_cache_size) {
                let mut new_cache = lru::LruCache::new(non_zero);
                let entries: Vec<(String, CachedImage)> = self
                    .image_cache
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (key, value) in entries {
                    new_cache.put(key, value);
                }
                self.image_cache = new_cache;
            }

            // Move origin to current and invalidate old results.
            self.move_preload_origin();

            // Enforce the new window immediately.
            let desired_set = self.desired_set_from_origin();
            self.clean_cache_outside_set(&desired_set);
            self.cache_current_image();
            self.preload_adjacent_images();
        }
    }

    pub fn get_cache_range(&self) -> usize {
        self.cache_radius
    }

    pub fn stop_caching(&mut self) {
        // Prevent new tasks from starting.
        self.should_stop_caching = true;
        // Invalidate any results that may still arrive.
        self.preload_generation = self.preload_generation.wrapping_add(1);
        // Clear the cache.
        self.image_cache.clear();
        // Note: preload_tasks and preloading_indices are left intact.
        // The workers will finish, their results will be discarded,
        // and the worker counts will be correctly decremented.
        // This ensures we don't lose track of running workers.
    }
}