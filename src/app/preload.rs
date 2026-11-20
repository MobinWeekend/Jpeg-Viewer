// preloading governs the range and tasks and duration for preloading

use super::texture_mipmap::{
    SharedMipmapState, is_mipmap_state_pending, start_mipmap_upload,
    start_mipmap_upload_from_entry,
};
use super::loading::load_entry_content_with_metadata_cancellable;
use super::types::{CachedImage, LoadedEntryData, LoadedImage, PreloadTask, ViewerApp};
use crate::decoder::gif::detection::is_gif_entry;
use crate::imaging::entry::ImageEntry;

use eframe::egui;
use rayon::spawn;

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::mpsc::channel;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
        let Some(entry) = self.image_entries.get(idx) else {
            return false;
        };
        let key = entry.get_id();
        if let Ok(cache) = self.gif_detection_cache.lock() {
            if let Some(is_gif) = cache.get(&key) {
                return *is_gif;
            }
        }
        let is_gif = is_gif_entry(entry);
        if let Ok(mut cache) = self.gif_detection_cache.lock() {
            cache.insert(key, is_gif);
        }
        is_gif
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
        self.cancel_all_preload_tasks();
        self.preload_origin = self.current_index;

        // New preload cycle. Mipmap lookahead is no longer delayed by a
        // navigation-idle timer; it starts as soon as the current image is ready.
        self.preload_skipped.clear();

        // Invalidate results from the previous cycle.
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.preloading_indices.clear();
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
        //let skipped = self.preload_skipped_count();
        if completed < total {
            //println!("Preload: {}/{} ({} skipped)", completed, total, skipped);
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
        index: usize,
        generation: u64,
        cancellation: &AtomicBool,
    ) -> Result<LoadedEntryData, String> {
        if cancellation.load(Ordering::Acquire) {
            return Err("Preload cancelled".to_owned());
        }
        // Preserve the existing behavior: standalone files may enter the
        // virtual-texture path, while archive members are preloaded as regular
        // decoded images just as they were before this shared-loading change.
        let threshold = if matches!(&entry, ImageEntry::File(_)) {
            virtual_texture_threshold
        } else {
            u32::MAX
        };
        load_entry_content_with_metadata_cancellable(
            entry,
            threshold,
            false,
            index,
            generation,
            Some(cancellation),
        )
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
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);

        spawn(move || {
            let result = Self::load_preload_entry(
                entry,
                virtual_texture_threshold,
                idx,
                generation,
                worker_cancellation.as_ref(),
            );
            if !worker_cancellation.load(Ordering::Acquire) {
                let _ = tx.send((idx, result, generation));
            }
        });

        self.preload_tasks.push(PreloadTask {
            receiver: rx,
            index: idx,
            generation,
            start_time: Instant::now(),
            cancellation,
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

    /// Whether an entry is a camera RAW image. The current RAW decoder must
    /// develop the complete sensor image, which is too expensive to do speculatively
    /// for adjacent images while another RAW image is being opened.
    fn is_raw_index(&self, idx: usize) -> bool {
        let name = match self.image_entries.get(idx) {
            Some(ImageEntry::File(path)) => path.to_string_lossy(),
            Some(ImageEntry::Zip(item)) => std::borrow::Cow::Borrowed(item.name.as_str()),
            Some(ImageEntry::S7z(item)) => std::borrow::Cow::Borrowed(item.name.as_str()),
            Some(ImageEntry::Rar(item)) => std::borrow::Cow::Borrowed(item.name.as_str()),
            None => return false,
        };

        std::path::Path::new(name.as_ref())
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(crate::imaging::core::ImageFormat::from_extension)
            == Some(crate::imaging::core::ImageFormat::CameraRaw)
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

            // RAW files are loaded on demand only. Preloading them would run
            // multiple costly full-resolution sensor developments in parallel.
            if self.is_raw_index(idx) {
                self.preload_skipped.insert(idx);
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

    /// Prepare mipmaps for the current image and then cached neighbours.
    ///
    /// When no mip job is active, the current image is checked first. As soon
    /// as its mipmaps are ready, the next cached neighbour starts immediately;
    /// there is no navigation-idle delay. Newly cached neighbours are picked up
    /// on the next app update.
    /// Only one speculative worker/upload stream runs at a time to avoid retaining
    /// full-resolution RGBA buffers for every image in the cache radius.
    pub(crate) fn prepare_next_cached_mipmap(&mut self, ctx: &egui::Context) {
        if self.settings_manager.get().texture_filter != "mipmap"
            || self.should_stop_caching
            || !self.workspace.should_preload()
            || self.is_loading()
        {
            return;
        }

        // Never start a second CPU/GPU mip job while another one is still
        // streaming. The existing callback continues to make progress, even if
        // navigation resumes before the current job is complete.
        if self.has_pending_mipmap_work() {
            return;
        }

        // First priority: the image currently on screen. This covers cache hits
        // whose mip preparation was deferred while a previous neighbour finished.
        if !self.inspect_mode && !self.is_gif && !self.is_preview {
            if let Some(image_id) = self.get_image_id(self.current_index) {
                let is_current_texture = self.texture_image_id.as_deref() == Some(image_id.as_str());
                let needs_mipmaps = self
                    .image_cache
                    .peek(&image_id)
                    .is_some_and(|cached| {
                        cached.texture_filter == "mipmap"
                            && !cached.is_gif
                            && !cached.is_preview
                            && cached.mipmap_state.is_none()
                    });

                if is_current_texture && needs_mipmaps {
                    if let Some(entry) = self.image_entries.get(self.current_index).cloned() {
                        let retained_pixels = self
                            .current_decoded_pixels
                            .as_ref()
                            .filter(|(source_id, _, _, _)| source_id == &image_id)
                            .map(|(_, width, height, pixels)| {
                                (*width, *height, Arc::clone(pixels))
                            })
                            .or_else(|| {
                                self.preloaded_mipmap_pixels
                                    .as_ref()
                                    .filter(|(source_id, _, _, _)| source_id == &image_id)
                                    .map(|(_, width, height, pixels)| {
                                        (*width, *height, Arc::clone(pixels))
                                    })
                            });
                        let state = if let Some((width, height, pixels)) = retained_pixels {
                            start_mipmap_upload(width, height, pixels)
                        } else {
                            start_mipmap_upload_from_entry(
                                entry,
                                self.settings_manager.get().virtual_texture_threshold,
                            )
                        };
                        if let Some(cached) = self.image_cache.get_mut(&image_id) {
                            cached.mipmap_state = Some(state.clone());
                            self.current_mipmap_state = Some(state);
                            ctx.request_repaint();
                            return;
                        }
                    }
                }
            }
        }

        let len = self.image_entries.len();
        if len <= 1 {
            return;
        }

        // Prefer the next image, then the previous image, then alternate outward.
        for distance in 1..=self.cache_radius {
            let forward = self.get_circular_index(self.current_index, distance as isize);
            let backward = self.get_circular_index(self.current_index, -(distance as isize));

            for index in [forward, backward] {
                if index == self.current_index || self.is_gif_index(index) {
                    continue;
                }
                let Some(image_id) = self.get_image_id(index) else {
                    continue;
                };
                let needs_mipmaps = self
                    .image_cache
                    .peek(&image_id)
                    .is_some_and(|cached| {
                        cached.texture_filter == "mipmap"
                            && !cached.is_gif
                            && !cached.is_preview
                            && cached.mipmap_state.is_none()
                    });
                if !needs_mipmaps {
                    continue;
                }

                let Some(entry) = self.image_entries.get(index).cloned() else {
                    continue;
                };
                let retained_pixels = self
                    .preloaded_mipmap_pixels
                    .as_ref()
                    .filter(|(source_id, _, _, _)| source_id == &image_id)
                    .map(|(_, width, height, pixels)| (*width, *height, Arc::clone(pixels)));
                let state = if let Some((width, height, pixels)) = retained_pixels {
                    // Keep the single speculative Arc until this image becomes
                    // current or leaves the cache window. The mip worker shares
                    // it, and Inspect can reuse it later without another read.
                    start_mipmap_upload(width, height, pixels)
                } else {
                    start_mipmap_upload_from_entry(
                        entry,
                        self.settings_manager.get().virtual_texture_threshold,
                    )
                };
                if let Some(cached) = self.image_cache.get_mut(&image_id) {
                    cached.mipmap_state = Some(state);
                    ctx.request_repaint();
                    return;
                }
            }
        }
    }

    /// Cached mip states that need a Glow callback this frame. A state already
    /// submitted by the main-view callback is skipped; cached neighbours and an
    /// outgoing texture retained as a loading placeholder continue in the background.
    pub(crate) fn cached_mipmap_uploads_for_frame(
        &self,
    ) -> Vec<(egui::TextureId, SharedMipmapState)> {
        if self.settings_manager.get().texture_filter != "mipmap"
            || self.should_stop_caching
            || !self.workspace.should_preload()
        {
            return Vec::new();
        }

        let len = self.image_entries.len();
        if len <= 1 {
            return Vec::new();
        }

        let mut uploads = Vec::new();
        let mut seen_image_ids = HashSet::new();
        for distance in 1..=self.cache_radius {
            let forward = self.get_circular_index(self.current_index, distance as isize);
            let backward = self.get_circular_index(self.current_index, -(distance as isize));

            for index in [forward, backward] {
                if index == self.current_index {
                    continue;
                }
                let Some(image_id) = self.get_image_id(index) else {
                    continue;
                };
                if !seen_image_ids.insert(image_id.clone()) {
                    continue;
                }
                let Some(cached) = self.image_cache.peek(&image_id) else {
                    continue;
                };
                if cached.texture_filter != "mipmap" || cached.is_gif || cached.is_preview {
                    continue;
                }
                if let Some(state) = cached
                    .mipmap_state
                    .as_ref()
                    .filter(|state| is_mipmap_state_pending(state))
                {
                    // Skip only when the main viewer already submitted this exact
                    // worker state. During navigation, `texture_image_id` can
                    // still identify the outgoing loading-placeholder texture
                    // even though its foreground state was deliberately cleared.
                    let handled_by_main_view = self.texture_image_id.as_deref()
                        == Some(image_id.as_str())
                        && self
                            .current_mipmap_state
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, state));
                    if !handled_by_main_view {
                        uploads.push((cached.texture.id(), Arc::clone(state)));
                    }
                }
            }
        }
        uploads
    }

    // ====== MAIN PRELOAD PIPELINE ======

    pub fn preload_adjacent_images(&mut self) {
        // Guard at the pipeline entry point as well as at the UI call sites.
        // Settings panels and other helpers can request preloading directly;
        // Masonry/Compare must never start the normal adjacent-image pipeline.
        if !self.workspace.should_preload() {
            self.should_stop_caching = true;
            return;
        }
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
        self.cancel_preloads_outside(&desired_set);

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

    pub(crate) fn cancel_all_preload_tasks(&mut self) {
        for task in &self.preload_tasks {
            if !task.cancellation.swap(true, Ordering::AcqRel) {
                crate::io_metrics::preload_cancelled();
            }
        }
        self.preloading_indices = self
            .preload_tasks
            .iter()
            .filter(|task| task.generation == self.preload_generation)
            .map(|task| task.index)
            .collect();
    }

    fn cancel_preloads_outside(&mut self, desired: &HashSet<usize>) {
        for task in &self.preload_tasks {
            if !desired.contains(&task.index)
                && !task.cancellation.swap(true, Ordering::AcqRel)
            {
                crate::io_metrics::preload_cancelled();
            }
        }
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
        let retained_is_still_cached = self
            .preloaded_mipmap_pixels
            .as_ref()
            .is_some_and(|(image_id, _, _, _)| self.image_cache.contains(image_id));
        if !retained_is_still_cached {
            self.preloaded_mipmap_pixels = None;
        }
    }

    // ====== PRELOAD RESULT PROCESSING ======

    /// Collect workers that have finished normally.
    fn collect_completed_preload_tasks(
        &mut self,
    ) -> Vec<(usize, Result<LoadedEntryData, String>, u64)> {
        let mut completed = Vec::new();
        let mut remove_indices = Vec::new();

        for (task_idx, task) in self.preload_tasks.iter_mut().enumerate() {
            match task.receiver.try_recv() {
                Ok((idx, result, generation)) => {
                    self.preload_workers = self.preload_workers.saturating_sub(1);
                    completed.push((idx, result, generation));
                    remove_indices.push(task_idx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Cancelled workers intentionally drop their sender instead
                    // of publishing stale results. Release their slot only now,
                    // after any non-interruptible archive operation has returned.
                    self.preload_workers = self.preload_workers.saturating_sub(1);
                    remove_indices.push(task_idx);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        for task_idx in remove_indices.into_iter().rev() {
            self.preload_tasks.remove(task_idx);
        }
        self.preloading_indices = self
            .preload_tasks
            .iter()
            .filter(|task| task.generation == self.preload_generation)
            .map(|task| task.index)
            .collect();

        completed
    }

    /// Remove and mark preload tasks that exceeded the timeout.
    fn process_preload_timeouts(&mut self) {
        for task in &self.preload_tasks {
            if task.start_time.elapsed() >= PRELOAD_TIMEOUT
                && !task.cancellation.swap(true, Ordering::AcqRel)
            {
                crate::io_metrics::preload_cancelled();
                if task.generation == self.preload_generation {
                    self.preload_skipped.insert(task.index);
                }
                eprintln!(
                    "Preload: cancelled index {} after {:?}; worker slot remains occupied until it exits",
                    task.index, PRELOAD_TIMEOUT
                );
            }
        }
    }

    /// Process one completed preload result.
    fn process_preload_result(
        &mut self,
        ctx: &egui::Context,
        idx: usize,
        result: Result<LoadedEntryData, String>,
    ) {
        match result {
            Ok(loaded_entry) => {
                if let LoadedImage::VirtualPending(_, width, height) = &loaded_entry.image {
                    self.preload_skipped.insert(idx);
                    println!(
                        "Preload: index {} skipped for virtual texturing ({}x{})",
                        idx, width, height
                    );
                } else {
                    self.preload_skipped.remove(&idx);
                    self.add_to_cache(ctx, idx, loaded_entry);
                }
            }
            Err(err) if err == "Preload cancelled" => {
                // A superseded speculative task is not a failed or skipped image.
            }
            Err(err) => {
                eprintln!("Preload: failed to load index {}: {}", idx, err);
                self.preload_skipped.insert(idx);
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

        // Invalidate results and cooperatively stop all speculative workers.
        // Archive APIs that only support whole-entry extraction may finish
        // their current operation before observing cancellation.
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.cancel_all_preload_tasks();

        // Clear normal cache.
        self.image_cache.clear();
        self.preloaded_mipmap_pixels = None;
        self.preload_working = false;
    }
}
