// preloading governs the range and tasks and duration for preloading

use super::types::{CachedImage, LoadedImage, PreloadTask, ViewerApp};
use crate::decoder::default_registry;
use crate::decoder::format_detection::load_bytes_with_detection;
use crate::gif::detection::is_gif_entry;
use crate::gif::loader::{
    load_gif_preview_from_7z, load_gif_preview_from_rar, load_gif_preview_from_zip,
};
use crate::image_core::DecodeOptions;
use crate::image_entry::ImageEntry;

use eframe::egui;
use rayon::spawn;

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const PRELOAD_TIMEOUT: Duration = Duration::from_secs(5);
const PRELOAD_REPAINT_INTERVAL: Duration = Duration::from_millis(16);

impl ViewerApp {
    // ====== INDEX / WINDOW HELPERS ======

    /// Get an index at a circular offset from `origin`.
    fn get_circular_index(&self, origin: usize, offset: isize) -> usize {
        let len = self.image_entries.len();

        if len == 0 {
            return 0;
        }

        ((origin as isize + offset).rem_euclid(len as isize)) as usize
    }

    /// Returns the indices that should currently be considered for preload.
    ///
    /// Order:
    /// 1. Current image
    /// 2. Preload origin
    /// 3. +1, -1, +2, -2, ...
    fn ordered_desired_window(&self) -> Vec<usize> {
        let mut desired = Vec::new();
        let len = self.image_entries.len();

        if len == 0 {
            return desired;
        }

        let mut added = HashSet::new();

        self.push_desired_index(&mut desired, &mut added, self.current_index);

        self.push_desired_index(&mut desired, &mut added, self.preload_origin);

        for offset in 1..=self.cache_radius as isize {
            let forward = self.get_circular_index(self.preload_origin, offset);
            self.push_desired_index(&mut desired, &mut added, forward);

            let backward = self.get_circular_index(self.preload_origin, -offset);
            self.push_desired_index(&mut desired, &mut added, backward);
        }

        desired
    }

    /// Add an index to an ordered list only once.
    fn push_desired_index(
        &self,
        desired: &mut Vec<usize>,
        added: &mut HashSet<usize>,
        index: usize,
    ) {
        if added.insert(index) {
            desired.push(index);
        }
    }

    /// Build the set of indices that should remain in the preload/cache
    /// window.
    ///
    /// The current image is always included.
    fn desired_set_from_origin(&self) -> HashSet<usize> {
        let mut desired = HashSet::new();
        let len = self.image_entries.len();

        if len == 0 {
            return desired;
        }

        for offset in 0..=self.cache_radius as isize {
            desired.insert(self.get_circular_index(self.preload_origin, offset));

            desired.insert(self.get_circular_index(self.preload_origin, -offset));
        }

        desired.insert(self.current_index);

        desired
    }

    /// Returns true if an index points to a GIF.
    fn is_gif_index(&self, idx: usize) -> bool {
        self.image_entries.get(idx).is_some_and(is_gif_entry)
    }

    /// Returns true if an entry has already been intentionally skipped.
    fn is_preload_skipped(&self, idx: usize) -> bool {
        self.preload_skipped.contains(&idx)
    }

    /// Returns true if an index is currently being processed.
    fn is_preloading(&self, idx: usize) -> bool {
        self.preloading_indices.contains(&idx)
    }

    /// Returns true if the index has been successfully cached.
    fn is_preload_cached(&self, idx: usize) -> bool {
        self.is_index_cached(idx)
    }

    /// Returns true if this entry requires no normal cache work.
    ///
    /// GIFs and explicitly skipped entries are both considered handled.
    fn is_preload_handled(&self, idx: usize) -> bool {
        self.is_gif_index(idx) || self.is_preload_skipped(idx) || self.is_preload_cached(idx)
    }

    /// Check if the current image is far enough from the preload origin
    /// that the origin should move.
    fn should_move_origin(&self) -> bool {
        let len = self.image_entries.len();

        if len <= 1 {
            return false;
        }

        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();

        let distance = diff.min(len - diff);

        distance >= self.delta_threshold
    }

    /// Move the preload origin to the current image.
    ///
    /// Moving the origin starts a new preload cycle, allowing previously
    /// skipped entries to become eligible again.
    fn move_preload_origin(&mut self) {
        self.preload_origin = self.current_index;
        self.navigation_timer = None;

        // New preload cycle.
        self.preload_skipped.clear();

        // Invalidate results from the previous cycle.
        self.preload_generation = self.preload_generation.wrapping_add(1);
    }

    /// Update the preload origin when navigation moved far enough.
    fn update_preload_origin(&mut self) {
        let len = self.image_entries.len();

        if len <= 1 {
            return;
        }

        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();

        let distance = diff.min(len - diff);

        if distance >= self.cache_radius * 2 || self.should_move_origin() {
            self.move_preload_origin();
        }
    }

    // ====== PRELOAD PROGRESS ======

    /// Number of desired entries that were intentionally skipped.
    ///
    /// This includes:
    /// - VirtualTexture entries
    /// - timed-out preload entries
    /// - any future preload type that explicitly uses `preload_skipped`
    pub fn preload_skipped_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|&&idx| self.is_preload_skipped(idx))
            .count()
    }

    /// Number of desired entries successfully placed in the normal cache.
    ///
    /// GIFs and skipped entries are excluded.
    pub fn preload_cached_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|&&idx| {
                !self.is_preload_skipped(idx)
                    && !self.is_gif_index(idx)
                    && self.is_preload_cached(idx)
            })
            .count()
    }

    /// Number of desired entries that are considered completed.
    ///
    /// Completed means:
    /// - cached
    /// - skipped
    /// - GIF
    fn preload_completed_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|&&idx| self.is_preload_handled(idx))
            .count()
    }

    /// Print preload progress.
    fn report_preload_progress(&self) {
        let desired = self.desired_set_from_origin();

        let total = desired.len();
        let completed = self.preload_completed_count();
        let skipped = self.preload_skipped_count();
        if completed < total {
            println!("Preload: {}/{} ({} skipped)", completed, total, skipped);
        } else {
            return;
        }
    }

    /// Returns true when every desired entry has been handled.
    fn is_cache_filled(&self) -> bool {
        let desired = self.desired_set_from_origin();

        desired.iter().all(|&idx| self.is_preload_handled(idx))
    }

    /// Returns true if there are active preload tasks.
    fn has_preload_tasks(&self) -> bool {
        !self.preload_tasks.is_empty()
    }

    /// Update the overall preload working state.
    fn update_preload_working(&mut self) {
        self.preload_working = !self.is_cache_filled() || self.has_preload_tasks();

        self.report_preload_progress();
    }

    // ====== PRELOAD ENTRY LOADING ======

    /// Load one entry for a preload worker.
    ///
    /// `VirtualPending` is returned to the UI thread and is deliberately
    /// not inserted into the normal image cache.
    fn load_preload_entry(
        entry: ImageEntry,
        virtual_texture_threshold: u32,
    ) -> Result<LoadedImage, String> {
        match entry {
            ImageEntry::File(path) => {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("Failed to read image: {e}"))?;
                load_bytes_with_detection(
                    bytes,
                    Some(&path),
                    virtual_texture_threshold,
                    &DecodeOptions::default(),
                    Some(&default_registry()),
                )
            }

            ImageEntry::Zip(zip) => {
                if zip.name.to_lowercase().ends_with(".gif") {
                    return load_gif_preview_from_zip(zip)
                        .map(|gif| LoadedImage::Animated(gif, true))
                        .ok_or_else(|| "Failed to load GIF preview from ZIP".to_string());
                }

                crate::loader::load_zip_image(zip)
                    .map(LoadedImage::Static)
                    .map_err(|err| err.to_string())
            }

            ImageEntry::S7z(s7z) => {
                if s7z.name.to_lowercase().ends_with(".gif") {
                    return load_gif_preview_from_7z(s7z)
                        .map(|gif| LoadedImage::Animated(gif, true))
                        .ok_or_else(|| "Failed to load GIF preview from 7z".to_string());
                }

                crate::loader::load_7z_image(s7z)
                    .map(LoadedImage::Static)
                    .map_err(|err| err.to_string())
            }

            ImageEntry::Rar(rar) => {
                if rar.name.to_lowercase().ends_with(".gif") {
                    return load_gif_preview_from_rar(rar)
                        .map(|gif| LoadedImage::Animated(gif, true))
                        .ok_or_else(|| "Failed to load GIF preview from RAR".to_string());
                }

                crate::loader::load_rar_image(rar)
                    .map(LoadedImage::Static)
                    .map_err(|err| err.to_string())
            }
        }
    }

    //  ====== PRELOAD START ======

    /// Start one asynchronous preload worker.
    fn start_preload_task(&mut self, idx: usize) {
        if self.should_stop_caching {
            return;
        }

        let Some(entry) = self.image_entries.get(idx).cloned() else {
            return;
        };

        self.preload_working = true;

        self.preloading_indices.insert(idx);
        self.preload_workers += 1;

        let generation = self.preload_generation;

        // Capture settings on the UI thread.
        let virtual_texture_threshold = self.settings_manager.get().virtual_texture_threshold;

        let (tx, rx) = channel();

        spawn(move || {
            let result = Self::load_preload_entry(entry, virtual_texture_threshold);

            let _ = tx.send((idx, result, generation));
        });

        self.preload_tasks.push(PreloadTask {
            receiver: rx,
            index: idx,
            start_time: Instant::now(),
        });
    }

    /// Determine how many preload workers may run simultaneously.
    fn available_preload_slots(&self) -> usize {
        let max_concurrent = if self.is_loading() {
            self.max_cache_task.min(1)
        } else {
            self.max_cache_task
        } as usize;

        max_concurrent.saturating_sub(self.preload_workers)
    }

    /// Start preload workers until all available slots are filled.
    fn start_available_preloads(&mut self, desired_ordered: Vec<usize>, slots: usize) -> usize {
        let mut started = 0;

        for idx in desired_ordered {
            if started >= slots {
                break;
            }

            if self.is_gif_index(idx) {
                continue;
            }

            if self.is_preload_cached(idx) {
                continue;
            }

            if self.is_preloading(idx) {
                continue;
            }

            if self.is_preload_skipped(idx) {
                continue;
            }

            self.start_preload_task(idx);
            started += 1;
        }

        started
    }

    // ====== MAIN PRELOAD PIPELINE ======

    pub fn preload_adjacent_images(&mut self) {
        if self.should_stop_caching {
            return;
        }

        if self.image_entries.len() <= 1 {
            self.preload_working = false;
            return;
        }

        // Update origin based on navigation.
        self.update_preload_origin();

        // Build current desired window.
        let desired_set = self.desired_set_from_origin();

        // Remove cache entries that are no longer wanted.
        self.clean_cache_outside_set(&desired_set);

        // Respect preload throttle.
        if let Some(last) = self.last_preload_start {
            let throttle_ms = self.settings_manager.get().preload_throttle_ms;

            if last.elapsed() < Duration::from_millis(throttle_ms) {
                self.update_preload_working();
                return;
            }
        }

        // Determine available worker slots.
        let slots = self.available_preload_slots();

        if slots == 0 {
            self.update_preload_working();
            return;
        }

        // Start highest-priority desired entries first.
        let desired_ordered = self.ordered_desired_window();

        let started = self.start_available_preloads(desired_ordered, slots);

        if started > 0 {
            self.last_preload_start = Some(Instant::now());
        }

        self.update_preload_working();
    }

    // ====== CACHE MANAGEMENT ======

    /// Remove cached images outside the desired set.
    fn clean_cache_outside_set(&mut self, desired: &HashSet<usize>) {
        if self.should_stop_caching {
            return;
        }

        let to_remove: Vec<String> = self
            .image_cache
            .iter()
            .filter_map(|(id, cached)| {
                if desired.contains(&cached.index) {
                    None
                } else {
                    Some(id.clone())
                }
            })
            .collect();

        for id in to_remove {
            self.image_cache.pop(&id);
        }
    }

    // ====== PRELOAD RESULT PROCESSING ======

    /// Collect workers that have finished normally.
    fn collect_completed_preload_tasks(
        &mut self,
    ) -> Vec<(usize, Result<LoadedImage, String>, u64)> {
        let mut completed = Vec::new();
        let mut remove_indices = Vec::new();

        for (task_idx, task) in self.preload_tasks.iter_mut().enumerate() {
            if let Ok((idx, result, generation)) = task.receiver.try_recv() {
                self.preload_workers = self.preload_workers.saturating_sub(1);

                self.preloading_indices.remove(&idx);

                completed.push((idx, result, generation));
                remove_indices.push(task_idx);
            }
        }

        for task_idx in remove_indices.into_iter().rev() {
            self.preload_tasks.remove(task_idx);
        }

        completed
    }

    /// Remove and mark preload tasks that exceeded the timeout.
    fn process_preload_timeouts(&mut self) {
        let timed_out: Vec<usize> = self
            .preload_tasks
            .iter()
            .enumerate()
            .filter_map(|(task_idx, task)| {
                (task.start_time.elapsed() >= PRELOAD_TIMEOUT).then_some(task_idx)
            })
            .collect();

        for task_idx in timed_out.into_iter().rev() {
            let task = self.preload_tasks.remove(task_idx);

            self.preload_workers = self.preload_workers.saturating_sub(1);

            self.preloading_indices.remove(&task.index);

            self.preload_skipped.insert(task.index);

            println!(
                "Preload: index {} timed out after {:?} — skipped",
                task.index, PRELOAD_TIMEOUT
            );
        }
    }

    /// Process one completed preload result.
    fn process_preload_result(
        &mut self,
        ctx: &egui::Context,
        idx: usize,
        result: Result<LoadedImage, String>,
    ) {
        match result {
            // Virtual texture
            Ok(LoadedImage::VirtualPending(_, width, height)) => {
                // VT images do not enter the normal cache.
                self.preload_skipped.insert(idx);

                println!(
                    "Preload: index {} skipped for virtual \
                     texturing ({}x{})",
                    idx, width, height
                );
            }

            // Normal image / GIF
            Ok(loaded_image) => {
                self.preload_skipped.remove(&idx);

                self.add_to_cache(ctx, idx, loaded_image);
            }

            // Failed preload
            Err(err) => {
                eprintln!("Preload: failed to load index {}: {}", idx, err);

                // Failed entries are intentionally not marked as skipped.
                // They can be retried on a later preload pass.
            }
        }
    }

    /// Process all finished preload workers and enforce the cache invariant.
    pub fn process_preload_tasks(&mut self, ctx: &egui::Context) {
        // Collect workers that completed normally.
        let completed = self.collect_completed_preload_tasks();

        // Mark workers that exceeded the timeout as skipped.
        self.process_preload_timeouts();

        // If caching was stopped, there is nothing else to process.
        if self.should_stop_caching {
            if !self.preload_tasks.is_empty() {
                ctx.request_repaint_after(PRELOAD_REPAINT_INTERVAL);
            }

            return;
        }

        let desired_set = self.desired_set_from_origin();

        // Process results belonging to the current preload generation.
        for (idx, result, generation) in completed {
            // Ignore stale generations.
            if generation != self.preload_generation {
                continue;
            }

            // Ignore entries that are no longer wanted.
            if !desired_set.contains(&idx) {
                continue;
            }

            self.process_preload_result(ctx, idx, result);
        }

        // Keep the cache inside the current desired window.
        self.clean_cache_outside_set(&desired_set);

        // Update progress/state.
        self.update_preload_working();

        // Continue processing while work remains.
        if self.preload_working {
            ctx.request_repaint_after(PRELOAD_REPAINT_INTERVAL);
        }
    }

    // ====== NAVIGATION / SETTINGS ======

    pub fn reset_navigation_timer(&mut self) {
        self.navigation_timer = Some(Instant::now());
    }

    pub fn update_cache_radius(&mut self, new_radius: usize) {
        let radius = new_radius.clamp(1, 100);

        if radius == self.cache_radius {
            return;
        }

        self.cache_radius = radius;

        self.delta_threshold = ((radius as f32 * self.cache_delta_factor).round() as usize).max(1);

        let desired_capacity = radius * 2 + 2;
        self.max_cache_size = desired_capacity.max(3);

        if let Some(non_zero) = NonZeroUsize::new(self.max_cache_size) {
            let mut new_cache = lru::LruCache::new(non_zero);

            let entries: Vec<(String, CachedImage)> = self
                .image_cache
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();

            for (key, value) in entries {
                new_cache.put(key, value);
            }

            self.image_cache = new_cache;
        }

        // Changing the radius starts a new preload cycle.
        self.move_preload_origin();

        let desired_set = self.desired_set_from_origin();

        self.clean_cache_outside_set(&desired_set);

        self.cache_current_image();
        self.preload_adjacent_images();
    }

    pub fn get_cache_range(&self) -> usize {
        self.cache_radius
    }

    // ====== STOP CACHING ======

    pub fn stop_caching(&mut self) {
        self.should_stop_caching = true;

        // Invalidate all currently running workers.
        self.preload_generation = self.preload_generation.wrapping_add(1);

        // Clear normal cache.
        self.image_cache.clear();

        // Rayon workers cannot be forcibly cancelled.
        //
        // Their generation is now stale, so their results will be ignored.
        self.preload_working = false;
    }
}
