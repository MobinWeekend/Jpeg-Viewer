//! Background generation and collection of sidebar thumbnails.
use super::super::types::{GeneratedThumbnail, ViewerApp};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

type ThumbnailJob = Box<dyn FnOnce() + Send + 'static>;
type ThumbnailStartedAt = Arc<Mutex<Option<Instant>>>;

fn mark_thumbnail_started(started_at: &ThumbnailStartedAt) {
    let mut started_at = started_at.lock().expect("thumbnail start time poisoned");
    started_at.get_or_insert_with(Instant::now);
}

fn thumbnail_started_elapsed(started_at: &ThumbnailStartedAt) -> Option<Duration> {
    started_at
        .lock()
        .expect("thumbnail start time poisoned")
        .as_ref()
        .map(Instant::elapsed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThumbnailPool {
    Dimensions,
    Native,
    Decoder,
}

impl ThumbnailPool {
    fn index(self) -> usize {
        match self {
            Self::Dimensions => 0,
            Self::Native => 1,
            Self::Decoder => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Dimensions => "dimensions",
            Self::Native => "native",
            Self::Decoder => "decoder",
        }
    }
}

struct QueuedThumbnailJob {
    key: String,
    priority: u8,
    sequence: u64,
    masonry_source_key: Option<String>,
    cancellation: Option<Arc<AtomicBool>>,
    /// Only Masonry's up-front layout-dimension pass blocks native thumbnail
    /// work. Just-in-time sidebar probes are non-blocking for other items.
    blocks_lower_stages: bool,
    run: ThumbnailJob,
}

struct ThumbnailSchedulerState {
    dimension_queue: Vec<QueuedThumbnailJob>,
    native_queue: Vec<QueuedThumbnailJob>,
    decoder_queue: Vec<QueuedThumbnailJob>,
    active: [usize; 3],
    // Cancelled/timed-out calls no longer hold the priority barrier even if
    // an uninterruptible platform API has not returned yet.
    active_cancellations: [Vec<Arc<AtomicBool>>; 3],
    active_dimension_barriers: Vec<Arc<AtomicBool>>,
}

impl ThumbnailSchedulerState {
    fn queue_mut(&mut self, pool: ThumbnailPool) -> &mut Vec<QueuedThumbnailJob> {
        match pool {
            ThumbnailPool::Dimensions => &mut self.dimension_queue,
            ThumbnailPool::Native => &mut self.native_queue,
            ThumbnailPool::Decoder => &mut self.decoder_queue,
        }
    }

    fn queue(&self, pool: ThumbnailPool) -> &Vec<QueuedThumbnailJob> {
        match pool {
            ThumbnailPool::Dimensions => &self.dimension_queue,
            ThumbnailPool::Native => &self.native_queue,
            ThumbnailPool::Decoder => &self.decoder_queue,
        }
    }

    fn active_uncancelled(&self, pool: ThumbnailPool) -> usize {
        let index = pool.index();
        let cancelled = self.active_cancellations[index]
            .iter()
            .filter(|token| token.load(Ordering::Acquire))
            .count();
        self.active[index].saturating_sub(cancelled)
    }

    fn has_blocking_dimension_work(&self) -> bool {
        self.dimension_queue
            .iter()
            .any(|queued| queued.blocks_lower_stages)
            || self
                .active_dimension_barriers
                .iter()
                .any(|token| !token.load(Ordering::Acquire))
    }

    fn can_start(&self, pool: ThumbnailPool) -> bool {
        match pool {
            ThumbnailPool::Dimensions => true,
            ThumbnailPool::Native => !self.has_blocking_dimension_work(),
            ThumbnailPool::Decoder => {
                !self.has_blocking_dimension_work()
                    && self.native_queue.is_empty()
                    && self.active_uncancelled(ThumbnailPool::Native) == 0
            }
        }
    }
}

/// Three physically separate pools share a priority coordinator:
///
/// - dimensions: 3 workers per configured base worker
/// - native OS/DE thumbnails: 2 workers per configured base worker
/// - Jpeg-Viewer decoding: 1 worker per configured base worker
///
/// Jobs may optionally establish a stage barrier. Only operations which truly
/// need exclusive priority use it; visible Masonry dimension probes are
/// non-blocking so native previews can appear while metadata is being measured.
struct ThumbnailScheduler {
    state: Mutex<ThumbnailSchedulerState>,
    ready: Condvar,
    next_sequence: AtomicU64,
    masonry_jobs: Arc<Mutex<HashMap<String, (String, Arc<AtomicBool>)>>>,
}

impl ThumbnailScheduler {
    fn submit_masonry(
        &self,
        pool: ThumbnailPool,
        key: String,
        priority: u8,
        source_key: String,
        cancelled: Arc<AtomicBool>,
        blocks_lower_stages: bool,
        job: ThumbnailJob,
    ) -> bool {
        {
            let mut registry = self
                .masonry_jobs
                .lock()
                .expect("Masonry thumbnail registry poisoned");
            registry.insert(key.clone(), (source_key.clone(), Arc::clone(&cancelled)));
        }
        let submitted = self
            .submit_stage(
                pool,
                key.clone(),
                priority,
                Some(source_key),
                Some(Arc::clone(&cancelled)),
                blocks_lower_stages,
                job,
            )
            .is_ok();
        if !submitted {
            self.finish_masonry(&key, &cancelled);
        }
        submitted
    }

    fn submit_stage(
        &self,
        pool: ThumbnailPool,
        key: String,
        priority: u8,
        masonry_source_key: Option<String>,
        cancellation: Option<Arc<AtomicBool>>,
        blocks_lower_stages: bool,
        job: ThumbnailJob,
    ) -> Result<(), ThumbnailJob> {
        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        if cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Acquire))
        {
            return Err(job);
        }
        let queue = state.queue_mut(pool);
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        queue.push(QueuedThumbnailJob {
            key,
            priority,
            sequence,
            masonry_source_key,
            cancellation,
            blocks_lower_stages,
            run: job,
        });
        self.ready.notify_all();
        Ok(())
    }

    fn finish_masonry(&self, key: &str, cancellation: &Arc<AtomicBool>) {
        let mut registry = self
            .masonry_jobs
            .lock()
            .expect("Masonry thumbnail registry poisoned");
        if registry
            .get(key)
            .is_some_and(|(_, token)| Arc::ptr_eq(token, cancellation))
        {
            registry.remove(key);
        }
    }

    fn cancel_masonry_not_visible(&self, visible_sources: &HashSet<String>) {
        {
            let mut registry = self
                .masonry_jobs
                .lock()
                .expect("Masonry thumbnail registry poisoned");
            registry.retain(|_, (source_key, token)| {
                if visible_sources.contains(source_key) {
                    true
                } else {
                    token.store(true, Ordering::Release);
                    false
                }
            });
        }

        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        state.dimension_queue.retain(|queued| {
            queued
                .masonry_source_key
                .as_ref()
                .is_none_or(|source_key| visible_sources.contains(source_key))
        });
        state.native_queue.retain(|queued| {
            queued
                .masonry_source_key
                .as_ref()
                .is_none_or(|source_key| visible_sources.contains(source_key))
        });
        state.decoder_queue.retain(|queued| {
            queued
                .masonry_source_key
                .as_ref()
                .is_none_or(|source_key| visible_sources.contains(source_key))
        });
        self.ready.notify_all();
    }

    fn promote(&self, key: &str, priority: u8) {
        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        for pool in [
            ThumbnailPool::Dimensions,
            ThumbnailPool::Native,
            ThumbnailPool::Decoder,
        ] {
            if let Some(queued) = state
                .queue_mut(pool)
                .iter_mut()
                .find(|queued| queued.key == key)
            {
                // `priority` is the user-facing thumbnail priority. Dimension
                // tasks use a separate scale so even a selected sidebar item
                // cannot jump above Masonry's full-layout dimension barrier.
                let stage_priority = if pool == ThumbnailPool::Dimensions {
                    sidebar_dimension_priority(priority)
                } else {
                    priority
                };
                queued.priority = queued.priority.max(stage_priority);
                self.ready.notify_all();
                return;
            }
        }
    }

    fn take_next(&self, pool: ThumbnailPool) -> QueuedThumbnailJob {
        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        loop {
            if state.can_start(pool) && !state.queue(pool).is_empty() {
                let next_index = state
                    .queue(pool)
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, queued)| (std::cmp::Reverse(queued.priority), queued.sequence))
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                let job = state.queue_mut(pool).swap_remove(next_index);
                state.active[pool.index()] += 1;
                if let Some(cancellation) = job.cancellation.as_ref() {
                    state.active_cancellations[pool.index()].push(Arc::clone(cancellation));
                    if pool == ThumbnailPool::Dimensions && job.blocks_lower_stages {
                        state
                            .active_dimension_barriers
                            .push(Arc::clone(cancellation));
                    }
                }
                return job;
            }

            #[cfg(target_os = "windows")]
            if pool != ThumbnailPool::Decoder {
                // Both dimensions (Shell property metadata) and native preview
                // workers use STA COM. Timed wakeups dispatch messages while idle.
                let (next_state, _) = self
                    .ready
                    .wait_timeout(state, Duration::from_millis(40))
                    .expect("thumbnail scheduler poisoned");
                state = next_state;
                if !state.can_start(pool) || state.queue(pool).is_empty() {
                    drop(state);
                    crate::platform::thumbnails::pump_worker_messages();
                    state = self.state.lock().expect("thumbnail scheduler poisoned");
                }
            } else {
                state = self
                    .ready
                    .wait(state)
                    .expect("thumbnail scheduler poisoned");
            }

            #[cfg(not(target_os = "windows"))]
            {
                state = self
                    .ready
                    .wait(state)
                    .expect("thumbnail scheduler poisoned");
            }
        }
    }

    fn cancel_job(&self, cancellation: &Arc<AtomicBool>) {
        cancellation.store(true, Ordering::Release);

        // Remove a timed-out request if it has not started yet. Otherwise its
        // stale queue entry could keep a lower-priority pool blocked forever.
        self.masonry_jobs
            .lock()
            .expect("Masonry thumbnail registry poisoned")
            .retain(|_, (_, token)| !Arc::ptr_eq(token, cancellation));

        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        for pool in [
            ThumbnailPool::Dimensions,
            ThumbnailPool::Native,
            ThumbnailPool::Decoder,
        ] {
            state.queue_mut(pool).retain(|queued| {
                queued
                    .cancellation
                    .as_ref()
                    .is_none_or(|token| !Arc::ptr_eq(token, cancellation))
            });
        }
        drop(state);

        // Wake lower-priority workers: the timed-out job no longer blocks
        // their stage even if its underlying OS call cannot be interrupted.
        self.ready.notify_all();
    }

    fn worker_finished(
        &self,
        pool: ThumbnailPool,
        cancellation: Option<&Arc<AtomicBool>>,
        blocks_lower_stages: bool,
    ) {
        let mut state = self.state.lock().expect("thumbnail scheduler poisoned");
        state.active[pool.index()] = state.active[pool.index()].saturating_sub(1);
        if let Some(cancellation) = cancellation {
            if let Some(index) = state.active_cancellations[pool.index()]
                .iter()
                .position(|token| Arc::ptr_eq(token, cancellation))
            {
                state.active_cancellations[pool.index()].swap_remove(index);
            }
            if pool == ThumbnailPool::Dimensions
                && blocks_lower_stages
                && let Some(index) = state
                    .active_dimension_barriers
                    .iter()
                    .position(|token| Arc::ptr_eq(token, cancellation))
            {
                state.active_dimension_barriers.swap_remove(index);
            }
        }
        self.ready.notify_all();
    }
}

static THUMBNAIL_SCHEDULER: OnceLock<Arc<ThumbnailScheduler>> = OnceLock::new();

/// Convert the user-facing worker multiplier to isolated pool sizes.
/// A setting of 3 therefore means 9 dimension, 6 native, and 3 decoder
/// workers (18 threads total).
fn thumbnail_pool_worker_counts(base_worker_count: usize) -> [usize; 3] {
    let base = base_worker_count.clamp(1, 8);
    [base * 3, base * 2, base]
}

fn thumbnail_scheduler(base_worker_count: usize) -> &'static Arc<ThumbnailScheduler> {
    THUMBNAIL_SCHEDULER.get_or_init(|| {
        let base = base_worker_count.clamp(1, 8);
        let scheduler = Arc::new(ThumbnailScheduler {
            state: Mutex::new(ThumbnailSchedulerState {
                dimension_queue: Vec::new(),
                native_queue: Vec::new(),
                decoder_queue: Vec::new(),
                active: [0; 3],
                active_cancellations: [Vec::new(), Vec::new(), Vec::new()],
                active_dimension_barriers: Vec::new(),
            }),
            ready: Condvar::new(),
            next_sequence: AtomicU64::new(0),
            masonry_jobs: Arc::new(Mutex::new(HashMap::new())),
        });

        let worker_counts = thumbnail_pool_worker_counts(base);
        for (pool, worker_count) in [
            (ThumbnailPool::Dimensions, worker_counts[0]),
            (ThumbnailPool::Native, worker_counts[1]),
            (ThumbnailPool::Decoder, worker_counts[2]),
        ] {
            for worker_index in 0..worker_count {
                let worker_scheduler = Arc::clone(&scheduler);
                let thread_name = format!("thumbnail-{}-worker-{worker_index}", pool.label());
                if let Err(error) = std::thread::Builder::new()
                    .name(thread_name)
                    .spawn(move || {
                        // Windows Shell image properties and native previews both
                        // require COM. Keep COM initialization on every worker that
                        // can call into Shell, and isolate pixel decoding from COM.
                        let _platform_context = (pool != ThumbnailPool::Decoder)
                            .then(crate::platform::thumbnails::ThumbnailWorkerContext::initialize);
                        loop {
                            let queued = worker_scheduler.take_next(pool);
                            let job_key = queued.key.clone();
                            let cancellation = queued.cancellation.clone();
                            let blocks_lower_stages = queued.blocks_lower_stages;
                            let is_cancelled = cancellation
                                .as_ref()
                                .is_some_and(|token| token.load(Ordering::Acquire));
                            if is_cancelled {
                                if let Some(cancellation) = cancellation.as_ref() {
                                    worker_scheduler.finish_masonry(&job_key, cancellation);
                                }
                            } else {
                                let run_job = queued.run;
                                let result =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        run_job()
                                    }));
                                if result.is_err() {
                                    eprintln!(
                                        "[thumbnail] {} worker panicked while processing {job_key}",
                                        pool.label()
                                    );
                                    if let Some(cancellation) = cancellation.as_ref() {
                                        worker_scheduler.finish_masonry(&job_key, cancellation);
                                    }
                                } else if let Some(cancellation) = cancellation
                                    .as_ref()
                                    .filter(|token| token.load(Ordering::Acquire))
                                {
                                    worker_scheduler.finish_masonry(&job_key, cancellation);
                                }
                            }
                            worker_scheduler.worker_finished(
                                pool,
                                cancellation.as_ref(),
                                blocks_lower_stages,
                            );
                            if pool != ThumbnailPool::Decoder {
                                crate::platform::thumbnails::pump_worker_messages();
                            }
                        }
                    })
                {
                    eprintln!(
                        "[thumbnail] failed to start {} worker {worker_index}: {error}",
                        pool.label()
                    );
                }
            }
        }
        scheduler
    })
}

#[cfg(test)]
mod thumbnail_scheduler_tests {
    use super::{
        QueuedThumbnailJob, ThumbnailPool, ThumbnailScheduler, ThumbnailSchedulerState,
        thumbnail_pool_worker_counts,
    };
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::{Arc, Condvar, Mutex};

    fn empty_state() -> ThumbnailSchedulerState {
        ThumbnailSchedulerState {
            dimension_queue: Vec::new(),
            native_queue: Vec::new(),
            decoder_queue: Vec::new(),
            active: [0; 3],
            active_cancellations: [Vec::new(), Vec::new(), Vec::new()],
            active_dimension_barriers: Vec::new(),
        }
    }

    #[test]
    fn job_timeout_clock_is_unset_until_a_worker_starts_it() {
        let started_at = Arc::new(Mutex::new(None));
        assert!(super::thumbnail_started_elapsed(&started_at).is_none());
        super::mark_thumbnail_started(&started_at);
        assert!(super::thumbnail_started_elapsed(&started_at).is_some());
    }

    #[test]
    fn worker_multiplier_three_creates_nine_six_and_three_workers() {
        assert_eq!(thumbnail_pool_worker_counts(3), [9, 6, 3]);
    }

    #[test]
    fn worker_multiplier_is_clamped_to_supported_range() {
        assert_eq!(thumbnail_pool_worker_counts(0), [3, 2, 1]);
        assert_eq!(thumbnail_pool_worker_counts(99), [24, 16, 8]);
    }

    #[test]
    fn sidebar_dimension_priority_stays_below_masonry_and_prefetch() {
        assert!(
            super::sidebar_dimension_priority(super::SIDEBAR_CURRENT_PRIORITY)
                < super::MASONRY_DIMENSION_PRIORITY
        );
        assert!(
            super::sidebar_dimension_priority(super::SIDEBAR_HOVER_PRIORITY)
                > super::sidebar_dimension_priority(super::SIDEBAR_VISIBLE_PRIORITY)
        );
        assert!(
            super::sidebar_dimension_priority(super::SIDEBAR_VISIBLE_PRIORITY)
                > super::sidebar_dimension_priority(super::SIDEBAR_PREFETCH_PRIORITY)
        );
    }

    #[test]
    fn native_workers_wait_for_all_dimension_work() {
        let mut state = empty_state();
        state.active[ThumbnailPool::Dimensions.index()] = 1;
        state
            .active_dimension_barriers
            .push(Arc::new(AtomicBool::new(false)));
        assert!(!state.can_start(ThumbnailPool::Native));
        state.active[ThumbnailPool::Dimensions.index()] = 0;
        state.active_dimension_barriers.clear();
        state.dimension_queue.push(QueuedThumbnailJob {
            key: "dimension-test".into(),
            priority: 20,
            sequence: 0,
            masonry_source_key: None,
            cancellation: None,
            blocks_lower_stages: true,
            run: Box::new(|| {}),
        });
        assert!(!state.can_start(ThumbnailPool::Native));
        assert!(state.can_start(ThumbnailPool::Dimensions));
    }

    #[test]
    fn sidebar_dimension_jobs_do_not_block_native_or_decoder_work_for_other_items() {
        let mut state = empty_state();
        state.dimension_queue.push(QueuedThumbnailJob {
            key: "sidebar-dimension-test".into(),
            priority: 6,
            sequence: 0,
            masonry_source_key: None,
            cancellation: None,
            blocks_lower_stages: false,
            run: Box::new(|| {}),
        });
        assert!(state.can_start(ThumbnailPool::Native));
        state.active[ThumbnailPool::Native.index()] = 0;
        assert!(state.can_start(ThumbnailPool::Decoder));
    }

    #[test]
    fn visible_masonry_dimension_probe_does_not_block_native_thumbnails() {
        let scheduler = ThumbnailScheduler {
            state: Mutex::new(empty_state()),
            ready: Condvar::new(),
            next_sequence: AtomicU64::new(0),
            masonry_jobs: Arc::new(Mutex::new(HashMap::new())),
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        assert!(scheduler.submit_masonry(
            ThumbnailPool::Dimensions,
            "masonry-dimensions:visible-file".into(),
            20,
            "visible-file".into(),
            cancellation,
            false,
            Box::new(|| {}),
        ));

        let state = scheduler.state.lock().unwrap();
        assert!(!state.dimension_queue[0].blocks_lower_stages);
        assert!(state.can_start(ThumbnailPool::Native));
    }

    #[test]
    fn cancelled_active_dimension_request_does_not_block_native_pool() {
        let mut state = empty_state();
        let cancellation = Arc::new(AtomicBool::new(true));
        state.active[ThumbnailPool::Dimensions.index()] = 1;
        state.active_cancellations[ThumbnailPool::Dimensions.index()]
            .push(Arc::clone(&cancellation));
        state
            .active_dimension_barriers
            .push(Arc::clone(&cancellation));
        assert!(state.can_start(ThumbnailPool::Native));
    }

    #[test]
    fn decoder_workers_wait_for_all_native_and_dimension_work() {
        let mut state = empty_state();
        state.active[ThumbnailPool::Native.index()] = 1;
        assert!(!state.can_start(ThumbnailPool::Decoder));
        state.active[ThumbnailPool::Native.index()] = 0;
        state.native_queue.push(QueuedThumbnailJob {
            key: "native-test".into(),
            priority: 10,
            sequence: 0,
            masonry_source_key: None,
            cancellation: None,
            blocks_lower_stages: false,
            run: Box::new(|| {}),
        });
        assert!(!state.can_start(ThumbnailPool::Decoder));
        state.native_queue.clear();
        state.active[ThumbnailPool::Dimensions.index()] = 1;
        state
            .active_dimension_barriers
            .push(Arc::new(AtomicBool::new(false)));
        assert!(!state.can_start(ThumbnailPool::Decoder));
        state.active = [0; 3];
        state.active_dimension_barriers.clear();
        assert!(state.can_start(ThumbnailPool::Decoder));
    }
}

pub(crate) fn update_masonry_thumbnail_visibility(visible_sources: &HashSet<String>) {
    if let Some(scheduler) = THUMBNAIL_SCHEDULER.get() {
        scheduler.cancel_masonry_not_visible(visible_sources);
    }
}

const MASONRY_DIMENSION_PRIORITY: u8 = 20;
const DIMENSION_JOB_TIMEOUT: Duration = Duration::from_secs(20);
const THUMBNAIL_JOB_TIMEOUT: Duration = Duration::from_secs(30);
const DIMENSION_QUEUE_TIMEOUT: Duration = Duration::from_secs(120);
const THUMBNAIL_QUEUE_TIMEOUT: Duration = Duration::from_secs(120);
const MASONRY_PLACEHOLDER_PRIORITY: u8 = 12;
const MASONRY_THUMBNAIL_PRIORITY: u8 = 10;
const SIDEBAR_THUMBNAIL_PRIORITY: u8 = 0;
const SIDEBAR_DIMENSION_PRIORITY: u8 = 6;
const SIDEBAR_PREFETCH_PRIORITY: u8 = 20;
pub(crate) const SIDEBAR_VISIBLE_PRIORITY: u8 = 60;
pub(crate) const SIDEBAR_HOVER_PRIORITY: u8 = 85;
pub(crate) const SIDEBAR_CURRENT_PRIORITY: u8 = 110;
pub(crate) const SIDEBAR_PLACEHOLDER_SIZE: u32 = 128;

/// Keep sidebar metadata probes below Masonry's layout barrier, while still
/// ordering the currently visible/hovered/current item ahead of lookahead work.
fn sidebar_dimension_priority(thumbnail_priority: u8) -> u8 {
    if thumbnail_priority >= SIDEBAR_CURRENT_PRIORITY {
        MASONRY_DIMENSION_PRIORITY.saturating_sub(1)
    } else if thumbnail_priority >= SIDEBAR_HOVER_PRIORITY {
        MASONRY_DIMENSION_PRIORITY.saturating_sub(2)
    } else if thumbnail_priority >= SIDEBAR_VISIBLE_PRIORITY {
        MASONRY_DIMENSION_PRIORITY.saturating_sub(3)
    } else if thumbnail_priority >= SIDEBAR_PREFETCH_PRIORITY {
        SIDEBAR_DIMENSION_PRIORITY.saturating_add(1)
    } else {
        SIDEBAR_DIMENSION_PRIORITY
    }
}
const MASONRY_PLACEHOLDER_SIZE: u32 = 128;

fn shared_thumbnail_key(key: &str, size: u32) -> String {
    // Resolution is an exact request value, not one of a few application-wide
    // buckets. Native providers may return their closest supported size.
    format!("thumb:{}:{}", size.max(1), key)
}

/// Return the physical-pixel bounding-box size needed by a thumbnail widget.
/// Using the maximum side also works for portrait previews and wide rows.
pub(crate) fn thumbnail_request_size(rect: egui::Rect, ctx: &egui::Context) -> u32 {
    let physical_side = rect.width().max(rect.height()) * ctx.pixels_per_point();
    if !physical_side.is_finite() || physical_side <= 1.0 {
        return 1;
    }
    physical_side.ceil().min(u32::MAX as f32) as u32
}

/// Fit a thumbnail's aspect ratio inside a UI rectangle without cropping.
/// Shared by Masonry, sidebar cards, archive previews, and New Tab cards so
/// thumbnail presentation stays consistent across the application.
pub(crate) fn thumbnail_fit_rect(bounds: egui::Rect, aspect_ratio: f32) -> egui::Rect {
    let bounds_width = bounds.width().max(0.0);
    let bounds_height = bounds.height().max(0.0);
    if bounds_width <= 0.0 || bounds_height <= 0.0 {
        return egui::Rect::from_center_size(bounds.center(), egui::vec2(0.0, 0.0));
    }

    let ratio = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio
    } else {
        bounds_width / bounds_height
    };
    let bounds_ratio = bounds_width / bounds_height;
    let size = if bounds_ratio > ratio {
        egui::vec2(bounds_height * ratio, bounds_height)
    } else {
        egui::vec2(bounds_width, bounds_width / ratio)
    };
    egui::Rect::from_center_size(bounds.center(), size)
}

fn exif_orientation(exif: &exif::Exif) -> Option<u32> {
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
}

fn oriented_dimensions(width: u32, height: u32, orientation: Option<u32>) -> (u32, u32) {
    if matches!(orientation, Some(5..=8)) {
        (height, width)
    } else {
        (width, height)
    }
}

fn exif_dimensions(exif: &exif::Exif) -> Option<(u32, u32)> {
    let width = exif
        .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .or_else(|| {
            exif.get_field(exif::Tag::ImageWidth, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })?;
    let height = exif
        .get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .or_else(|| {
            exif.get_field(exif::Tag::ImageLength, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })?;
    (width > 0 && height > 0).then_some((width, height))
}

/// Read dimensions through Jpeg-Viewer's format registry. This is used for
/// archive entries and as the fallback for formats not understood by image's
/// lightweight file-header reader. EXIF orientation is applied so the result
/// matches the image displayed by the viewer.
fn source_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    let format = crate::decoder::format_detection::detect_format(bytes)?;
    let (width, height) = crate::decoder::default_registry()
        .dimensions(bytes, format)
        .ok()?;
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(bytes))
        .ok();
    let dimensions = oriented_dimensions(width, height, exif.as_ref().and_then(exif_orientation));
    (dimensions.0 > 0 && dimensions.1 > 0).then_some(dimensions)
}

/// Read original dimensions from a file without reading or decoding the full
/// source. The lightweight image-header reader and EXIF tags handle common
/// formats; keeping this fast is important because native desktop thumbnails
/// should be usable without first loading large source files into memory.
fn source_dimensions_from_file(path: &std::path::Path) -> Option<(u32, u32)> {
    crate::io_metrics::metadata_query();
    // Use image's optimized path-based header probe. It avoids the additional
    // format-guessing and buffered-reader setup required by a generic reader.
    // EXIF is read separately because orientation is needed to keep Masonry's
    // layout consistent with the displayed image.
    let header_dimensions = image::image_dimensions(path).ok();
    let exif = File::open(path).ok().and_then(|file| {
        let mut reader = BufReader::new(file);
        exif::Reader::new().read_from_container(&mut reader).ok()
    });
    if let Some(dimensions) = header_dimensions
        .or_else(|| exif.as_ref().and_then(exif_dimensions))
        .or_else(|| crate::platform::thumbnails::source_dimensions(path))
    {
        let dimensions = oriented_dimensions(
            dimensions.0,
            dimensions.1,
            exif.as_ref().and_then(exif_orientation),
        );
        if dimensions.0 > 0 && dimensions.1 > 0 {
            return Some(dimensions);
        }
    }
    None
}

/// Read one image member from a supported archive. The caller chooses the
/// worker pool; Masonry dimensions use the metadata pool, while pixel decoding
/// remains isolated in the Jpeg-Viewer decoder pool.
fn read_archive_entry_bytes(entry: &crate::imaging::entry::ImageEntry) -> Result<Vec<u8>, String> {
    use std::io::Read;
    match entry {
        crate::imaging::entry::ImageEntry::File(path) => {
            let bytes = fs::read(path)
                .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
            crate::io_metrics::source_read(bytes.len());
            Ok(bytes)
        }
        crate::imaging::entry::ImageEntry::Zip(item) => {
            let file = File::open(&item.archive_path).map_err(|error| error.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
            let mut item_file = archive
                .by_index(item.entry_index)
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            item_file
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            crate::io_metrics::archive_extraction(bytes.len());
            Ok(bytes)
        }
        crate::imaging::entry::ImageEntry::S7z(item) => {
            let mut archive = sevenz_rust2::ArchiveReader::open(
                &item.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|error| error.to_string())?;
            let bytes = archive
                .read_file(&item.name)
                .map_err(|error| error.to_string())?;
            crate::io_metrics::archive_extraction(bytes.len());
            Ok(bytes)
        }
        crate::imaging::entry::ImageEntry::Rar(item) => {
            let mut archive = unrar::Archive::new(&item.archive_path)
                .open_for_processing()
                .map_err(|error| error.to_string())?;
            loop {
                let header = archive
                    .read_header()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("Entry not found: {}", item.name))?;
                if header.entry().filename.to_string_lossy() == item.name {
                    let (bytes, _next) = header.read().map_err(|error| error.to_string())?;
                    crate::io_metrics::archive_extraction(bytes.len());
                    break Ok(bytes);
                }
                archive = header.skip().map_err(|error| error.to_string())?;
            }
        }
    }
}

fn source_dimensions_from_entry(entry: &crate::imaging::entry::ImageEntry) -> Option<(u32, u32)> {
    match entry {
        crate::imaging::entry::ImageEntry::File(path) => source_dimensions_from_file(path),
        _ => read_archive_entry_bytes(entry)
            .ok()
            .and_then(|bytes| source_dimensions_from_bytes(&bytes)),
    }
}

/// Avoid enlarging a thumbnail beyond the source image's own maximum
/// dimension. The source dimensions are already EXIF-oriented.
fn thumbnail_output_limit(requested_size: u32, source_dimensions: Option<(u32, u32)>) -> u32 {
    source_dimensions
        .map(|(width, height)| requested_size.min(width.max(height)))
        .unwrap_or(requested_size)
        .max(1)
}

/// Native providers are allowed to return their nearest cache flavor. A result
/// at least two-thirds of the requested side is considered close enough to
/// avoid an expensive source decode. JPEG Viewer's own full decoder is capped
/// at 256px, so asking it to replace a 256px native result cannot improve the
/// cached output; the native acceptance floor is capped accordingly. The
/// low-resolution placeholder pass accepts any non-empty native result.
fn native_thumbnail_quality_floor(requested_size: u32) -> u32 {
    requested_size
        .min(96_u32.max(requested_size.saturating_mul(2).div_ceil(3)))
        .min(MAX_JPEG_VIEWER_DECODE_THUMBNAIL_SIZE)
}

/// Prefer the original source ratio even when the desktop preview has a
/// different shape. The renderer deliberately maps the native texture into
/// this ratio; preview dimensions are only a fallback when source metadata is
/// unavailable.
fn source_aspect_ratio_or_preview(
    source_aspect_ratio: Option<f32>,
    preview_width: u32,
    preview_height: u32,
) -> Option<f32> {
    source_aspect_ratio
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .or_else(|| {
            (preview_width > 0 && preview_height > 0)
                .then_some(preview_width as f32 / preview_height as f32)
        })
}

/// Keep a decoded thumbnail candidate independently of the original image's
/// safety-limited full decode. Native Shell previews must survive failed or
/// intentionally skipped decoder fallbacks. `resolution` records the real
/// provider pixels before any aspect-ratio normalization.
struct ThumbnailCandidate {
    image: image::RgbaImage,
    resolution: u32,
    source: &'static str,
}

fn offer_thumbnail_candidate(
    best: &mut Option<ThumbnailCandidate>,
    image: image::RgbaImage,
    source: &'static str,
) {
    let resolution = image.width().max(image.height());
    if resolution == 0 {
        return;
    }

    // Strictly greater preserves source preference on ties: native OS/DE first,
    // then embedded previews, then our own full decoder.
    if best
        .as_ref()
        .is_none_or(|candidate| resolution > candidate.resolution)
    {
        *best = Some(ThumbnailCandidate {
            image,
            resolution,
            source,
        });
    }
}

const MAX_JPEG_VIEWER_DECODE_THUMBNAIL_SIZE: u32 = 256;

/// JPEG Viewer's own full decoder remains a last-resort, modest-size fallback.
/// Native Shell/DE and embedded previews may retain larger source resolutions,
/// but decoding an entire original should not turn the application into a
/// competing high-resolution thumbnail provider.
fn candidate_output_limit(candidate: &ThumbnailCandidate, requested: u32) -> u32 {
    if candidate.source == "Jpeg-Viewer decoder" {
        requested.min(MAX_JPEG_VIEWER_DECODE_THUMBNAIL_SIZE)
    } else {
        requested
    }
}

fn complete_thumbnail_request(
    key: String,
    result: Result<GeneratedThumbnail, String>,
    tx: std::sync::mpsc::Sender<(String, Result<GeneratedThumbnail, String>)>,
    repaint_ctx: egui::Context,
    scheduler: &ThumbnailScheduler,
    cancellation: &Arc<AtomicBool>,
    masonry_only: bool,
) {
    if !cancellation.load(Ordering::Acquire) {
        let _ = tx.send((key.clone(), result));
        repaint_ctx.request_repaint();
    }
    if masonry_only {
        scheduler.finish_masonry(&key, cancellation);
    }
}

fn submit_pipeline_stage(
    scheduler: &ThumbnailScheduler,
    pool: ThumbnailPool,
    key: &str,
    priority: u8,
    masonry_source_key: Option<&str>,
    cancellation: &Arc<AtomicBool>,
    first_stage: bool,
    blocks_lower_stages: bool,
    job: ThumbnailJob,
) -> bool {
    if first_stage && let Some(source_key) = masonry_source_key {
        return scheduler.submit_masonry(
            pool,
            key.to_owned(),
            priority,
            source_key.to_owned(),
            Arc::clone(cancellation),
            blocks_lower_stages,
            job,
        );
    }

    scheduler
        .submit_stage(
            pool,
            key.to_owned(),
            priority,
            masonry_source_key.map(str::to_owned),
            Some(Arc::clone(cancellation)),
            blocks_lower_stages,
            job,
        )
        .is_ok()
}

/// Jpeg-Viewer's own fallback is intentionally run only by the dedicated
/// decoder pool. It retains a smaller native candidate when reading or decoding
/// the original fails, and never repeats the native OS/DE request.
fn decode_file_thumbnail_fallback(
    path: &std::path::Path,
    target_size: u32,
    mut source_dimensions: Option<(u32, u32)>,
    mut best_candidate: Option<ThumbnailCandidate>,
    masonry_placeholder: bool,
    cancellation: &AtomicBool,
) -> Result<GeneratedThumbnail, String> {
    if cancellation.load(Ordering::Acquire) {
        return Err("__masonry_thumbnail_cancelled__".into());
    }
    let mut source_aspect_ratio =
        source_dimensions.map(|(width, height)| width as f32 / height as f32);
    let bytes = {
        if let Ok(metadata) = fs::metadata(path)
            && metadata.len() > MAX_THUMB_SOURCE_READ_BYTES
        {
            if let Some(candidate) = best_candidate.take() {
                let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                eprintln!(
                    "[thumbnail] {}: source file is {} bytes; skipping full-source fallback and using {} ({}x{})",
                    path.display(),
                    metadata.len(),
                    candidate.source,
                    candidate.image.width(),
                    candidate.image.height(),
                );
                return finish_thumbnail_candidate(
                    candidate,
                    output_limit,
                    source_aspect_ratio,
                    source_dimensions,
                );
            }
            return Err(format!(
                "Source file is too large for the thumbnail fallback read ({} bytes): {}",
                metadata.len(),
                path.display(),
            ));
        }
        if cancellation.load(Ordering::Acquire) {
            return Err("__masonry_thumbnail_cancelled__".into());
        }
        match fs::read(path) {
            Ok(bytes) => {
                crate::io_metrics::source_read(bytes.len());
                bytes
            }
            Err(error) => {
                if let Some(candidate) = best_candidate.take() {
                    eprintln!(
                        "[thumbnail] {}: source read failed ({error}); using {} ({}x{})",
                        path.display(),
                        candidate.source,
                        candidate.image.width(),
                        candidate.image.height(),
                    );
                    let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                    return finish_thumbnail_candidate(
                        candidate,
                        output_limit,
                        source_aspect_ratio,
                        source_dimensions,
                    );
                }
                return Err(format!("Failed to read {}: {error}", path.display()));
            }
        }
    };

    // Dimensions should normally arrive from the higher-priority metadata
    // pool. Retain this byte-based fallback for unsupported/unknown headers.
    source_dimensions = source_dimensions.or_else(|| source_dimensions_from_bytes(&bytes));
    source_aspect_ratio = source_aspect_ratio
        .or_else(|| source_dimensions.map(|(width, height)| width as f32 / height as f32));

    let is_raw = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::imaging::core::ImageFormat::from_extension)
        == Some(crate::imaging::core::ImageFormat::CameraRaw)
        || zenraw::is_raw_file(&bytes);
    let output_limit = thumbnail_output_limit(target_size, source_dimensions);

    if !is_raw
        && let Some(embedded) = extract_embedded_metadata_thumbnail(&bytes)
        && let Ok(image) = image::load_from_memory(&embedded)
    {
        let image = image.to_rgba8();
        let embedded_resolution = image.width().max(image.height());
        offer_thumbnail_candidate(&mut best_candidate, image, "EXIF embedded preview");

        if (masonry_placeholder && embedded_resolution > 0)
            || (embedded_resolution >= output_limit
                && best_candidate
                    .as_ref()
                    .is_some_and(|candidate| candidate.resolution >= output_limit))
        {
            return finish_thumbnail_candidate(
                best_candidate.expect("embedded candidate exists"),
                output_limit,
                source_aspect_ratio,
                source_dimensions,
            );
        }
    }

    if cancellation.load(Ordering::Acquire) {
        return Err("__masonry_thumbnail_cancelled__".into());
    }
    match decode_checked_sidebar_image(
        &bytes,
        &path.display().to_string(),
        is_raw,
        Some(path),
        source_dimensions,
    ) {
        Ok(decoded) => offer_thumbnail_candidate(
            &mut best_candidate,
            decoded,
            if is_raw {
                "embedded RAW preview"
            } else {
                "Jpeg-Viewer decoder"
            },
        ),
        Err(error) => {
            if let Some(candidate) = best_candidate.as_ref() {
                eprintln!(
                    "[thumbnail] {}: decoder fallback failed ({error}); using {} ({}x{})",
                    path.display(),
                    candidate.source,
                    candidate.image.width(),
                    candidate.image.height(),
                );
            } else {
                return Err(error);
            }
        }
    }

    let candidate = best_candidate.ok_or_else(|| {
        format!(
            "No usable native, embedded, or decoded thumbnail: {}",
            path.display()
        )
    })?;
    let output_limit = candidate_output_limit(&candidate, output_limit);
    finish_thumbnail_candidate(
        candidate,
        output_limit,
        source_aspect_ratio,
        source_dimensions,
    )
}

fn schedule_file_native_stage(
    scheduler: Arc<ThumbnailScheduler>,
    path: PathBuf,
    key: String,
    target_size: u32,
    source_dimensions: Option<(u32, u32)>,
    priority_token: Arc<AtomicU8>,
    masonry_placeholder: bool,
    masonry_only: bool,
    masonry_source_key: Option<String>,
    cancellation: Arc<AtomicBool>,
    tx: std::sync::mpsc::Sender<(String, Result<GeneratedThumbnail, String>)>,
    repaint_ctx: egui::Context,
    first_stage: bool,
    started_at: ThumbnailStartedAt,
) -> bool {
    let stage_scheduler = Arc::clone(&scheduler);
    let stage_cancellation = Arc::clone(&cancellation);
    let stage_source_key = masonry_source_key.clone();
    let stage_key = key.clone();
    let stage_path = path.clone();
    let stage_started_at = Arc::clone(&started_at);
    let stage_priority_token = Arc::clone(&priority_token);
    let native_job = Box::new(move || {
        if stage_cancellation.load(Ordering::Acquire) {
            if masonry_only {
                stage_scheduler.finish_masonry(&stage_key, &stage_cancellation);
            }
            return;
        }
        mark_thumbnail_started(&stage_started_at);

        let native_request_size = thumbnail_output_limit(target_size, source_dimensions);
        let native = if masonry_placeholder {
            crate::platform::thumbnails::load_or_request_placeholder(
                &stage_path,
                native_request_size,
            )
        } else {
            crate::platform::thumbnails::load_or_request(&stage_path, native_request_size)
        };

        let source_aspect_ratio =
            source_dimensions.map(|(width, height)| width as f32 / height as f32);
        let mut best_candidate = None;
        let mut ready_result = None;
        if let Some(native) = native {
            let native_width = native.width();
            let native_height = native.height();
            let native_resolution = native_width.max(native_height);
            offer_thumbnail_candidate(&mut best_candidate, native, "native OS/DE thumbnail");

            let quality_floor = native_thumbnail_quality_floor(native_request_size);
            let sufficient = native_resolution >= quality_floor;
            if masonry_placeholder && native_resolution > 0 {
                ready_result = best_candidate.take().map(|candidate| {
                    finish_thumbnail_candidate(
                        candidate,
                        native_request_size,
                        source_aspect_ratio,
                        source_dimensions,
                    )
                });
            } else if sufficient {
                if native_resolution < native_request_size
                    && crate::platform::thumbnails::diagnostics_enabled()
                {
                    eprintln!(
                        "[thumbnail] {}: accepting native {}x{} preview for {}px request (quality floor {}px)",
                        stage_path.display(),
                        native_width,
                        native_height,
                        native_request_size,
                        quality_floor,
                    );
                }
                ready_result = best_candidate.take().map(|candidate| {
                    finish_thumbnail_candidate(
                        candidate,
                        native_request_size,
                        source_aspect_ratio,
                        source_dimensions,
                    )
                });
            } else if crate::platform::thumbnails::diagnostics_enabled() {
                eprintln!(
                    "[thumbnail] {}: native OS/DE returned {}x{} for {}px request (quality floor {}px); queueing viewer fallback",
                    stage_path.display(),
                    native_width,
                    native_height,
                    native_request_size,
                    quality_floor,
                );
            }
        }

        if let Some(result) = ready_result {
            complete_thumbnail_request(
                stage_key.clone(),
                result,
                tx.clone(),
                repaint_ctx.clone(),
                &stage_scheduler,
                &stage_cancellation,
                masonry_only,
            );
            return;
        }

        if stage_cancellation.load(Ordering::Acquire) {
            if masonry_only {
                stage_scheduler.finish_masonry(&stage_key, &stage_cancellation);
            }
            return;
        }

        let decoder_scheduler = Arc::clone(&stage_scheduler);
        let decoder_cancellation = Arc::clone(&stage_cancellation);
        let decoder_key = stage_key.clone();
        let decoder_path = stage_path.clone();
        let decoder_tx = tx.clone();
        let decoder_repaint = repaint_ctx.clone();
        let decoder_source_key = stage_source_key.clone();
        let decoder_submit_key = decoder_key.clone();
        let decoder_job_scheduler = Arc::clone(&decoder_scheduler);
        let decoder_job_cancellation = Arc::clone(&decoder_cancellation);
        let decoder_priority_token = Arc::clone(&stage_priority_token);
        let decoder_job = Box::new(move || {
            let result = decode_file_thumbnail_fallback(
                &decoder_path,
                target_size,
                source_dimensions,
                best_candidate,
                masonry_placeholder,
                &decoder_job_cancellation,
            );
            complete_thumbnail_request(
                decoder_key,
                result,
                decoder_tx,
                decoder_repaint,
                &decoder_job_scheduler,
                &decoder_job_cancellation,
                masonry_only,
            );
        }) as ThumbnailJob;
        let submitted = decoder_scheduler
            .submit_stage(
                ThumbnailPool::Decoder,
                decoder_submit_key,
                decoder_priority_token.load(Ordering::Acquire),
                decoder_source_key,
                Some(decoder_cancellation.clone()),
                false,
                decoder_job,
            )
            .is_ok();
        if !submitted {
            complete_thumbnail_request(
                stage_key,
                Err("Jpeg-Viewer decoder request was cancelled before it could start".into()),
                tx.clone(),
                repaint_ctx.clone(),
                &stage_scheduler,
                &stage_cancellation,
                masonry_only,
            );
        }
    }) as ThumbnailJob;

    let submitted = submit_pipeline_stage(
        &scheduler,
        ThumbnailPool::Native,
        &key,
        priority_token.load(Ordering::Acquire),
        masonry_source_key.as_deref(),
        &cancellation,
        first_stage,
        false,
        native_job,
    );
    if !submitted && masonry_only {
        scheduler.finish_masonry(&key, &cancellation);
    }
    submitted
}

fn finish_thumbnail_candidate(
    candidate: ThumbnailCandidate,
    output_limit: u32,
    source_aspect_ratio: Option<f32>,
    source_dimensions: Option<(u32, u32)>,
) -> Result<GeneratedThumbnail, String> {
    let ratio = source_aspect_ratio_or_preview(
        source_aspect_ratio,
        candidate.image.width(),
        candidate.image.height(),
    );
    let (width, height, pixels, resolution) =
        fit_thumbnail_to(candidate.image, output_limit, ratio)?;
    Ok(GeneratedThumbnail {
        width,
        height,
        pixels,
        resolution,
        source_aspect_ratio: ratio,
        source_dimensions,
    })
}

/// Stable cache identity shared with Browse/Queue/Favorites. Their filesystem
/// cards normalize paths to an absolute path before constructing the key, so
/// Masonry must do the same even when a collection contains relative paths.
pub(crate) fn source_thumbnail_key(entry: &crate::imaging::entry::ImageEntry) -> String {
    match entry {
        crate::imaging::entry::ImageEntry::File(path) => {
            format!("file:{}", super::storage::absolute_path(path).display())
        }
        _ => entry.get_id(),
    }
}

/// Reject suspiciously large decoded sources before asking a decoder to allocate
/// a full-resolution RGBA buffer. This is a safety ceiling, not a target size.
// Permit large photographic sources while keeping worst-case RGBA allocation bounded.
// The thumbnail is immediately reduced to the requested widget resolution.
const MAX_THUMB_SOURCE_PIXELS: u64 = 100_000_000;
// Only used when a native square preview arrives and lightweight file metadata
// could not provide the source ratio. Reuse these bytes for the fallback decode.
// Don't load an arbitrarily large compressed source into memory just because
// a native preview is smaller than the requested display resolution. When a
// native candidate exists, it remains usable if this fallback read is skipped.
const MAX_THUMB_SOURCE_READ_BYTES: u64 = 128 * 1024 * 1024;

/// Calculate thumbnail dimensions after fitting the provider's native preview
/// into the requested square bound, then correcting its aspect ratio to match
/// the original image. The corrected long side may exceed `max_side`: for
/// example, a 128x128 KDE preview requested at 128px becomes 228x128 for a
/// 16:9 source. The shorter side remains bounded by the requested resolution.
fn aspect_corrected_dimensions(
    width: u32,
    height: u32,
    source_aspect_ratio: Option<f32>,
) -> (u32, u32) {
    const MAX_NORMALIZED_SIDE: u32 = 16_384;
    const MAX_NORMALIZED_PIXELS: u64 = 16_777_216;
    const ASPECT_RATIO_EPSILON: f64 = 0.005;

    if width == 0 || height == 0 {
        return (width, height);
    }

    let Some(source_ratio) = source_aspect_ratio
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .map(f64::from)
    else {
        return (width, height);
    };

    let preview_ratio = width as f64 / height as f64;
    let relative_ratio_error = (preview_ratio / source_ratio - 1.0).abs();
    if !relative_ratio_error.is_finite() || relative_ratio_error <= ASPECT_RATIO_EPSILON {
        return (width, height);
    }

    // Preserve the shorter side; expand the other dimension to the source
    // aspect ratio. This gives 228x128 from a 128x128 preview at 16:9.
    let short_side = width.min(height).max(1) as f64;
    let (mut target_width, mut target_height) = if source_ratio >= 1.0 {
        ((short_side * source_ratio).round().max(1.0), short_side)
    } else {
        (short_side, (short_side / source_ratio).round().max(1.0))
    };

    // Bound allocations for unusually wide/tall sources. Typical photos are
    // unaffected; integer rounding keeps the resulting ratio close to source.
    let max_side_scale = (MAX_NORMALIZED_SIDE as f64 / target_width.max(target_height)).min(1.0);
    let pixel_count = target_width * target_height;
    let max_pixel_scale = if pixel_count > MAX_NORMALIZED_PIXELS as f64 {
        (MAX_NORMALIZED_PIXELS as f64 / pixel_count).sqrt()
    } else {
        1.0
    };
    let scale = max_side_scale.min(max_pixel_scale).min(1.0);
    target_width = (target_width * scale).round().max(1.0);
    target_height = (target_height * scale).round().max(1.0);

    (target_width as u32, target_height as u32)
}

/// Normalize a preview's pixel dimensions to the original image's aspect ratio.
/// Kept as a small unit-test seam for the pixel-repainting operation.
#[cfg(test)]
fn normalize_thumbnail_aspect_ratio(
    rgba: image::RgbaImage,
    source_aspect_ratio: Option<f32>,
) -> image::RgbaImage {
    let (target_width, target_height) =
        aspect_corrected_dimensions(rgba.width(), rgba.height(), source_aspect_ratio);
    if (target_width, target_height) == rgba.dimensions() {
        return rgba;
    }
    image::imageops::resize(
        &rgba,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    )
}

/// Fit the provider's original pixels to the requested maximum dimension,
/// then repaint them into a rectangle matching the source image's aspect ratio.
/// The aspect correction happens in the same resize pass, so a native preview
/// is not needlessly resampled twice.
fn fit_thumbnail_to(
    rgba: image::RgbaImage,
    max_side: u32,
    source_aspect_ratio: Option<f32>,
) -> Result<(usize, usize, Vec<u8>, u32), String> {
    let source_width = rgba.width();
    let source_height = rgba.height();
    let max_side = max_side.max(1);
    if source_width == 0 || source_height == 0 {
        return Err("Empty image".into());
    }

    // Fit first, before correcting the ratio. This is deliberate: requested
    // resolution constrains the real native preview, while ratio correction may
    // expand its long dimension (128x128 -> 228x128 for a 16:9 source).
    let scale = (max_side as f64 / source_width as f64)
        .min(max_side as f64 / source_height as f64)
        .min(1.0);
    let fitted_width = ((source_width as f64 * scale).round() as u32).max(1);
    let fitted_height = ((source_height as f64 * scale).round() as u32).max(1);
    let effective_resolution = fitted_width.max(fitted_height);
    let (target_width, target_height) =
        aspect_corrected_dimensions(fitted_width, fitted_height, source_aspect_ratio);

    // Triangle is a fast filter for the single resize, whether it is a simple
    // downscale or the small stretch needed to correct a desktop preview ratio.
    let thumbnail = if (target_width, target_height) == rgba.dimensions() {
        rgba
    } else {
        image::imageops::resize(
            &rgba,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        )
    };
    Ok((
        thumbnail.width() as usize,
        thumbnail.height() as usize,
        thumbnail.into_raw(),
        effective_resolution,
    ))
}

/// Prefer a standard EXIF-embedded JPEG thumbnail before decoding the full source.
/// This is common in JPEGs and some TIFF-based camera files.
fn extract_embedded_metadata_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    // kamadak-exif exposes the thumbnail IFD's offset and length tags, but does
    // not provide an `Exif::thumbnail()` byte-slice accessor. Locate the TIFF
    // header inside the EXIF APP1 payload, then use those tags to extract bytes.
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
    let offset = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    let length = exif
        .get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    if length == 0 {
        return None;
    }

    // EXIF offsets are relative to the start of the TIFF header, not the JPEG.
    let exif_marker = b"Exif\0\0";
    let marker_start = bytes
        .windows(exif_marker.len())
        .position(|w| w == exif_marker)?;
    let tiff_start = marker_start.checked_add(exif_marker.len())?;
    let start = tiff_start.checked_add(offset)?;
    let end = start.checked_add(length)?;
    Some(bytes.get(start..end)?.to_vec())
}

fn decode_checked_sidebar_image(
    bytes: &[u8],
    name: &str,
    is_raw: bool,
    source_path: Option<&std::path::Path>,
    source_dimensions: Option<(u32, u32)>,
) -> Result<image::RgbaImage, String> {
    if is_raw {
        let thumbnail = crate::decoder::extract_raw_thumbnail(bytes, source_path)
            .ok_or_else(|| format!("No embedded RAW thumbnail found in {name}"))?;
        let image = image::load_from_memory(&thumbnail).map_err(|error| {
            format!("Failed to decode embedded RAW thumbnail in {name}: {error}")
        })?;
        return Ok(image.to_rgba8());
    }

    let format = crate::decoder::format_detection::detect_format(bytes)
        .ok_or_else(|| format!("Unknown image format: {name}"))?;
    let registry = crate::decoder::default_registry();
    // Prefer dimensions already measured by the dedicated metadata workers.
    // Keep the format-registry probe as a safety fallback for unknown headers.
    let (width, height) = source_dimensions
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(Ok)
        .unwrap_or_else(|| {
            registry
                .dimensions(bytes, format)
                .map_err(|error| error.to_string())
        })?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| format!("Image dimensions overflow: {name}"))?;
    let rgba_bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| format!("Decoded image size overflow: {name}"))?;
    if width == 0 || height == 0 || pixels > MAX_THUMB_SOURCE_PIXELS {
        return Err(format!(
            "Image exceeds thumbnail decode safety limit ({width}x{height}): {name}"
        ));
    }
    // Guard the allocation size explicitly before invoking the full decoder.
    if rgba_bytes > MAX_THUMB_SOURCE_PIXELS * 4 {
        return Err(format!(
            "Decoded image would exceed thumbnail memory limit: {name}"
        ));
    }
    let decoded = registry
        .decode(bytes, format, &crate::imaging::core::DecodeOptions)
        .map_err(|e| e.to_string())?;
    image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.into_data())
        .ok_or_else(|| format!("Invalid decoded image data: {name}"))
}

fn thumbnail_cache_size(cache_key: &str, source_key: &str) -> Option<u32> {
    let mut parts = cache_key.splitn(3, ':');
    if parts.next()? != "thumb" {
        return None;
    }
    let size = parts.next()?.parse::<u32>().ok()?;
    (parts.next()? == source_key).then_some(size)
}

fn thumbnail_source_key_from_cache_key(cache_key: &str) -> Option<&str> {
    let mut parts = cache_key.splitn(3, ':');
    (parts.next()? == "thumb").then_some(())?;
    parts.next()?;
    parts.next()
}

fn remove_attempts_for_cached_source(attempted: &mut HashSet<String>, cache_key: &str) {
    let Some(source_key) = thumbnail_source_key_from_cache_key(cache_key) else {
        attempted.remove(cache_key);
        return;
    };
    attempted.retain(|attempted_key| {
        thumbnail_source_key_from_cache_key(attempted_key) != Some(source_key)
    });
}

impl ViewerApp {
    /// Queue a cheap source-dimension probe only when a filesystem image enters
    /// the visible Masonry viewport. Probing the entire filtered collection before
    /// requesting thumbnails made large folders wait for off-screen metadata work.
    /// Archive entries intentionally skip this pass: their thumbnail pipeline
    /// already reads/decompresses the member and obtains dimensions from those bytes.
    pub(crate) fn request_masonry_dimensions(
        &mut self,
        entry: &crate::imaging::entry::ImageEntry,
        ctx: &egui::Context,
    ) {
        let source_key = source_thumbnail_key(entry);
        let entry_id = entry.get_id();

        // Reuse dimensions already attached to this tab before consulting the
        // shared caches. This avoids probing again when a tab is restored or
        // when another thumbnail pipeline already populated its layout data.
        let tab_dimensions = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .and_then(|tab| tab.masonry_dimensions.get(&entry_id).copied())
            .filter(|(width, height)| *width > 0 && *height > 0);
        if let Some((width, height)) = tab_dimensions {
            self.masonry_dimension_results
                .insert(source_key.clone(), Some((width, height)));
            self.masonry_aspect_ratio_results.remove(&source_key);
            self.masonry_dimension_attempted.insert(source_key);
            return;
        }

        // Exact source dimensions win. If a provider only supplied a preview
        // ratio, apply that immediately as a fallback while the filesystem
        // dimension probe continues in the background.
        if let Some(Some((width, height))) =
            self.masonry_dimension_results.get(&source_key).copied()
        {
            if width > 0 && height > 0 {
                let mut changed = false;
                if let Some(tab) = self.workspace.tabs.get_mut(self.workspace.active_index) {
                    let ratio = width as f32 / height as f32;
                    changed = tab.masonry_dimensions.get(&entry_id) != Some(&(width, height))
                        || tab.masonry_aspect_ratios.get(&entry_id) != Some(&ratio);
                    tab.masonry_dimensions
                        .insert(entry_id.clone(), (width, height));
                    tab.masonry_aspect_ratios.insert(entry_id, ratio);
                }
                if changed {
                    // The layout pass already ran before visible cards consume
                    // this lazy cache entry. Schedule one more pass so the new
                    // ratio affects shortest-column placement immediately.
                    ctx.request_repaint();
                }
            }
            self.masonry_dimension_attempted.insert(source_key);
            return;
        }
        if let Some(aspect_ratio) = self.masonry_aspect_ratio_results.get(&source_key).copied()
            && aspect_ratio.is_finite()
            && aspect_ratio > 0.0
        {
            let mut changed = false;
            if let Some(tab) = self.workspace.tabs.get_mut(self.workspace.active_index) {
                changed = tab.masonry_aspect_ratios.get(&entry_id) != Some(&aspect_ratio);
                tab.masonry_aspect_ratios.insert(entry_id, aspect_ratio);
            }
            if changed {
                ctx.request_repaint();
            }
        }

        // Archive thumbnail generation already extracts its member once and
        // reports source dimensions/aspect ratio from that same byte buffer.
        // Only ordinary files need the independent header/EXIF probe.
        if !matches!(entry, crate::imaging::entry::ImageEntry::File(_)) {
            return;
        }
        if self.masonry_dimension_attempted.contains(&source_key)
            || self.masonry_dimension_pending.contains(&source_key)
        {
            return;
        }

        // Enqueue only visible entries (the caller guarantees viewport
        // intersection) and let the existing Dimensions pool enforce the
        // configured worker limit. The old four-job cap serialized the first
        // viewport unnecessarily on fast SSDs; this queue remains bounded by
        // the visible cards, not by the total folder size.
        let worker_limit = self.settings_manager.get().thumbnail_workers.clamp(1, 8);

        let (tx, rx) = std::sync::mpsc::channel();
        let worker_entry = entry.clone();
        let worker_ctx = ctx.clone();
        let started_at = Arc::new(Mutex::new(None));
        let worker_started_at = Arc::clone(&started_at);
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let scheduler_key = format!("masonry-dimensions:{source_key}");
        let scheduler = Arc::clone(thumbnail_scheduler(worker_limit));
        let worker_scheduler = Arc::clone(&scheduler);
        let worker_scheduler_key = scheduler_key.clone();
        let job = Box::new(move || {
            if worker_cancellation.load(Ordering::Acquire) {
                return;
            }
            mark_thumbnail_started(&worker_started_at);
            let dimensions = source_dimensions_from_entry(&worker_entry);
            if !worker_cancellation.load(Ordering::Acquire) {
                let _ = tx.send(dimensions);
                worker_ctx.request_repaint();
            }
            // Dimension probes are single-stage jobs. Remove their visibility
            // registration immediately on success instead of retaining one
            // registry entry per image until it scrolls off-screen.
            worker_scheduler.finish_masonry(&worker_scheduler_key, &worker_cancellation);
        }) as ThumbnailJob;

        if scheduler.submit_masonry(
            ThumbnailPool::Dimensions,
            scheduler_key,
            MASONRY_DIMENSION_PRIORITY,
            source_key.clone(),
            Arc::clone(&cancellation),
            false, // Dimension discovery must not hold native thumbnail workers.
            job,
        ) {
            self.masonry_dimension_attempted.insert(source_key.clone());
            self.masonry_dimension_pending.insert(source_key.clone());
            self.masonry_dimension_receivers.push((
                source_key,
                true, // If this item scrolls away, it can be retried when visible again.
                Instant::now(),
                started_at,
                cancellation,
                rx,
            ));
        }
    }

    /// Collect completed visible-item dimension probes into the shared cache.
    /// Workspace tabs apply these results lazily when their cards are visible,
    /// avoiding a full scan of every open tab for each completed source.
    pub(crate) fn collect_masonry_dimensions(&mut self) {
        let mut remaining = Vec::with_capacity(self.masonry_dimension_receivers.len());
        let mut completed = Vec::new();
        let mut visibility_cancelled = Vec::new();
        for (
            source_key,
            retry_on_visibility_cancel,
            queued_at,
            started_at,
            cancellation,
            receiver,
        ) in self.masonry_dimension_receivers.drain(..)
        {
            // A completed dimensions result is useful even if the sidebar
            // scrolled away in the same frame. Only discard a probe when it
            // was cancelled before producing a result.
            match receiver.try_recv() {
                Ok(dimensions) => {
                    completed.push((source_key, dimensions));
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if cancellation.load(Ordering::Acquire) && retry_on_visibility_cancel {
                        visibility_cancelled.push(source_key);
                    } else {
                        // A worker panic or unexpected task exit is a completed
                        // failure, not a reason to enqueue the same broken probe
                        // again on every UI frame.
                        completed.push((source_key, None));
                    }
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }

            if cancellation.load(Ordering::Acquire) {
                if retry_on_visibility_cancel {
                    // Sidebar entries can leave the viewport while their probe
                    // is queued/running. Do not cache that cancellation as a
                    // permanent dimensions failure; the next visible request
                    // is allowed to try again.
                    visibility_cancelled.push(source_key);
                } else {
                    completed.push((source_key, None));
                }
                continue;
            }

            let timed_out = thumbnail_started_elapsed(&started_at)
                .map(|elapsed| elapsed >= DIMENSION_JOB_TIMEOUT)
                .unwrap_or_else(|| queued_at.elapsed() >= DIMENSION_QUEUE_TIMEOUT);
            if timed_out {
                if let Some(scheduler) = THUMBNAIL_SCHEDULER.get() {
                    scheduler.cancel_job(&cancellation);
                } else {
                    cancellation.store(true, Ordering::Release);
                }
                eprintln!(
                    "[thumbnail] dimension probe timed out for {source_key}; using the fallback Masonry aspect ratio"
                );
                completed.push((source_key, None));
                continue;
            }
            remaining.push((
                source_key,
                retry_on_visibility_cancel,
                queued_at,
                started_at,
                cancellation,
                receiver,
            ));
        }
        self.masonry_dimension_receivers = remaining;

        for source_key in visibility_cancelled {
            self.masonry_dimension_pending.remove(&source_key);
            self.masonry_dimension_attempted.remove(&source_key);
        }

        for (source_key, dimensions) in completed {
            self.masonry_dimension_pending.remove(&source_key);
            if let Some(dimensions) = dimensions.filter(|(width, height)| *width > 0 && *height > 0)
            {
                self.masonry_dimension_results
                    .insert(source_key.clone(), Some(dimensions));
                self.masonry_aspect_ratio_results.remove(&source_key);
            } else {
                // A parallel thumbnail decode may already have discovered valid
                // dimensions from its source bytes. Never replace those with a
                // failed/empty header probe result.
                self.masonry_dimension_results
                    .entry(source_key)
                    .or_insert(None);
            }
            // Do not scan every open tab's complete entry list for each result.
            // The active Masonry card applies this shared result lazily when it
            // next becomes visible; other tabs do the same when activated.
        }
    }

    pub(crate) fn request_sidebar_thumbnail(
        &mut self,
        path: PathBuf,
        key: String,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cache_key = shared_thumbnail_key(&key, target_size);
        let source_dimensions = self.cached_source_dimensions(&key);
        self.request_sized_sidebar_thumbnail(
            path,
            cache_key,
            target_size,
            source_dimensions,
            SIDEBAR_THUMBNAIL_PRIORITY,
            ctx,
            false,
            false,
            false,
        );
    }

    /// Larger visible sidebar entries use a quick OS placeholder first, then
    /// request a higher-resolution replacement after the preview reaches cache.
    /// Small cards request their final size directly so the normal quality floor
    /// can reject an unhelpfully tiny native preview.
    pub(crate) fn request_visible_sidebar_thumbnail(
        &mut self,
        path: PathBuf,
        key: String,
        target_size: u32,
        priority: u8,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cached_resolution = self.best_shared_thumbnail_resolution(&key).unwrap_or(0);
        let source_dimensions = self.cached_source_dimensions(&key);

        // A small card has no meaningful second stage. Ask the native provider
        // for its actual display size and keep the normal quality threshold so
        // an extremely small cached OS preview cannot become the final result.
        if target_size <= SIDEBAR_PLACEHOLDER_SIZE {
            if cached_resolution < target_size {
                self.request_sized_sidebar_thumbnail(
                    path,
                    shared_thumbnail_key(&key, target_size),
                    target_size,
                    source_dimensions,
                    priority,
                    ctx,
                    false,
                    false,
                    true,
                );
            }
            return;
        }

        let placeholder_size = SIDEBAR_PLACEHOLDER_SIZE;
        let placeholder_key = shared_thumbnail_key(&key, placeholder_size);
        let placeholder_failed = self.shared_thumbnail_failed(&key, placeholder_size);
        let placeholder_pending = self.sidebar_thumbnail_pending.contains(&placeholder_key);
        let placeholder_completed_with_preview =
            self.sidebar_thumbnail_attempted.contains(&placeholder_key)
                && !placeholder_failed
                && !placeholder_pending
                && cached_resolution > 0;
        if cached_resolution < placeholder_size
            && !placeholder_completed_with_preview
            && !placeholder_failed
        {
            self.request_sized_sidebar_thumbnail(
                path,
                shared_thumbnail_key(&key, placeholder_size),
                placeholder_size,
                source_dimensions,
                priority,
                ctx,
                false,
                true,
                true,
            );
        } else if cached_resolution < target_size {
            self.request_sized_sidebar_thumbnail(
                path,
                shared_thumbnail_key(&key, target_size),
                target_size,
                source_dimensions,
                priority.saturating_sub(2),
                ctx,
                false,
                false,
                true,
            );
        }
    }

    /// Start low-priority work one row outside the viewport. It is tracked just
    /// like visible work and is cancelled as soon as it leaves the prefetch set.
    pub(crate) fn prefetch_sidebar_entry_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let key = source_thumbnail_key(&entry);
        self.sidebar_prefetch_sources.insert(key.clone());
        match entry {
            crate::imaging::entry::ImageEntry::File(path) => {
                self.request_visible_sidebar_thumbnail(
                    path,
                    key,
                    target_size.min(SIDEBAR_PLACEHOLDER_SIZE),
                    SIDEBAR_PREFETCH_PRIORITY,
                    ctx,
                );
            }
            archive_entry => {
                self.request_visible_sidebar_archive_thumbnail(
                    archive_entry,
                    key,
                    target_size.min(SIDEBAR_PLACEHOLDER_SIZE),
                    SIDEBAR_PREFETCH_PRIORITY,
                    ctx,
                );
            }
        }
    }

    fn cached_source_dimensions(&self, source_key: &str) -> Option<(u32, u32)> {
        self.masonry_dimension_results
            .get(source_key)
            .copied()
            .flatten()
    }

    /// Resolve the best cached resolution for a shared key. Prefer the smallest
    /// texture at or above the requested size, then fall back to the largest
    /// lower-resolution preview while an upgrade is generated.
    pub(crate) fn get_shared_thumbnail(
        &mut self,
        key: &str,
        requested_size: u32,
    ) -> Option<egui::TextureHandle> {
        let requested_size = requested_size.max(1);
        let mut available = self
            .sidebar_thumbnails
            .iter()
            .filter_map(|(cache_key, _)| thumbnail_cache_size(cache_key, key))
            .collect::<Vec<_>>();
        available.sort_unstable();
        available.dedup();

        let selected_size = available
            .iter()
            .copied()
            .find(|size| *size >= requested_size)
            .or_else(|| {
                available
                    .iter()
                    .copied()
                    .filter(|size| *size < requested_size)
                    .max()
            });
        if let Some(size) = selected_size {
            let cache_key = shared_thumbnail_key(key, size);
            if let Some(texture) = self.sidebar_thumbnails.get(&cache_key).cloned() {
                return Some(texture);
            }
        }

        // Read older unqualified cache keys during an in-session transition.
        self.sidebar_thumbnails.get(key).cloned()
    }

    pub(crate) fn best_shared_thumbnail_resolution(&self, key: &str) -> Option<u32> {
        self.sidebar_thumbnails
            .iter()
            .filter_map(|(cache_key, _)| thumbnail_cache_size(cache_key, key))
            .max()
    }

    pub(crate) fn has_shared_thumbnail_at_least(&mut self, key: &str, requested_size: u32) -> bool {
        let requested_size = requested_size.max(1);

        self.sidebar_thumbnails.iter().any(|(cache_key, _)| {
            thumbnail_cache_size(cache_key, key).is_some_and(|size| size >= requested_size)
        })
    }

    pub(crate) fn shared_thumbnail_failed(&self, key: &str, requested_size: u32) -> bool {
        self.sidebar_thumbnail_failed
            .contains(&shared_thumbnail_key(key, requested_size))
            || self.sidebar_thumbnail_failed.contains(key)
    }

    /// Request a resolution-appropriate thumbnail for Masonry. The size is part
    /// of the shared cache key so Masonry can request upgrades while Browse,
    /// Queue, and Favorites reuse the same cached texture.
    pub(crate) fn request_masonry_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        target_size: u32,
        source_dimensions: Option<(u32, u32)>,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cache_source_key = source_thumbnail_key(&entry);
        let cache_key = shared_thumbnail_key(&cache_source_key, target_size);
        let priority = if target_size <= MASONRY_PLACEHOLDER_SIZE {
            MASONRY_PLACEHOLDER_PRIORITY
        } else {
            MASONRY_THUMBNAIL_PRIORITY
        };
        match entry {
            crate::imaging::entry::ImageEntry::File(path) => {
                self.request_sized_sidebar_thumbnail(
                    path,
                    cache_key,
                    target_size,
                    source_dimensions,
                    priority,
                    ctx,
                    true,
                    priority == MASONRY_PLACEHOLDER_PRIORITY,
                    false,
                );
            }
            archive_entry => {
                self.request_sized_archive_thumbnail(
                    archive_entry,
                    cache_key,
                    target_size,
                    priority,
                    ctx,
                    Some(cache_source_key),
                    false,
                );
            }
        }
    }

    fn request_sized_sidebar_thumbnail(
        &mut self,
        path: PathBuf,
        key: String,
        target_size: u32,
        precomputed_source_dimensions: Option<(u32, u32)>,
        priority: u8,
        ctx: &egui::Context,
        masonry_only: bool,
        placeholder_request: bool,
        sidebar_managed: bool,
    ) {
        if !path.is_file() {
            return;
        }
        if self.sidebar_thumbnail_pending.contains(&key) {
            if priority > SIDEBAR_THUMBNAIL_PRIORITY {
                if let Some((_, _, _, queued_priority, _, _, _, _)) = self
                    .sidebar_thumbnail_receivers
                    .iter()
                    .find(|(pending_key, ..)| pending_key == &key)
                {
                    queued_priority.fetch_max(priority, Ordering::AcqRel);
                }
                thumbnail_scheduler(self.settings_manager.get().thumbnail_workers)
                    .promote(&key, priority);
            }
            return;
        }
        if self.sidebar_thumbnail_attempted.contains(&key) {
            return;
        }

        self.sidebar_thumbnail_failed.remove(&key);
        self.sidebar_thumbnail_attempted.insert(key.clone());
        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        let masonry_placeholder = placeholder_request;
        let masonry_source_key = if masonry_only {
            thumbnail_source_key_from_cache_key(&key).map(str::to_owned)
        } else {
            None
        };
        let receiver_masonry_source = masonry_source_key.clone();
        let started_at = Arc::new(Mutex::new(None));
        let cancellation = Arc::new(AtomicBool::new(false));
        let priority_token = Arc::new(AtomicU8::new(priority));
        self.sidebar_thumbnail_receivers.push((
            key.clone(),
            receiver_masonry_source,
            sidebar_managed,
            Arc::clone(&priority_token),
            Instant::now(),
            Arc::clone(&started_at),
            Arc::clone(&cancellation),
            rx,
        ));

        let scheduler = Arc::clone(thumbnail_scheduler(
            self.settings_manager.get().thumbnail_workers,
        ));
        let repaint_ctx = ctx.clone();
        let job_key = key.clone();
        let scheduler_key = key.clone();
        let source_key_for_job = masonry_source_key.clone();

        // Reuse Masonry/previous-sidebar metadata before submitting another
        // probe. Sidebar probes are just-in-time and non-blocking for native
        // requests belonging to other visible files.
        let cache_source_key = thumbnail_source_key_from_cache_key(&key).map(str::to_owned);
        let dimensions_already_attempted = cache_source_key
            .as_ref()
            .is_some_and(|source_key| self.masonry_dimension_attempted.contains(source_key));
        let precomputed_source_dimensions = precomputed_source_dimensions.or_else(|| {
            cache_source_key
                .as_deref()
                .and_then(|source_key| self.cached_source_dimensions(source_key))
        });
        let first_stage = if precomputed_source_dimensions.is_none()
            && !dimensions_already_attempted
            && !masonry_only
        {
            let dimension_scheduler = Arc::clone(&scheduler);
            let dimension_cancellation = Arc::clone(&cancellation);
            // Keep a separate handle for queue submission; the job closure owns
            // dimension_cancellation.
            let dimension_submit_cancellation = Arc::clone(&dimension_cancellation);
            let dimension_path = path.clone();
            let dimension_key = scheduler_key.clone();
            let dimension_source_key = source_key_for_job.clone();
            let native_scheduler = Arc::clone(&scheduler);
            let native_cancellation = Arc::clone(&cancellation);
            let native_path = path.clone();
            let native_key = scheduler_key.clone();
            let native_source_key = source_key_for_job.clone();
            let native_tx = tx.clone();
            let native_repaint = repaint_ctx.clone();
            let dimension_started_at = Arc::clone(&started_at);
            let dimension_priority_token = Arc::clone(&priority_token);
            let (dimension_tx, dimension_rx) = std::sync::mpsc::channel();
            let dimension_repaint = repaint_ctx.clone();
            let dimension_job = Box::new(move || {
                if dimension_cancellation.load(Ordering::Acquire) {
                    return;
                }
                mark_thumbnail_started(&dimension_started_at);
                let dimensions = source_dimensions_from_file(&dimension_path);
                if dimension_cancellation.load(Ordering::Acquire) {
                    return;
                }
                let _ = dimension_tx.send(dimensions);
                dimension_repaint.request_repaint();
                let submitted = schedule_file_native_stage(
                    Arc::clone(&native_scheduler),
                    native_path,
                    native_key.clone(),
                    target_size,
                    dimensions,
                    Arc::clone(&dimension_priority_token),
                    masonry_placeholder,
                    masonry_only,
                    native_source_key,
                    Arc::clone(&native_cancellation),
                    native_tx.clone(),
                    native_repaint.clone(),
                    false,
                    Arc::clone(&dimension_started_at),
                );
                if !submitted {
                    complete_thumbnail_request(
                        native_key,
                        Err("Native OS/DE thumbnail request could not be queued".into()),
                        native_tx,
                        native_repaint,
                        &native_scheduler,
                        &native_cancellation,
                        masonry_only,
                    );
                }
            }) as ThumbnailJob;
            let submitted = submit_pipeline_stage(
                &dimension_scheduler,
                ThumbnailPool::Dimensions,
                &dimension_key,
                sidebar_dimension_priority(priority),
                dimension_source_key.as_deref(),
                &dimension_submit_cancellation,
                true,
                false,
                dimension_job,
            );
            if submitted && let Some(source_key) = cache_source_key.clone() {
                self.masonry_dimension_attempted.insert(source_key.clone());
                self.masonry_dimension_pending.insert(source_key.clone());
                self.masonry_dimension_receivers.push((
                    source_key,
                    true,
                    Instant::now(),
                    Arc::clone(&started_at),
                    Arc::clone(&cancellation),
                    dimension_rx,
                ));
            }
            submitted
        } else {
            schedule_file_native_stage(
                Arc::clone(&scheduler),
                path,
                scheduler_key,
                target_size,
                precomputed_source_dimensions,
                priority_token,
                masonry_placeholder,
                masonry_only,
                source_key_for_job,
                Arc::clone(&cancellation),
                tx,
                repaint_ctx,
                true,
                Arc::clone(&started_at),
            )
        };

        if !first_stage {
            if masonry_only {
                scheduler.finish_masonry(&job_key, &cancellation);
            }
            self.sidebar_thumbnail_pending.remove(&job_key);
            self.sidebar_thumbnail_attempted.remove(&job_key);
            self.sidebar_thumbnail_receivers.pop();
        }
    }

    #[allow(unused_assignments)]
    pub(crate) fn request_sidebar_archive_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        key: String,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cache_key = shared_thumbnail_key(&key, target_size);
        self.request_sized_archive_thumbnail(
            entry,
            cache_key,
            target_size,
            SIDEBAR_THUMBNAIL_PRIORITY,
            ctx,
            None,
            false,
        );
    }

    pub(crate) fn request_visible_sidebar_archive_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        key: String,
        target_size: u32,
        priority: u8,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let placeholder_size = target_size.min(SIDEBAR_PLACEHOLDER_SIZE).max(1);
        let cached_resolution = self.best_shared_thumbnail_resolution(&key).unwrap_or(0);
        let placeholder_key = shared_thumbnail_key(&key, placeholder_size);
        let placeholder_failed = self.shared_thumbnail_failed(&key, placeholder_size);
        let placeholder_pending = self.sidebar_thumbnail_pending.contains(&placeholder_key);
        let placeholder_completed_with_preview =
            self.sidebar_thumbnail_attempted.contains(&placeholder_key)
                && !placeholder_failed
                && !placeholder_pending
                && cached_resolution > 0;
        if cached_resolution < placeholder_size
            && !placeholder_completed_with_preview
            && !placeholder_failed
        {
            self.request_sized_archive_thumbnail(
                entry,
                shared_thumbnail_key(&key, placeholder_size),
                placeholder_size,
                priority,
                ctx,
                None,
                true,
            );
        } else if cached_resolution < target_size {
            self.request_sized_archive_thumbnail(
                entry,
                shared_thumbnail_key(&key, target_size),
                target_size,
                priority.saturating_sub(2),
                ctx,
                None,
                true,
            );
        }
    }

    fn request_sized_archive_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        key: String,
        target_size: u32,
        priority: u8,
        ctx: &egui::Context,
        masonry_source_key: Option<String>,
        sidebar_managed: bool,
    ) {
        if self.sidebar_thumbnail_pending.contains(&key) {
            if priority > SIDEBAR_THUMBNAIL_PRIORITY {
                if let Some((_, _, _, queued_priority, _, _, _, _)) = self
                    .sidebar_thumbnail_receivers
                    .iter()
                    .find(|(pending_key, ..)| pending_key == &key)
                {
                    queued_priority.fetch_max(priority, Ordering::AcqRel);
                }
                thumbnail_scheduler(self.settings_manager.get().thumbnail_workers)
                    .promote(&key, priority);
            }
            return;
        }
        if self.sidebar_thumbnail_attempted.contains(&key) {
            return;
        }
        self.sidebar_thumbnail_failed.remove(&key);
        self.sidebar_thumbnail_attempted.insert(key.clone());
        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        let masonry_only = masonry_source_key.is_some();
        let started_at = Arc::new(Mutex::new(None));
        let cancellation = Arc::new(AtomicBool::new(false));
        let priority_token = Arc::new(AtomicU8::new(priority));
        self.sidebar_thumbnail_receivers.push((
            key.clone(),
            masonry_source_key.clone(),
            sidebar_managed,
            Arc::clone(&priority_token),
            Instant::now(),
            Arc::clone(&started_at),
            Arc::clone(&cancellation),
            rx,
        ));

        let scheduler = Arc::clone(thumbnail_scheduler(
            self.settings_manager.get().thumbnail_workers,
        ));
        let worker_cancellation = Arc::clone(&cancellation);
        let worker_scheduler = Arc::clone(&scheduler);
        let worker_source_key = masonry_source_key.clone();
        let scheduler_key = key.clone();
        let repaint_ctx = ctx.clone();
        let dimension_started_at = Arc::clone(&started_at);
        let dimension_priority_token = Arc::clone(&priority_token);
        let (dimension_tx, dimension_rx) = std::sync::mpsc::channel();
        let dimension_repaint = repaint_ctx.clone();
        let dimension_job = Box::new(move || {
            if worker_cancellation.load(Ordering::Acquire) {
                return;
            }
            mark_thumbnail_started(&dimension_started_at);
            let bytes = read_archive_entry_bytes(&entry);
            let source_dimensions = bytes
                .as_ref()
                .ok()
                .and_then(|bytes| source_dimensions_from_bytes(bytes));
            let _ = dimension_tx.send(source_dimensions);
            dimension_repaint.request_repaint();
            if worker_cancellation.load(Ordering::Acquire) {
                return;
            }

            let decoder_scheduler = Arc::clone(&worker_scheduler);
            let decoder_cancellation = Arc::clone(&worker_cancellation);
            // The decoder closure owns decoder_cancellation; retain a second
            // handle for submitting the cancellable queued job.
            let decoder_submit_cancellation = Arc::clone(&decoder_cancellation);
            let decoder_source_key = worker_source_key.clone();
            let decoder_key = scheduler_key.clone();
            let decoder_repaint = repaint_ctx.clone();
            let decoder_tx = tx.clone();
            let decoder_priority_token = Arc::clone(&dimension_priority_token);
            let decoder_job = Box::new(move || {
                let result = (|| -> Result<GeneratedThumbnail, String> {
                    if decoder_cancellation.load(Ordering::Acquire) {
                        return Err("__masonry_thumbnail_cancelled__".into());
                    }
                    let bytes = bytes?;
                    let name = match &entry {
                        crate::imaging::entry::ImageEntry::File(path) => path.display().to_string(),
                        crate::imaging::entry::ImageEntry::Zip(item) => item.name.clone(),
                        crate::imaging::entry::ImageEntry::S7z(item) => item.name.clone(),
                        crate::imaging::entry::ImageEntry::Rar(item) => item.name.clone(),
                    };
                    let is_raw = std::path::Path::new(&name)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .and_then(crate::imaging::core::ImageFormat::from_extension)
                        == Some(crate::imaging::core::ImageFormat::CameraRaw)
                        || zenraw::is_raw_file(&bytes);
                    let source_aspect_ratio =
                        source_dimensions.map(|(width, height)| width as f32 / height as f32);
                    if decoder_cancellation.load(Ordering::Acquire) {
                        return Err("__masonry_thumbnail_cancelled__".into());
                    }
                    let rgba = decode_checked_sidebar_image(
                        &bytes,
                        &name,
                        is_raw,
                        None,
                        source_dimensions,
                    )?;
                    let source_aspect_ratio = source_aspect_ratio_or_preview(
                        source_aspect_ratio,
                        rgba.width(),
                        rgba.height(),
                    );
                    let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                    let (width, height, pixels, resolution) =
                        fit_thumbnail_to(rgba, output_limit, source_aspect_ratio)?;
                    Ok(GeneratedThumbnail {
                        width,
                        height,
                        pixels,
                        resolution,
                        source_aspect_ratio,
                        source_dimensions,
                    })
                })();
                complete_thumbnail_request(
                    decoder_key,
                    result,
                    decoder_tx,
                    decoder_repaint,
                    &decoder_scheduler,
                    &decoder_cancellation,
                    masonry_only,
                );
            }) as ThumbnailJob;

            let submitted = worker_scheduler
                .submit_stage(
                    ThumbnailPool::Decoder,
                    scheduler_key.clone(),
                    decoder_priority_token.load(Ordering::Acquire),
                    decoder_source_key,
                    Some(decoder_submit_cancellation),
                    false,
                    decoder_job,
                )
                .is_ok();
            if !submitted {
                complete_thumbnail_request(
                    scheduler_key,
                    Err(
                        "Jpeg-Viewer archive decoder request was cancelled before it could start"
                            .into(),
                    ),
                    tx.clone(),
                    repaint_ctx.clone(),
                    &worker_scheduler,
                    &worker_cancellation,
                    masonry_only,
                );
            }
        }) as ThumbnailJob;

        let submitted = submit_pipeline_stage(
            &scheduler,
            ThumbnailPool::Dimensions,
            &key,
            if masonry_only {
                MASONRY_DIMENSION_PRIORITY
            } else {
                sidebar_dimension_priority(priority)
            },
            masonry_source_key.as_deref(),
            &cancellation,
            true,
            false,
            dimension_job,
        );
        if submitted
            && sidebar_managed
            && let Some(source_key) = thumbnail_source_key_from_cache_key(&key).map(str::to_owned)
        {
            self.masonry_dimension_attempted.insert(source_key.clone());
            self.masonry_dimension_pending.insert(source_key.clone());
            self.masonry_dimension_receivers.push((
                source_key,
                true,
                Instant::now(),
                Arc::clone(&started_at),
                Arc::clone(&cancellation),
                dimension_rx,
            ));
        }
        if !submitted {
            if masonry_only {
                scheduler.finish_masonry(&key, &cancellation);
            }
            self.sidebar_thumbnail_pending.remove(&key);
            self.sidebar_thumbnail_attempted.remove(&key);
            self.sidebar_thumbnail_receivers.pop();
        }
    }
    /// Cancel sidebar thumbnail requests that have fallen outside both the
    /// rendered viewport and the small directional prefetch window. Results
    /// already present in the shared cache are deliberately retained.
    pub(crate) fn finish_sidebar_thumbnail_frame(&mut self) {
        let mut relevant = self.sidebar_visible_sources.clone();
        relevant.extend(self.sidebar_prefetch_sources.iter().cloned());
        let Some(scheduler) = THUMBNAIL_SCHEDULER.get().cloned() else {
            for (_, _, sidebar_managed, _, _, _, cancellation, _) in
                &self.sidebar_thumbnail_receivers
            {
                if *sidebar_managed {
                    cancellation.store(true, Ordering::Release);
                }
            }
            return;
        };

        for (job_key, _, sidebar_managed, _, _, _, cancellation, _) in
            &self.sidebar_thumbnail_receivers
        {
            if !sidebar_managed {
                continue;
            }
            let still_relevant = thumbnail_source_key_from_cache_key(job_key)
                .is_some_and(|source_key| relevant.contains(source_key));
            if !still_relevant && !cancellation.load(Ordering::Acquire) {
                scheduler.cancel_job(cancellation);
            }
        }
    }

    pub fn collect_sidebar_thumbnails(&mut self, ctx: &egui::Context) {
        self.collect_masonry_dimensions();
        let mut completed = Vec::new();
        let mut cancelled = Vec::new();
        let mut disconnected = Vec::new();
        let mut timed_out = Vec::new();
        self.sidebar_thumbnail_receivers.retain(
            |(
                job_key,
                masonry_source,
                sidebar_managed,
                _priority_token,
                queued_at,
                started_at,
                cancellation,
                receiver,
            )| {
                if cancellation.load(Ordering::Acquire) {
                    cancelled.push(job_key.clone());
                    return false;
                }
                match receiver.try_recv() {
                    Ok((_result_key, result)) => {
                        completed.push((
                            job_key.clone(),
                            masonry_source.clone(),
                            *sidebar_managed,
                            result,
                        ));
                        return false;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected.push(job_key.clone());
                        return false;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                let has_timed_out = thumbnail_started_elapsed(started_at)
                    .map(|elapsed| elapsed >= THUMBNAIL_JOB_TIMEOUT)
                    .unwrap_or_else(|| queued_at.elapsed() >= THUMBNAIL_QUEUE_TIMEOUT);
                if has_timed_out {
                    if let Some(scheduler) = THUMBNAIL_SCHEDULER.get() {
                        scheduler.cancel_job(cancellation);
                    } else {
                        cancellation.store(true, Ordering::Release);
                    }
                    timed_out.push(job_key.clone());
                    eprintln!("[thumbnail] thumbnail pipeline timed out for {job_key}");
                    return false;
                }
                true
            },
        );
        for key in cancelled {
            self.sidebar_thumbnail_pending.remove(&key);
            self.sidebar_thumbnail_attempted.remove(&key);
            self.sidebar_thumbnail_failed.remove(&key);
        }
        for key in disconnected {
            self.sidebar_thumbnail_pending.remove(&key);
            // A disconnected result means a worker exited unexpectedly (for
            // example after a panic). Mark it failed so the UI does not create
            // an unbounded retry loop. Explicit retries clear this state.
            self.sidebar_thumbnail_attempted.insert(key.clone());
            self.sidebar_thumbnail_failed.insert(key);
        }
        for key in timed_out {
            self.sidebar_thumbnail_pending.remove(&key);
            self.sidebar_thumbnail_attempted.insert(key.clone());
            self.sidebar_thumbnail_failed.insert(key);
        }
        for (key, masonry_source, sidebar_managed, result) in completed {
            self.sidebar_thumbnail_pending.remove(&key);
            if let Ok(thumbnail) = &result
                && let Some(source_key) = thumbnail_source_key_from_cache_key(&key)
            {
                if let Some(dimensions) = thumbnail
                    .source_dimensions
                    .filter(|(width, height)| *width > 0 && *height > 0)
                {
                    self.masonry_dimension_results
                        .insert(source_key.to_owned(), Some(dimensions));
                    self.masonry_dimension_attempted
                        .insert(source_key.to_owned());
                    self.masonry_aspect_ratio_results.remove(source_key);
                } else if let Some(aspect_ratio) = thumbnail
                    .source_aspect_ratio
                    .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
                {
                    // Cache the provider's preview ratio without walking every
                    // entry in every tab. Visible Masonry cards consume it
                    // lazily; a real source-dimension probe may refine it later.
                    self.masonry_aspect_ratio_results
                        .insert(source_key.to_owned(), aspect_ratio);
                }
            }
            let sidebar_source_visible = !sidebar_managed
                || thumbnail_source_key_from_cache_key(&key).is_some_and(|source_key| {
                    self.sidebar_visible_sources.contains(source_key)
                        || self.sidebar_prefetch_sources.contains(source_key)
                });
            if !sidebar_source_visible
                || masonry_source
                    .as_ref()
                    .is_some_and(|source| !self.masonry_visible_sources.contains(source))
            {
                // This preview finished just as the user scrolled away or
                // filtered the entry out. Do not upload it to the GPU cache.
                self.sidebar_thumbnail_attempted.remove(&key);
                self.sidebar_thumbnail_failed.remove(&key);
                continue;
            }
            match result {
                Ok(thumbnail) => {
                    // Aspect ratios were placed in the shared source-key cache
                    // above. Masonry applies them only when the matching card is
                    // visible, avoiding an O(all-files) scan for every thumbnail.
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [thumbnail.width, thumbnail.height],
                        &thumbnail.pixels,
                    );
                    // `resolution` records real fitted source/provider detail
                    // before aspect correction. The corrected long dimension may
                    // be larger (128x128 -> 228x128), but must not falsely count
                    // as an available higher-resolution cache entry.
                    let texture_key = thumbnail_source_key_from_cache_key(&key)
                        .map(|source_key| shared_thumbnail_key(source_key, thumbnail.resolution))
                        .unwrap_or_else(|| key.clone());
                    let texture = ctx.load_texture(
                        format!("sidebar_thumb_{texture_key}"),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    // Enforce the shared cache's entry cap; a second pass below
                    // enforces its byte budget because Masonry may keep previews
                    // at any native/requested resolution in this same LRU cache.
                    if self.sidebar_thumbnails.len() >= 256
                        && let Some((evicted_key, _texture)) = self.sidebar_thumbnails.pop_lru()
                    {
                        remove_attempts_for_cached_source(
                            &mut self.sidebar_thumbnail_attempted,
                            &evicted_key,
                        );
                    }
                    self.sidebar_thumbnails.put(texture_key, texture);

                    // Bound the shared sidebar/Masonry cache by GPU texture bytes
                    // as well as count; requests are no longer limited to 512px.
                    const THUMBNAIL_TEXTURE_BUDGET: usize = 96 * 1024 * 1024;
                    let mut texture_bytes = self
                        .sidebar_thumbnails
                        .iter()
                        .map(|(_, texture)| {
                            let [width, height] = texture.size();
                            width.saturating_mul(height).saturating_mul(4)
                        })
                        .sum::<usize>();
                    while texture_bytes > THUMBNAIL_TEXTURE_BUDGET
                        && self.sidebar_thumbnails.len() > 1
                    {
                        if let Some((evicted_key, evicted_texture)) =
                            self.sidebar_thumbnails.pop_lru()
                        {
                            let [width, height] = evicted_texture.size();
                            texture_bytes = texture_bytes
                                .saturating_sub(width.saturating_mul(height).saturating_mul(4));
                            remove_attempts_for_cached_source(
                                &mut self.sidebar_thumbnail_attempted,
                                &evicted_key,
                            );
                        } else {
                            break;
                        }
                    }
                }
                Err(error) => {
                    eprintln!("[thumbnail] {error}");
                    self.sidebar_thumbnail_failed.insert(key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ThumbnailCandidate, candidate_output_limit, fit_thumbnail_to,
        native_thumbnail_quality_floor, normalize_thumbnail_aspect_ratio,
        offer_thumbnail_candidate, oriented_dimensions, source_thumbnail_key, thumbnail_cache_size,
        thumbnail_fit_rect, thumbnail_output_limit, thumbnail_source_key_from_cache_key,
    };
    use eframe::egui;

    #[test]
    fn own_decoder_fallback_stays_capped_at_256px() {
        let decoded = super::ThumbnailCandidate {
            image: image::RgbaImage::new(1200, 800),
            resolution: 1200,
            source: "Jpeg-Viewer decoder",
        };
        let native = super::ThumbnailCandidate {
            image: image::RgbaImage::new(1200, 800),
            resolution: 1200,
            source: "native OS/DE thumbnail",
        };
        assert_eq!(candidate_output_limit(&decoded, 900), 256);
        assert_eq!(candidate_output_limit(&native, 900), 900);
    }

    #[test]
    fn native_thumbnail_quality_floor_accepts_reasonably_close_cache_sizes() {
        assert_eq!(native_thumbnail_quality_floor(128), 96);
        assert_eq!(native_thumbnail_quality_floor(256), 171);
        assert_eq!(native_thumbnail_quality_floor(768), 256);
        assert_eq!(native_thumbnail_quality_floor(64), 64);
    }

    #[test]
    fn thumbnail_fit_rect_uses_full_bounds_for_matching_aspect_ratio() {
        let bounds = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 180.0));
        let fitted = thumbnail_fit_rect(bounds, 16.0 / 9.0);
        assert!((fitted.width() - bounds.width()).abs() < 0.0001);
        assert!((fitted.height() - bounds.height()).abs() < 0.0001);
    }

    #[test]
    fn thumbnail_fit_rect_contains_and_centers_wide_thumbnails() {
        let bounds = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(300.0, 200.0));
        let fitted = thumbnail_fit_rect(bounds, 16.0 / 9.0);
        assert!((fitted.width() / fitted.height() - 16.0 / 9.0).abs() < 0.0001);
        assert!(bounds.contains(fitted.min));
        assert!(bounds.contains(fitted.max));
        assert!((fitted.center().x - bounds.center().x).abs() < 0.0001);
        assert!((fitted.center().y - bounds.center().y).abs() < 0.0001);
    }

    #[test]
    fn thumbnail_fit_rect_falls_back_to_bounds_ratio_for_invalid_input() {
        let bounds = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 200.0));
        let fitted = thumbnail_fit_rect(bounds, f32::NAN);
        assert!((fitted.width() / fitted.height() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn square_native_preview_is_normalized_to_landscape_source_ratio() {
        let preview = image::RgbaImage::from_pixel(128, 128, image::Rgba([40, 80, 120, 255]));
        let normalized = normalize_thumbnail_aspect_ratio(preview, Some(16.0 / 9.0));
        assert_eq!(normalized.dimensions(), (228, 128));
    }

    #[test]
    fn square_native_preview_is_normalized_to_portrait_source_ratio() {
        let preview = image::RgbaImage::from_pixel(128, 128, image::Rgba([40, 80, 120, 255]));
        let normalized = normalize_thumbnail_aspect_ratio(preview, Some(9.0 / 16.0));
        assert_eq!(normalized.dimensions(), (128, 228));
    }

    #[test]
    fn already_correct_preview_is_not_resampled() {
        let preview = image::RgbaImage::from_pixel(228, 128, image::Rgba([40, 80, 120, 255]));
        let normalized = normalize_thumbnail_aspect_ratio(preview, Some(16.0 / 9.0));
        assert_eq!(normalized.dimensions(), (228, 128));
    }

    #[test]
    fn square_native_preview_keeps_requested_short_side_after_aspect_correction() {
        let preview = image::RgbaImage::from_pixel(128, 128, image::Rgba([40, 80, 120, 255]));
        let (width, height, pixels, resolution) = fit_thumbnail_to(preview, 128, Some(16.0 / 9.0))
            .expect("fit and aspect-correct native preview");
        assert_eq!((width, height), (228, 128));
        assert_eq!(resolution, 128);
        assert_eq!(pixels.len(), width * height * 4);
    }

    #[test]
    fn correctly_proportioned_preview_stays_within_requested_maximum_dimension() {
        let preview = image::RgbaImage::from_pixel(256, 144, image::Rgba([40, 80, 120, 255]));
        let (width, height, _pixels, resolution) = fit_thumbnail_to(preview, 128, Some(16.0 / 9.0))
            .expect("fit correctly proportioned preview");
        assert_eq!((width, height), (128, 72));
        assert_eq!(resolution, 128);
    }

    #[test]
    fn missing_or_invalid_source_ratio_keeps_preview_dimensions() {
        let preview = image::RgbaImage::from_pixel(128, 128, image::Rgba([40, 80, 120, 255]));
        assert_eq!(
            normalize_thumbnail_aspect_ratio(preview.clone(), None).dimensions(),
            (128, 128)
        );
        assert_eq!(
            normalize_thumbnail_aspect_ratio(preview, Some(f32::NAN)).dimensions(),
            (128, 128)
        );
    }

    #[test]
    fn thumbnail_output_size_does_not_exceed_original_source_resolution() {
        assert_eq!(thumbnail_output_limit(256, Some((160, 90))), 160);
        assert_eq!(thumbnail_output_limit(128, Some((160, 90))), 128);
        assert_eq!(thumbnail_output_limit(256, None), 256);
    }

    #[test]
    fn exif_rotations_swap_original_dimensions() {
        assert_eq!(oriented_dimensions(4000, 3000, Some(1)), (4000, 3000));
        assert_eq!(oriented_dimensions(4000, 3000, Some(6)), (3000, 4000));
        assert_eq!(oriented_dimensions(4000, 3000, Some(8)), (3000, 4000));
    }

    #[test]
    fn thumbnail_fitting_supports_sizes_above_the_old_512px_ceiling() {
        let source = image::RgbaImage::new(3000, 1000);
        let (width, height, pixels, resolution) =
            fit_thumbnail_to(source, 1536, None).expect("fit thumbnail");
        assert_eq!((width, height), (1536, 512));
        assert_eq!(resolution, 1536);
        assert_eq!(pixels.len(), width * height * 4);
    }

    #[test]
    fn undersized_native_thumbnail_is_retained_when_fallback_is_smaller() {
        let mut best: Option<ThumbnailCandidate> = None;
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(1024, 683),
            "native OS/DE thumbnail",
        );
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(256, 170),
            "EXIF embedded preview",
        );

        let best = best.expect("native thumbnail should remain available");
        assert_eq!(best.source, "native OS/DE thumbnail");
        assert_eq!(best.resolution, 1024);
        assert_eq!(best.image.dimensions(), (1024, 683));
    }

    #[test]
    fn higher_resolution_decoder_result_replaces_native_candidate() {
        let mut best: Option<ThumbnailCandidate> = None;
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(1024, 683),
            "native OS/DE thumbnail",
        );
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(2048, 1365),
            "Jpeg-Viewer decoder",
        );

        let best = best.expect("decoder result should be retained");
        assert_eq!(best.source, "Jpeg-Viewer decoder");
        assert_eq!(best.resolution, 2048);
        assert_eq!(best.image.dimensions(), (2048, 1365));
    }

    #[test]
    fn equal_resolution_candidate_keeps_native_provider_preference() {
        let mut best: Option<ThumbnailCandidate> = None;
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(1024, 683),
            "native OS/DE thumbnail",
        );
        offer_thumbnail_candidate(
            &mut best,
            image::RgbaImage::new(1024, 682),
            "Jpeg-Viewer decoder",
        );

        let best = best.expect("native candidate should be retained on a tie");
        assert_eq!(best.source, "native OS/DE thumbnail");
    }

    #[test]
    fn cache_key_parser_accepts_arbitrary_resolution_and_colons_in_source_key() {
        let source = "file:/tmp/a:b/image.jpg";
        let key = format!("thumb:1536:{source}");
        assert_eq!(thumbnail_cache_size(&key, source), Some(1536));
        assert_eq!(thumbnail_source_key_from_cache_key(&key), Some(source));
        assert_eq!(thumbnail_cache_size(&key, "file:/tmp/other.jpg"), None);
    }

    #[test]
    fn archive_and_file_sources_use_stable_thumbnail_keys() {
        let entry = crate::imaging::entry::ImageEntry::Zip(crate::imaging::entry::ArchiveImage {
            archive_path: std::path::PathBuf::from("/tmp/archive.zip"),
            entry_index: 4,
            name: "folder/image.jpg".to_owned(),
        });
        assert_eq!(source_thumbnail_key(&entry), entry.get_id());
    }
}

#[cfg(test)]
mod source_aspect_ratio_tests {
    use super::{oriented_dimensions, source_aspect_ratio_or_preview};

    #[test]
    fn known_source_ratio_wins_over_square_native_preview() {
        let ratio = source_aspect_ratio_or_preview(Some(16.0 / 9.0), 128, 128)
            .expect("known source ratio should be retained");
        assert!((ratio - 16.0 / 9.0).abs() < 0.0001);
    }

    #[test]
    fn preview_ratio_is_used_only_when_source_dimensions_are_unavailable() {
        let ratio = source_aspect_ratio_or_preview(None, 128, 72)
            .expect("native preview ratio should be a fallback");
        assert!((ratio - 16.0 / 9.0).abs() < 0.0001);
    }

    #[test]
    fn invalid_source_ratio_does_not_hide_valid_preview_dimensions() {
        let ratio = source_aspect_ratio_or_preview(Some(f32::NAN), 90, 160)
            .expect("valid preview ratio should be used");
        assert!((ratio - 9.0 / 16.0).abs() < 0.0001);
    }

    #[test]
    fn exif_rotations_swap_source_dimensions() {
        assert_eq!(oriented_dimensions(1920, 1080, Some(6)), (1080, 1920));
        assert_eq!(oriented_dimensions(1920, 1080, Some(8)), (1080, 1920));
        assert_eq!(oriented_dimensions(1920, 1080, Some(3)), (1920, 1080));
    }
}
