// preloading governs the range and tasks and duration for preloading

use super::types::{CachedImage, LoadedImage, PreloadTask, ViewerApp};
use crate::app::constants::MAX_TILE_SIZE;
use crate::gif::detection::is_gif_entry;
use crate::gif::loader::{
    load_gif_preview_from_7z, load_gif_preview_from_rar, load_gif_preview_from_zip,
};
use crate::image_entry::ImageEntry;

use eframe::egui;
use rayon::spawn;

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const PRELOAD_TIMEOUT: Duration = Duration::from_secs(5);

impl ViewerApp {
    // =====================================================================
    // INDEX / WINDOW HELPERS
    // =====================================================================

    /// Get the index at a circular offset from origin.
    fn get_circular_index(&self, origin: usize, offset: isize) -> usize {
        let len = self.image_entries.len();

        if len == 0 {
            return 0;
        }

        ((origin as isize + offset).rem_euclid(len as isize)) as usize
    }

    /// Build an ordered list of desired indices around the origin.
    ///
    /// Priority:
    /// 1. Current image
    /// 2. Preload origin
    /// 3. Alternating +1, -1, +2, -2, ...
    fn ordered_desired_window(&self) -> Vec<usize> {
        let mut desired = Vec::new();
        let len = self.image_entries.len();

        if len == 0 {
            return desired;
        }

        desired.push(self.current_index);

        let mut added = HashSet::new();
        added.insert(self.current_index);

        if !added.contains(&self.preload_origin) {
            desired.push(self.preload_origin);
            added.insert(self.preload_origin);
        }

        for offset in 1..=self.cache_radius as isize {
            let forward = self.get_circular_index(self.preload_origin, offset);

            if added.insert(forward) {
                desired.push(forward);
            }

            let backward = self.get_circular_index(self.preload_origin, -offset);

            if added.insert(backward) {
                desired.push(backward);
            }
        }

        desired
    }

    /// Build the set of indices that should be kept in the cache/preload
    /// window.
    ///
    /// The current image is always included even if it lies outside the
    /// normal origin radius.
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

    /// Check if the current image is far enough from the preload origin
    /// that the origin should move.
    fn should_move_origin(&self) -> bool {
        let len = self.image_entries.len();

        if len <= 1 {
            return false;
        }

        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();

        let dist = diff.min(len - diff);

        dist >= self.delta_threshold
    }

    /// Move the preload origin to the current image.
    ///
    /// This starts a new preload cycle, so previously skipped entries become
    /// eligible again.
    fn move_preload_origin(&mut self) {
        self.preload_origin = self.current_index;
        self.navigation_timer = None;

        // New preload cycle.
        self.preload_skipped.clear();

        // Invalidate results belonging to the previous cycle.
        self.preload_generation = self.preload_generation.wrapping_add(1);
    }

    // =====================================================================
    // PRELOAD PROGRESS
    // =====================================================================

    /// Number of desired entries that were intentionally skipped.
    ///
    /// This currently includes:
    ///
    /// - VirtualTexture entries
    /// - any other entry that the preload system decides should not enter
    ///   the normal cache
    pub fn preload_skipped_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|idx| self.preload_skipped.contains(idx))
            .count()
    }

    /// Number of desired entries successfully placed in the normal cache.
    ///
    /// VirtualTexture entries and GIFs are intentionally excluded.
    pub fn preload_cached_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|idx| {
                if self.preload_skipped.contains(idx) {
                    return false;
                }

                if let Some(entry) = self.image_entries.get(**idx) {
                    if is_gif_entry(entry) {
                        return false;
                    }
                }

                self.is_index_cached(**idx)
            })
            .count()
    }

    /// Number of desired entries considered completed.
    ///
    /// An entry is completed when:
    ///
    /// - it is cached
    /// - it was skipped
    /// - it is a GIF
    fn preload_completed_count(&self) -> usize {
        let desired = self.desired_set_from_origin();

        desired
            .iter()
            .filter(|idx| {
                if self.preload_skipped.contains(idx) {
                    return true;
                }

                if let Some(entry) = self.image_entries.get(**idx) {
                    if is_gif_entry(entry) {
                        return true;
                    }
                }

                self.is_index_cached(**idx)
            })
            .count()
    }

    /// Print preload progress.
    ///
    /// Example:
    ///
    ///     Preload: 15/15 (1 skipped)
    fn report_preload_progress(&self) {
        let desired = self.desired_set_from_origin();

        let total = desired.len();
        let completed = self.preload_completed_count();
        let skipped = self.preload_skipped_count();

        println!("Preload: {}/{} ({} skipped)", completed, total, skipped);
    }

    /// Returns true when every desired entry has been handled.
    ///
    /// GIFs and skipped entries count as completed without occupying the
    /// normal image cache.
    fn is_cache_filled(&self) -> bool {
        let desired = self.desired_set_from_origin();

        desired.iter().all(|idx| {
            // GIFs do not need normal preloading.
            if let Some(entry) = self.image_entries.get(*idx) {
                if is_gif_entry(entry) {
                    return true;
                }
            }

            // VT / skipped entries count as completed.
            if self.preload_skipped.contains(idx) {
                return true;
            }

            // Normal cached image.
            self.is_index_cached(*idx)
        })
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

    // =====================================================================
    // PRELOAD ENTRY LOADING
    // =====================================================================

    /// Load one entry for the preload worker.
    ///
    /// The important part here is that your centralized
    /// `load_bytes_with_detection()` should be used wherever possible.
    ///
    /// `LoadedImage::VirtualPending` is deliberately returned to the main
    /// thread rather than being inserted into the normal cache.
    fn load_preload_entry(
        entry: ImageEntry,
        virtual_texture_threshold: u32,
    ) -> Result<LoadedImage, String> {
        match entry {
            ImageEntry::File(path) => {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("Failed to read image: {e}"))?;

                crate::app::loading::load_bytes_with_detection(
                    bytes,
                    Some(&path),
                    virtual_texture_threshold,
                    MAX_TILE_SIZE,
                )
            }

            ImageEntry::Zip(zip) => {
                // GIFs are handled through the existing GIF preview loader.
                if zip.name.to_lowercase().ends_with(".gif") {
                    return load_gif_preview_from_zip(zip)
                        .map(|gif| LoadedImage::Animated(gif, true))
                        .ok_or_else(|| "Failed to load GIF preview from ZIP".to_string());
                }

                // Keep your existing archive loader here.
                //
                // If you later expose raw ZIP bytes, route them through
                // load_bytes_with_detection() exactly like File above.
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

    // =====================================================================
    // PRELOAD START
    // =====================================================================

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

        // Capture the setting now.
        //
        // The worker must not access self.settings_manager because self is
        // owned by the UI thread.
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

    // =====================================================================
    // MAIN PRELOAD PIPELINE
    // =====================================================================

    pub fn preload_adjacent_images(&mut self) {
        if self.should_stop_caching {
            return;
        }

        if self.image_entries.len() <= 1 {
            self.preload_working = false;
            return;
        }

        // ---------------------------------------------------------------
        // Step 1: Update preload origin.
        // ---------------------------------------------------------------

        let len = self.image_entries.len();

        let diff = (self.current_index as isize - self.preload_origin as isize).unsigned_abs();

        let dist = diff.min(len - diff);

        if dist >= self.cache_radius * 2 {
            self.move_preload_origin();
        } else if self.should_move_origin() {
            self.move_preload_origin();
        }

        // ---------------------------------------------------------------
        // Step 2: Build desired window.
        // ---------------------------------------------------------------

        let desired_set = self.desired_set_from_origin();

        // ---------------------------------------------------------------
        // Step 3: Remove cache entries outside the desired window.
        // ---------------------------------------------------------------

        self.clean_cache_outside_set(&desired_set);

        // ---------------------------------------------------------------
        // Step 4: Respect preload throttle.
        // ---------------------------------------------------------------

        if let Some(last) = self.last_preload_start {
            let throttle_ms = self.settings_manager.get().preload_throttle_ms;

            if last.elapsed() < Duration::from_millis(throttle_ms) {
                self.update_preload_working();
                return;
            }
        }

        // ---------------------------------------------------------------
        // Step 5: Determine maximum workers.
        // ---------------------------------------------------------------

        let max_concurrent = if self.is_loading() {
            self.max_cache_task.min(1)
        } else {
            self.max_cache_task
        } as usize;

        let active_workers = self.preload_workers;

        if active_workers >= max_concurrent {
            self.update_preload_working();
            return;
        }

        let slots = max_concurrent - active_workers;

        // ---------------------------------------------------------------
        // Step 6: Start preload tasks.
        // ---------------------------------------------------------------

        let desired_ordered = self.ordered_desired_window();

        let mut started = 0;

        for idx in desired_ordered {
            if started >= slots {
                break;
            }

            // GIFs don't need normal cache preloading.
            if let Some(entry) = self.image_entries.get(idx) {
                if is_gif_entry(entry) {
                    continue;
                }
            }

            // Already cached.
            if self.is_index_cached(idx) {
                continue;
            }

            // Already being processed.
            if self.preloading_indices.contains(&idx) {
                continue;
            }

            // Already handled as VT / skipped.
            if self.preload_skipped.contains(&idx) {
                continue;
            }

            self.start_preload_task(idx);
            started += 1;
        }

        if started > 0 {
            self.last_preload_start = Some(Instant::now());
        }

        self.update_preload_working();
    }

    // =====================================================================
    // CACHE MANAGEMENT
    // =====================================================================

    /// Remove cached images outside the desired set.
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

        for id in to_remove {
            self.image_cache.pop(&id);
        }
    }

    // =====================================================================
    // PRELOAD RESULT PROCESSING
    // =====================================================================

    pub fn process_preload_tasks(&mut self, ctx: &egui::Context) {
        let mut completed = Vec::new();
        let mut tasks_to_remove = Vec::new();

        // ---------------------------------------------------------------
        // Step 1: Process workers that have already completed.
        // ---------------------------------------------------------------

        for (task_idx, task) in self.preload_tasks.iter_mut().enumerate() {
            if let Ok((idx, result, generation)) = task.receiver.try_recv() {
                self.preload_workers = self.preload_workers.saturating_sub(1);

                self.preloading_indices.remove(&idx);

                completed.push((idx, result, generation));
                tasks_to_remove.push(task_idx);
            }
        }

        for task_idx in tasks_to_remove.into_iter().rev() {
            self.preload_tasks.remove(task_idx);
        }

        // ---------------------------------------------------------------
        // Step 2: Handle timeout bookkeeping.
        //
        // NOTE:
        // Removing the task here does NOT cancel the Rayon worker.
        // The worker can still finish later, but its receiver has been
        // dropped and its result will be ignored.
        // ---------------------------------------------------------------

        let mut timed_out = Vec::new();

        for (task_idx, task) in self.preload_tasks.iter().enumerate() {
            if task.start_time.elapsed() >= PRELOAD_TIMEOUT {
                timed_out.push(task_idx);
            }
        }

        for task_idx in timed_out.into_iter().rev() {
            let task = self.preload_tasks.remove(task_idx);

            self.preload_workers = self.preload_workers.saturating_sub(1);

            self.preloading_indices.remove(&task.index);

            // Timeout is another reason an item is skipped.
            self.preload_skipped.insert(task.index);

            println!(
                "Preload: index {} timed out after {:?} — skipped",
                task.index, PRELOAD_TIMEOUT
            );
        }

        // ---------------------------------------------------------------
        // Step 3: If caching has been stopped, cleanly finish.
        // ---------------------------------------------------------------

        if self.should_stop_caching {
            if !self.preload_tasks.is_empty() {
                ctx.request_repaint_after(Duration::from_millis(16));
            }

            return;
        }

        // ---------------------------------------------------------------
        // Step 4: Determine what is still wanted.
        // ---------------------------------------------------------------

        let desired_set = self.desired_set_from_origin();

        // ---------------------------------------------------------------
        // Step 5: Process completed results.
        // ---------------------------------------------------------------

        for (idx, result, generation) in completed {
            // Result belongs to an old preload generation.
            if generation != self.preload_generation {
                continue;
            }

            // Result is no longer in the desired window.
            if !desired_set.contains(&idx) {
                continue;
            }

            match result {
                // -------------------------------------------------------
                // Virtual texture
                // -------------------------------------------------------
                Ok(LoadedImage::VirtualPending(_, width, height)) => {
                    //
                    // This image has already been classified by
                    // load_bytes_with_detection().
                    //
                    // It must NOT enter image_cache.
                    //
                    // Mark it as completed/skipped instead.
                    //
                    self.preload_skipped.insert(idx);

                    println!(
                        "Preload: index {} skipped for virtual \
                         texturing ({}x{})",
                        idx, width, height
                    );
                }

                // -------------------------------------------------------
                // Normal image / GIF
                // -------------------------------------------------------
                Ok(loaded_image) => {
                    // A successful normal preload means it should no
                    // longer be considered skipped.
                    self.preload_skipped.remove(&idx);

                    self.add_to_cache(ctx, idx, loaded_image);
                }

                // -------------------------------------------------------
                // Failed preload
                // -------------------------------------------------------
                Err(err) => {
                    eprintln!("Preload: failed to load index {}: {}", idx, err);

                    // Do not mark a normal load error as completed.
                    //
                    // It can be retried on the next preload pass.
                }
            }
        }

        // ---------------------------------------------------------------
        // Step 6: Enforce cache invariant.
        // ---------------------------------------------------------------

        self.clean_cache_outside_set(&desired_set);

        // ---------------------------------------------------------------
        // Step 7: Update progress.
        // ---------------------------------------------------------------

        self.update_preload_working();

        // ---------------------------------------------------------------
        // Step 8: Keep processing while actual preload work remains.
        // ---------------------------------------------------------------

        if self.preload_working {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    // =====================================================================
    // NAVIGATION / SETTINGS
    // =====================================================================

    pub fn reset_navigation_timer(&mut self) {
        self.navigation_timer = Some(Instant::now());
    }

    pub fn update_cache_radius(&mut self, new_radius: usize) {
        let radius = new_radius.clamp(1, 100);

        if radius != self.cache_radius {
            self.cache_radius = radius;

            self.delta_threshold =
                ((radius as f32 * self.cache_delta_factor).round() as usize).max(1);

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
    }

    pub fn get_cache_range(&self) -> usize {
        self.cache_radius
    }

    // =====================================================================
    // STOP CACHING
    // =====================================================================

    pub fn stop_caching(&mut self) {
        self.should_stop_caching = true;

        // Invalidate all results belonging to currently running workers.
        self.preload_generation = self.preload_generation.wrapping_add(1);

        // Clear normal cache.
        self.image_cache.clear();

        // Existing Rayon workers cannot be forcibly cancelled.
        //
        // Their generation is now stale, so any result they eventually
        // produce will be discarded.
        self.preload_working = false;
    }
}
