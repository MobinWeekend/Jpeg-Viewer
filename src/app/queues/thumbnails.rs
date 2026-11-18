//! Background generation and collection of sidebar thumbnails.
use super::super::types::{GeneratedThumbnail, ViewerApp};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::PathBuf;
use std::io::BufReader;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use std::time::Duration;

type ThumbnailJob = Box<dyn FnOnce() + Send + 'static>;

struct QueuedThumbnailJob {
    key: String,
    priority: u8,
    sequence: u64,
    /// Only jobs created exclusively for the visible Masonry surface are
    /// eligible for off-screen cancellation. Shared sidebar jobs stay alive.
    masonry_source_key: Option<String>,
    run: ThumbnailJob,
}

/// One bounded worker pool and one priority queue are shared by Browse, Queue,
/// Favorites, and Masonry. Visible Masonry tiles are queued ahead of sidebar
/// previews so the primary workspace gets the first available decode capacity.
struct ThumbnailScheduler {
    queue: Mutex<Vec<QueuedThumbnailJob>>,
    ready: Condvar,
    next_sequence: AtomicU64,
    capacity: usize,
    /// Tracks both queued and currently-running Masonry-only work so a scroll
    /// can cancel it even after a worker has taken the job from the queue.
    masonry_jobs: Arc<Mutex<HashMap<String, (String, Arc<AtomicBool>)>>>,
}

impl ThumbnailScheduler {
    fn submit(&self, key: String, priority: u8, job: ThumbnailJob) -> Result<(), ThumbnailJob> {
        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        if queue.len() >= self.capacity {
            return Err(job);
        }
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        queue.push(QueuedThumbnailJob {
            key,
            priority,
            sequence,
            masonry_source_key: None,
            run: job,
        });
        self.ready.notify_one();
        Ok(())
    }

    fn submit_masonry(
        &self,
        key: String,
        priority: u8,
        source_key: String,
        cancelled: Arc<AtomicBool>,
        job: ThumbnailJob,
    ) -> bool {
        // Use the same lock order as cancellation (registry, then queue) to
        // prevent a scroll/cancel racing with submission from deadlocking.
        let mut registry_guard = self.masonry_jobs.lock()
            .expect("Masonry thumbnail registry poisoned");
        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        if queue.len() >= self.capacity {
            return false;
        }
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        registry_guard.insert(key.clone(), (source_key.clone(), cancelled.clone()));
        drop(registry_guard);

        let registry = Arc::clone(&self.masonry_jobs);
        let cleanup_key = key.clone();
        let cleanup_token = Arc::clone(&cancelled);
        let run = Box::new(move || {
            if !cancelled.load(Ordering::Acquire) {
                job();
            }
            let mut registry = registry.lock().expect("Masonry thumbnail registry poisoned");
            if registry.get(&cleanup_key)
                .is_some_and(|(_, token)| Arc::ptr_eq(token, &cleanup_token))
            {
                registry.remove(&cleanup_key);
            }
        }) as ThumbnailJob;

        queue.push(QueuedThumbnailJob {
            key,
            priority,
            sequence,
            masonry_source_key: Some(source_key),
            run,
        });
        self.ready.notify_one();
        true
    }

    fn cancel_masonry_not_visible(&self, visible_sources: &HashSet<String>) {
        // Mark queued and active work first. The registry lock is released
        // before taking the queue lock to keep lock ordering consistent.
        {
            let mut registry = self.masonry_jobs.lock()
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

        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        queue.retain(|queued| {
            queued.masonry_source_key.as_ref()
                .is_none_or(|source_key| visible_sources.contains(source_key))
        });
    }

    fn promote(&self, key: &str, priority: u8) {
        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        if let Some(queued) = queue.iter_mut().find(|queued| queued.key == key) {
            queued.priority = queued.priority.max(priority);
            self.ready.notify_one();
        }
    }

    fn take_next(&self) -> ThumbnailJob {
        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        loop {
            if !queue.is_empty() {
                let next_index = queue
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, queued)| (std::cmp::Reverse(queued.priority), queued.sequence))
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                return queue.swap_remove(next_index).run;
            }
            #[cfg(target_os = "windows")]
            {
                // STA COM threads need a message pump. A short timed wait lets
                // idle thumbnail workers dispatch COM/window messages without
                // busy-spinning or holding the scheduler lock while dispatching.
                let (next_queue, _) = self
                    .ready
                    .wait_timeout(queue, Duration::from_millis(40))
                    .expect("thumbnail queue poisoned");
                queue = next_queue;
                if queue.is_empty() {
                    drop(queue);
                    crate::platform::thumbnails::pump_worker_messages();
                    queue = self.queue.lock().expect("thumbnail queue poisoned");
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                queue = self.ready.wait(queue).expect("thumbnail queue poisoned");
            }
        }
    }
}

static THUMBNAIL_SCHEDULER: OnceLock<Arc<ThumbnailScheduler>> = OnceLock::new();

fn thumbnail_scheduler(worker_count: usize) -> &'static Arc<ThumbnailScheduler> {
    THUMBNAIL_SCHEDULER.get_or_init(|| {
        let scheduler = Arc::new(ThumbnailScheduler {
            queue: Mutex::new(Vec::new()),
            ready: Condvar::new(),
            next_sequence: AtomicU64::new(0),
            capacity: 64,
            masonry_jobs: Arc::new(Mutex::new(HashMap::new())),
        });
        for worker_index in 0..worker_count.clamp(1, 8) {
            let worker_scheduler = Arc::clone(&scheduler);
            let _ = std::thread::Builder::new()
                .name(format!("thumbnail-worker-{worker_index}"))
                .spawn(move || {
                    // Keep platform state (notably the Windows COM apartment)
                    // alive for the worker's lifetime, not once per thumbnail.
                    let _platform_context =
                        crate::platform::thumbnails::ThumbnailWorkerContext::initialize();
                    loop {
                        let job = worker_scheduler.take_next();
                        job();
                        crate::platform::thumbnails::pump_worker_messages();
                    }
                });
        }
        scheduler
    })
}

pub(crate) fn update_masonry_thumbnail_visibility(visible_sources: &HashSet<String>) {
    if let Some(scheduler) = THUMBNAIL_SCHEDULER.get() {
        scheduler.cancel_masonry_not_visible(visible_sources);
    }
}

const MASONRY_DIMENSION_PRIORITY: u8 = 20;
const MASONRY_PLACEHOLDER_PRIORITY: u8 = 12;
const MASONRY_THUMBNAIL_PRIORITY: u8 = 10;
const SIDEBAR_THUMBNAIL_PRIORITY: u8 = 0;
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
        .or_else(|| exif.get_field(exif::Tag::ImageWidth, exif::In::PRIMARY)
            .and_then(|field| field.value.get_uint(0)))?;
    let height = exif
        .get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .or_else(|| exif.get_field(exif::Tag::ImageLength, exif::In::PRIMARY)
            .and_then(|field| field.value.get_uint(0)))?;
    (width > 0 && height > 0).then_some((width, height))
}

/// Read dimensions through Jpeg-Viewer's format registry. This is used for
/// archive entries and as the fallback for formats not understood by image's
/// lightweight file-header reader. EXIF orientation is applied so the result
/// matches the image displayed by the viewer.
fn source_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    let format = crate::decoder::format_detection::detect_format(bytes)?;
    let (width, height) = crate::decoder::default_registry().dimensions(bytes, format).ok()?;
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
    // Probe the image header first. This is normally a small, sequential read
    // and avoids parsing the full EXIF container on the common path. EXIF is
    // still read for orientation and as a fallback for formats whose dimensions
    // are not understood by image's lightweight header reader.
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

/// Avoid enlarging a thumbnail beyond the source image's own maximum
/// dimension. The source dimensions are already EXIF-oriented.
fn thumbnail_output_limit(
    requested_size: u32,
    source_dimensions: Option<(u32, u32)>,
) -> u32 {
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
    if best.as_ref().is_none_or(|candidate| resolution > candidate.resolution) {
        *best = Some(ThumbnailCandidate { image, resolution, source });
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

fn finish_thumbnail_candidate(
    candidate: ThumbnailCandidate,
    output_limit: u32,
    source_aspect_ratio: Option<f32>,
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
const MAX_SOURCE_RATIO_FALLBACK_READ_BYTES: u64 = 128 * 1024 * 1024;
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
    let marker_start = bytes.windows(exif_marker.len()).position(|w| w == exif_marker)?;
    let tiff_start = marker_start.checked_add(exif_marker.len())?;
    let start = tiff_start.checked_add(offset)?;
    let end = start.checked_add(length)?;
    Some(bytes.get(start..end)?.to_vec())
}

fn decode_checked_sidebar_image(
    bytes: &[u8], name: &str, is_raw: bool, source_path: Option<&std::path::Path>,
) -> Result<image::RgbaImage, String> {
    if is_raw {
        let thumbnail = crate::decoder::extract_raw_thumbnail(bytes, source_path)
            .ok_or_else(|| format!("No embedded RAW thumbnail found in {name}"))?;
        let image = image::load_from_memory(&thumbnail)
            .map_err(|error| format!("Failed to decode embedded RAW thumbnail in {name}: {error}"))?;
        return Ok(image.to_rgba8());
    }

    let format = crate::decoder::format_detection::detect_format(bytes)
        .ok_or_else(|| format!("Unknown image format: {name}"))?;
    let registry = crate::decoder::default_registry();
    let (width, height) = registry.dimensions(bytes, format).map_err(|e| e.to_string())?;
    let pixels = u64::from(width).checked_mul(u64::from(height))
        .ok_or_else(|| format!("Image dimensions overflow: {name}"))?;
    let rgba_bytes = pixels.checked_mul(4)
        .ok_or_else(|| format!("Decoded image size overflow: {name}"))?;
    if width == 0 || height == 0 || pixels > MAX_THUMB_SOURCE_PIXELS {
        return Err(format!("Image exceeds thumbnail decode safety limit ({width}x{height}): {name}"));
    }
    // Guard the allocation size explicitly before invoking the full decoder.
    if rgba_bytes > MAX_THUMB_SOURCE_PIXELS * 4 {
        return Err(format!("Decoded image would exceed thumbnail memory limit: {name}"));
    }
    let decoded = registry.decode(bytes, format, &crate::imaging::core::DecodeOptions::default())
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
    /// Queue a metadata-only dimension pass for every file in the filtered
    /// Masonry collection. These jobs outrank thumbnail work so all tile
    /// geometry can be established before expensive preview generation begins.
    pub(crate) fn prepare_masonry_dimensions(
        &mut self,
        entries: &[&crate::imaging::entry::ImageEntry],
        ctx: &egui::Context,
    ) {
        let scheduler = thumbnail_scheduler(self.settings_manager.get().thumbnail_workers);
        for entry in entries {
            let crate::imaging::entry::ImageEntry::File(path) = *entry else {
                // Archive dimensions are obtained from the archive member bytes
                // when its first preview is requested. Do not decompress every
                // archive member just to run the metadata prepass.
                continue;
            };
            let source_key = source_thumbnail_key(entry);
            let entry_id = entry.get_id();
            if let Some(dimensions) = self.masonry_dimension_results.get(&source_key).copied() {
                if let Some((width, height)) = dimensions {
                    if width > 0 && height > 0 {
                        if let Some(tab) = self.workspace.tabs.get_mut(self.workspace.active_index) {
                            tab.masonry_dimensions.insert(entry_id.clone(), (width, height));
                            tab.masonry_aspect_ratios.insert(entry_id, width as f32 / height as f32);
                        }
                    }
                }
                self.masonry_dimension_attempted.insert(source_key);
                continue;
            }
            if self.masonry_dimension_attempted.contains(&source_key)
                || self.masonry_dimension_pending.contains(&source_key)
            {
                continue;
            }

            let (tx, rx) = std::sync::mpsc::channel();
            let worker_path = path.to_path_buf();
            let worker_ctx = ctx.clone();
            let job = Box::new(move || {
                let dimensions = source_dimensions_from_file(&worker_path);
                let _ = tx.send(dimensions);
                worker_ctx.request_repaint();
            }) as ThumbnailJob;
            let scheduler_key = format!("masonry-dimensions:{source_key}");
            match scheduler.submit(scheduler_key, MASONRY_DIMENSION_PRIORITY, job) {
                Ok(()) => {
                    self.masonry_dimension_attempted.insert(source_key.clone());
                    self.masonry_dimension_pending.insert(source_key.clone());
                    self.masonry_dimension_receivers.push((source_key, rx));
                }
                Err(_job) => {
                    // The bounded scheduler is full. Stop here and retry the
                    // remaining entries on the next frame as completed work is
                    // collected; do not starve visible thumbnails with an
                    // unbounded queue of metadata tasks.
                    break;
                }
            }
        }
    }

    /// True once every ordinary file in this filtered Masonry collection has
    /// completed (successfully or unsuccessfully) its lightweight dimensions
    /// probe. Unknown formats still get laid out with the neutral fallback ratio.
    pub(crate) fn masonry_dimensions_ready(
        &self,
        entries: &[&crate::imaging::entry::ImageEntry],
    ) -> bool {
        entries.iter().all(|entry| {
            if !matches!(entry, crate::imaging::entry::ImageEntry::File(_)) {
                return true;
            }
            let source_key = source_thumbnail_key(entry);
            self.masonry_dimension_attempted.contains(&source_key)
                && !self.masonry_dimension_pending.contains(&source_key)
        })
    }

    /// Update the Masonry aspect-ratio/dimension maps from the metadata pass.
    /// A single source can appear in several open tabs, so propagate the result
    /// to every matching entry instead of rescanning the same file per tab.
    pub(crate) fn collect_masonry_dimensions(&mut self) {
        let mut remaining = Vec::with_capacity(self.masonry_dimension_receivers.len());
        let mut completed = Vec::new();
        for (source_key, receiver) in self.masonry_dimension_receivers.drain(..) {
            match receiver.try_recv() {
                Ok(dimensions) => completed.push((source_key, dimensions)),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    remaining.push((source_key, receiver));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.masonry_dimension_pending.remove(&source_key);
                    self.masonry_dimension_attempted.remove(&source_key);
                }
            }
        }
        self.masonry_dimension_receivers = remaining;

        for (source_key, dimensions) in completed {
            self.masonry_dimension_pending.remove(&source_key);
            self.masonry_dimension_results.insert(source_key.clone(), dimensions);
            let Some((width, height)) = dimensions.filter(|(width, height)| *width > 0 && *height > 0) else {
                continue;
            };
            let aspect_ratio = width as f32 / height as f32;
            for tab in &mut self.workspace.tabs {
                let matching_ids = tab.entries.iter()
                    .filter(|entry| source_thumbnail_key(entry) == source_key)
                    .map(crate::imaging::entry::ImageEntry::get_id)
                    .collect::<Vec<_>>();
                for id in matching_ids {
                    tab.masonry_dimensions.insert(id.clone(), (width, height));
                    tab.masonry_aspect_ratios.insert(id, aspect_ratio);
                }
            }
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
        self.request_sized_sidebar_thumbnail(
            path,
            cache_key,
            target_size,
            None,
            SIDEBAR_THUMBNAIL_PRIORITY,
            ctx,
        );
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
        let mut available = self.sidebar_thumbnails.iter()
            .filter_map(|(cache_key, _)| thumbnail_cache_size(cache_key, key))
            .collect::<Vec<_>>();
        available.sort_unstable();
        available.dedup();

        let selected_size = available.iter().copied()
            .find(|size| *size >= requested_size)
            .or_else(|| available.iter().copied().filter(|size| *size < requested_size).max());
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
        self.sidebar_thumbnails.iter()
            .filter_map(|(cache_key, _)| thumbnail_cache_size(cache_key, key))
            .max()
    }

    pub(crate) fn has_shared_thumbnail_at_least(&mut self, key: &str, requested_size: u32) -> bool {
        let requested_size = requested_size.max(1);
        let found = self.sidebar_thumbnails.iter()
            .any(|(cache_key, _)| thumbnail_cache_size(cache_key, key)
                .is_some_and(|size| size >= requested_size));
        found
    }

    pub(crate) fn shared_thumbnail_failed(&self, key: &str, requested_size: u32) -> bool {
        self.sidebar_thumbnail_failed.contains(&shared_thumbnail_key(key, requested_size))
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
    ) {
        if !path.is_file() {
            return;
        }
        // Check pending before attempted: Masonry may request the same shared
        // resolution after Browse queued it, and must be able to promote it.
        if self.sidebar_thumbnail_pending.contains(&key) {
            if priority > SIDEBAR_THUMBNAIL_PRIORITY {
                thumbnail_scheduler(self.settings_manager.get().thumbnail_workers).promote(&key, priority);
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
        let masonry_only = priority >= MASONRY_THUMBNAIL_PRIORITY;
        let masonry_placeholder = priority == MASONRY_PLACEHOLDER_PRIORITY;
        let masonry_source_key = thumbnail_source_key_from_cache_key(&key).map(str::to_owned);
        let receiver_masonry_source = if masonry_only { masonry_source_key.clone() } else { None };
        self.sidebar_thumbnail_receivers.push((key.clone(), receiver_masonry_source, rx));

        let job_key = key.clone();
        let scheduler_key = key.clone();
        let repaint_ctx = ctx.clone();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let job = Box::new(move || {
            let result = (|| -> Result<GeneratedThumbnail, String> {
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                // Masonry's metadata-first phase normally supplies these
                // dimensions already. Other consumers retain the lazy fallback.
                let mut source_dimensions = precomputed_source_dimensions.or_else(|| {
                    (!masonry_only).then(|| source_dimensions_from_file(&path)).flatten()
                });
                let mut source_aspect_ratio = source_dimensions
                    .map(|(width, height)| width as f32 / height as f32);
                let mut preloaded_source_bytes: Option<Vec<u8>> = None;
                let mut native_request_size = thumbnail_output_limit(target_size, source_dimensions);
                let mut best_candidate: Option<ThumbnailCandidate> = None;

                // Request the native OS/DE preview first. A lower-resolution
                // result is still a valid candidate: it must not be thrown away
                // just because the viewer asked for a larger Masonry tile.
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                let native = if masonry_placeholder {
                    crate::platform::thumbnails::load_or_request_placeholder(
                        &path,
                        native_request_size,
                    )
                } else {
                    crate::platform::thumbnails::load_or_request(
                        &path,
                        native_request_size,
                    )
                };
                if let Some(native) = native {
                    // When neither the file-header reader nor Shell metadata
                    // exposes the original ratio, inspect modest-sized source
                    // files before deciding how to normalize a square preview.
                    if source_aspect_ratio.is_none() && native.width() == native.height() {
                        let source_size = fs::metadata(&path).ok().map(|metadata| metadata.len());
                        if source_size.is_some_and(|size| size <= MAX_SOURCE_RATIO_FALLBACK_READ_BYTES) {
                            if let Ok(bytes) = fs::read(&path) {
                                source_dimensions = source_dimensions_from_bytes(&bytes);
                                source_aspect_ratio = source_dimensions
                                    .map(|(width, height)| width as f32 / height as f32);
                                // Recalculate the usable output bound now that
                                // source dimensions are known. The native request
                                // has already been issued, but its result can now
                                // be compared against the true source maximum.
                                native_request_size =
                                    thumbnail_output_limit(target_size, source_dimensions);
                                preloaded_source_bytes = Some(bytes);
                            }
                        }
                    }

                    let native_width = native.width();
                    let native_height = native.height();
                    let native_resolution = native_width.max(native_height);
                    offer_thumbnail_candidate(
                        &mut best_candidate,
                        native,
                        "native OS/DE thumbnail",
                    );

                    // Desktop providers use discrete cache flavors and may
                    // return a close-but-not-exact size. A result at or above
                    // two-thirds of the requested side is considered close
                    // enough to avoid a full source decode.
                    let native_quality_floor = native_thumbnail_quality_floor(native_request_size);
                    let native_resolution_is_sufficient =
                        native_resolution >= native_quality_floor;

                    // The placeholder pass is about coverage and responsiveness,
                    // not final sharpness. Never hold a visible tile hostage to a
                    // full-resolution decode when Windows/the DE has returned any
                    // valid non-empty preview, even if it is only 32 or 48 pixels.
                    if masonry_placeholder && native_resolution > 0 {
                        return finish_thumbnail_candidate(
                            best_candidate.expect("native thumbnail candidate was just inserted"),
                            native_request_size,
                            source_aspect_ratio,
                        );
                    }

                    if native_resolution < native_request_size
                        && crate::platform::thumbnails::diagnostics_enabled()
                    {
                        let assessment = if native_resolution_is_sufficient {
                            "meets the native quality floor; source decoding will be skipped"
                        } else {
                            "kept as a fallback candidate while checking embedded/source previews"
                        };
                        eprintln!(
                            "[thumbnail] {}: native OS/DE returned {}x{} for a {}px request ({}px native floor); {}",
                            path.display(),
                            native_width,
                            native_height,
                            native_request_size,
                            native_quality_floor,
                            assessment,
                        );
                    }

                    if native_resolution_is_sufficient
                    {
                        #[cfg(target_os = "windows")]
                        if native_resolution < native_request_size
                            && crate::platform::thumbnails::diagnostics_enabled()
                        {
                            eprintln!(
                                "[thumbnail] {}: accepting Windows native {}x{} thumbnail for {}px request (quality floor {}px); skipping source decode",
                                path.display(),
                                native_width,
                                native_height,
                                native_request_size,
                                native_quality_floor,
                            );
                        }
                        return finish_thumbnail_candidate(
                            best_candidate.expect("native thumbnail candidate was just inserted"),
                            native_request_size,
                            source_aspect_ratio,
                        );
                    }
                }

                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }

                // A larger embedded/full decode may improve an undersized native
                // preview. Keep the native candidate while trying those sources,
                // and avoid reading unusually large compressed files into RAM.
                let bytes = match preloaded_source_bytes {
                    Some(bytes) => bytes,
                    None => {
                        if let Ok(metadata) = fs::metadata(&path) {
                            if metadata.len() > MAX_THUMB_SOURCE_READ_BYTES {
                                if let Some(candidate) = best_candidate.take() {
                                    let output_limit =
                                        thumbnail_output_limit(target_size, source_dimensions);
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
                                    );
                                }
                                return Err(format!(
                                    "Source file is too large for the thumbnail fallback read ({} bytes): {}",
                                    metadata.len(),
                                    path.display(),
                                ));
                            }
                        }
                        if worker_cancellation.load(Ordering::Acquire) {
                            return Err("__masonry_thumbnail_cancelled__".into());
                        }
                        match fs::read(&path) {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                if let Some(candidate) = best_candidate.take() {
                                    eprintln!(
                                        "[thumbnail] {}: source read failed ({error}); using {} ({}x{})",
                                        path.display(),
                                        candidate.source,
                                        candidate.image.width(),
                                        candidate.image.height(),
                                    );
                                    let output_limit =
                                        thumbnail_output_limit(target_size, source_dimensions);
                                    return finish_thumbnail_candidate(
                                        candidate,
                                        output_limit,
                                        source_aspect_ratio,
                                    );
                                }
                                return Err(format!("Failed to read {}: {error}", path.display()));
                            }
                        }
                    }
                };
                source_dimensions = source_dimensions.or_else(|| source_dimensions_from_bytes(&bytes));
                source_aspect_ratio = source_aspect_ratio
                    .or_else(|| source_dimensions.map(|(width, height)| width as f32 / height as f32));

                // RAW uses its embedded preview/thumbnail path; do not develop
                // full sensor data merely to populate a thumbnail.
                let is_raw = path.extension().and_then(|e| e.to_str())
                    .and_then(crate::imaging::core::ImageFormat::from_extension)
                    == Some(crate::imaging::core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);

                let output_limit = thumbnail_output_limit(target_size, source_dimensions);

                if !is_raw {
                    if let Some(embedded) = extract_embedded_metadata_thumbnail(&bytes) {
                        if let Ok(image) = image::load_from_memory(&embedded) {
                            let image = image.to_rgba8();
                            let embedded_resolution = image.width().max(image.height());
                            offer_thumbnail_candidate(
                                &mut best_candidate,
                                image,
                                "EXIF embedded preview",
                            );

                            // An embedded preview that reaches the requested
                            // size can be used immediately. Smaller embedded
                            // candidates remain available while full decode is
                            // attempted and can still win over a worse native
                            // result.
                            if (masonry_placeholder && embedded_resolution > 0)
                                || (embedded_resolution >= output_limit
                                    && best_candidate.as_ref().is_some_and(|candidate| {
                                        candidate.resolution >= output_limit
                                    }))
                            {
                                // A valid embedded preview is preferable to
                                // leaving the visible placeholder blank while a
                                // much slower full-source decode runs.
                                return finish_thumbnail_candidate(
                                    best_candidate.expect("embedded candidate exists"),
                                    output_limit,
                                    source_aspect_ratio,
                                );
                            }
                        }
                    }
                }

                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                match decode_checked_sidebar_image(
                    &bytes,
                    &path.display().to_string(),
                    is_raw,
                    Some(&path),
                ) {
                    Ok(decoded) => offer_thumbnail_candidate(
                        &mut best_candidate,
                        decoded,
                        if is_raw { "embedded RAW preview" } else { "Jpeg-Viewer decoder" },
                    ),
                    Err(error) => {
                        if let Some(candidate) = best_candidate.as_ref() {
                            // Keep the source decoder's reason visible during
                            // diagnosis, but don't discard a usable native or
                            // embedded thumbnail because the original is huge.
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
                    format!("No usable native, embedded, or decoded thumbnail: {}", path.display())
                })?;
                let output_limit = candidate_output_limit(&candidate, output_limit);
                finish_thumbnail_candidate(candidate, output_limit, source_aspect_ratio)
            })();

            // A cancelled off-screen Masonry job intentionally sends no result:
            // dropping the sender clears its pending/attempted state in the UI
            // collector without reporting a false thumbnail failure.
            if masonry_only && worker_cancellation.load(Ordering::Acquire) {
                return;
            }
            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        let scheduler = thumbnail_scheduler(self.settings_manager.get().thumbnail_workers);
        let submitted = if masonry_only {
            if let Some(source_key) = masonry_source_key {
                scheduler.submit_masonry(
                    scheduler_key,
                    priority,
                    source_key,
                    cancellation,
                    job,
                )
            } else {
                eprintln!("[thumbnail] Masonry job has no source identity; submitting without off-screen cancellation");
                scheduler.submit(scheduler_key, priority, job).is_ok()
            }
        } else {
            scheduler.submit(scheduler_key, priority, job).is_ok()
        };
        if !submitted {
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
        );
    }

    #[allow(unused_assignments)]
    fn request_sized_archive_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        key: String,
        target_size: u32,
        priority: u8,
        ctx: &egui::Context,
        masonry_source_key: Option<String>,
    ) {
        // As above, promote a pending sidebar extraction before honoring the
        // attempted set. This keeps visible Masonry entries ahead in the queue.
        if self.sidebar_thumbnail_pending.contains(&key) {
            if priority > SIDEBAR_THUMBNAIL_PRIORITY {
                thumbnail_scheduler(self.settings_manager.get().thumbnail_workers).promote(&key, priority);
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
        let receiver_masonry_source = masonry_source_key.clone();
        self.sidebar_thumbnail_receivers.push((key.clone(), receiver_masonry_source, rx));
        let job_key = key.clone();
        let scheduler_key = key.clone();
        let repaint_ctx = ctx.clone();
        let masonry_only = masonry_source_key.is_some();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let job = Box::new(move || {
            let result = (|| -> Result<GeneratedThumbnail, String> {
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                use std::io::Read;
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                let bytes = match &entry {
                    crate::imaging::entry::ImageEntry::File(path) => {
                        std::fs::read(path).map_err(|e| e.to_string())?
                    }
                    crate::imaging::entry::ImageEntry::Zip(item) => {
                        let file = std::fs::File::open(&item.archive_path).map_err(|e| e.to_string())?;
                        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                        let mut item_file = archive.by_index(item.entry_index).map_err(|e| e.to_string())?;
                        let mut bytes = Vec::new();
                        item_file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                        bytes
                    }
                    crate::imaging::entry::ImageEntry::S7z(item) => {
                        let mut archive = sevenz_rust2::ArchiveReader::open(
                            &item.archive_path, sevenz_rust2::Password::empty(),
                        ).map_err(|e| e.to_string())?;
                        archive.read_file(&item.name).map_err(|e| e.to_string())?
                    }
                    crate::imaging::entry::ImageEntry::Rar(item) => {
                        let mut archive = unrar::Archive::new(&item.archive_path)
                            .open_for_processing().map_err(|e| e.to_string())?;
                        loop {
                            let header = archive.read_header().map_err(|e| e.to_string())?
                                .ok_or_else(|| format!("Entry not found: {}", item.name))?;
                            if header.entry().filename.to_string_lossy() == item.name {
                                let (bytes, next) = header.read().map_err(|e| e.to_string())?;
                                archive = next;
                                break bytes;
                            }
                            archive = header.skip().map_err(|e| e.to_string())?;
                        }
                    }
                };
                let name = match &entry {
                    crate::imaging::entry::ImageEntry::File(path) => path.display().to_string(),
                    crate::imaging::entry::ImageEntry::Zip(item) => item.name.clone(),
                    crate::imaging::entry::ImageEntry::S7z(item) => item.name.clone(),
                    crate::imaging::entry::ImageEntry::Rar(item) => item.name.clone(),
                };
                let is_raw = std::path::Path::new(&name)
                    .extension().and_then(|e| e.to_str())
                    .and_then(crate::imaging::core::ImageFormat::from_extension)
                    == Some(crate::imaging::core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);
                let source_dimensions = source_dimensions_from_bytes(&bytes);
                let source_aspect_ratio = source_dimensions
                    .map(|(width, height)| width as f32 / height as f32);
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("__masonry_thumbnail_cancelled__".into());
                }
                let rgba = decode_checked_sidebar_image(&bytes, &name, is_raw, None)?;
                let source_aspect_ratio = source_aspect_ratio_or_preview(
                    source_aspect_ratio,
                    rgba.width(),
                    rgba.height(),
                );
                let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                let (width, height, pixels, resolution) = fit_thumbnail_to(rgba, output_limit, source_aspect_ratio)?;
                Ok(GeneratedThumbnail { width, height, pixels, resolution, source_aspect_ratio })
            })();
            if masonry_only && worker_cancellation.load(Ordering::Acquire) {
                return;
            }
            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        let scheduler = thumbnail_scheduler(self.settings_manager.get().thumbnail_workers);
        let submitted = if let Some(source_key) = masonry_source_key {
            scheduler.submit_masonry(
                scheduler_key,
                priority,
                source_key,
                cancellation,
                job,
            )
        } else {
            scheduler.submit(scheduler_key, priority, job).is_ok()
        };
        if !submitted {
            self.sidebar_thumbnail_pending.remove(&job_key);
            self.sidebar_thumbnail_attempted.remove(&job_key);
            self.sidebar_thumbnail_receivers.pop();
        }
    }
    pub fn collect_sidebar_thumbnails(&mut self, ctx: &egui::Context) {
        self.collect_masonry_dimensions();
        let mut completed = Vec::new();
        let mut disconnected = Vec::new();
        self.sidebar_thumbnail_receivers
            .retain(|(job_key, masonry_source, receiver)| match receiver.try_recv() {
                Ok((_result_key, result)) => {
                    completed.push((job_key.clone(), masonry_source.clone(), result));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected.push(job_key.clone());
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
            });
        for key in disconnected {
            self.sidebar_thumbnail_pending.remove(&key);
            self.sidebar_thumbnail_attempted.remove(&key);
            self.sidebar_thumbnail_failed.remove(&key);
        }
        for (key, masonry_source, result) in completed {
            self.sidebar_thumbnail_pending.remove(&key);
            if masonry_source.as_ref().is_some_and(|source| {
                !self.masonry_visible_sources.contains(source)
            }) {
                // This preview finished just as the user scrolled away or
                // filtered the entry out. Do not upload it to the GPU cache.
                self.sidebar_thumbnail_attempted.remove(&key);
                self.sidebar_thumbnail_failed.remove(&key);
                continue;
            }
            match result {
                Ok(thumbnail) => {
                    if let Some(aspect_ratio) = thumbnail.source_aspect_ratio.filter(|ratio| {
                        ratio.is_finite() && *ratio > 0.0
                    }) {
                        // The job key is thumb:<requested-size>:<stable-source-key>.
                        // Propagate the source ratio to every open workspace tab
                        // containing that same entry, including jobs initially
                        // queued by Browse/Queue/Favorites.
                        if let Some(source_key) = thumbnail_source_key_from_cache_key(&key) {
                            for tab in &mut self.workspace.tabs {
                                let matching_ids = tab.entries.iter()
                                    .filter(|entry| source_thumbnail_key(entry) == source_key)
                                    .map(|entry| entry.get_id())
                                    .collect::<Vec<_>>();
                                for id in matching_ids {
                                    tab.masonry_aspect_ratios.insert(id, aspect_ratio);
                                }
                            }
                        }
                    }
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
                    if self.sidebar_thumbnails.len() >= 256 {
                        if let Some((evicted_key, _texture)) = self.sidebar_thumbnails.pop_lru() {
                            remove_attempts_for_cached_source(&mut self.sidebar_thumbnail_attempted, &evicted_key);
                        }
                    }
                    self.sidebar_thumbnails.put(texture_key, texture);

                    // Bound the shared sidebar/Masonry cache by GPU texture bytes
                    // as well as count; requests are no longer limited to 512px.
                    const THUMBNAIL_TEXTURE_BUDGET: usize = 96 * 1024 * 1024;
                    let mut texture_bytes = self.sidebar_thumbnails.iter()
                        .map(|(_, texture)| {
                            let [width, height] = texture.size();
                            width.saturating_mul(height).saturating_mul(4)
                        })
                        .sum::<usize>();
                    while texture_bytes > THUMBNAIL_TEXTURE_BUDGET && self.sidebar_thumbnails.len() > 1 {
                        if let Some((evicted_key, evicted_texture)) = self.sidebar_thumbnails.pop_lru() {
                            let [width, height] = evicted_texture.size();
                            texture_bytes = texture_bytes.saturating_sub(width.saturating_mul(height).saturating_mul(4));
                            remove_attempts_for_cached_source(&mut self.sidebar_thumbnail_attempted, &evicted_key);
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
        candidate_output_limit, fit_thumbnail_to, normalize_thumbnail_aspect_ratio,
        offer_thumbnail_candidate, oriented_dimensions, source_thumbnail_key,
        thumbnail_cache_size, thumbnail_fit_rect, native_thumbnail_quality_floor,
        thumbnail_output_limit,
        thumbnail_source_key_from_cache_key, ThumbnailCandidate,
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
        assert_eq!(normalize_thumbnail_aspect_ratio(preview.clone(), None).dimensions(), (128, 128));
        assert_eq!(normalize_thumbnail_aspect_ratio(preview, Some(f32::NAN)).dimensions(), (128, 128));
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
        let (width, height, pixels, resolution) = fit_thumbnail_to(source, 1536, None).expect("fit thumbnail");
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
