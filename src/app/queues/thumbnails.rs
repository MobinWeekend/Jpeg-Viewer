//! Background generation and collection of sidebar thumbnails.
use super::super::types::{GeneratedThumbnail, ViewerApp};
use eframe::egui;
use std::collections::HashSet;
use std::fs::{self, File};
use std::path::PathBuf;
use std::io::BufReader;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

type ThumbnailJob = Box<dyn FnOnce() + Send + 'static>;

struct QueuedThumbnailJob {
    key: String,
    priority: u8,
    sequence: u64,
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
}

impl ThumbnailScheduler {
    fn submit(&self, key: String, priority: u8, job: ThumbnailJob) -> Result<(), ThumbnailJob> {
        let mut queue = self.queue.lock().expect("thumbnail queue poisoned");
        if queue.len() >= self.capacity {
            return Err(job);
        }
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        queue.push(QueuedThumbnailJob { key, priority, sequence, run: job });
        self.ready.notify_one();
        Ok(())
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
            queue = self.ready.wait(queue).expect("thumbnail queue poisoned");
        }
    }
}

fn thumbnail_scheduler(worker_count: usize) -> &'static Arc<ThumbnailScheduler> {
    static SCHEDULER: OnceLock<Arc<ThumbnailScheduler>> = OnceLock::new();
    SCHEDULER.get_or_init(|| {
        let scheduler = Arc::new(ThumbnailScheduler {
            queue: Mutex::new(Vec::new()),
            ready: Condvar::new(),
            next_sequence: AtomicU64::new(0),
            capacity: 64,
        });
        for worker_index in 0..worker_count.clamp(1, 8) {
            let worker_scheduler = Arc::clone(&scheduler);
            let _ = std::thread::Builder::new()
                .name(format!("thumbnail-worker-{worker_index}"))
                .spawn(move || loop {
                    let job = worker_scheduler.take_next();
                    job();
                });
        }
        scheduler
    })
}

const MASONRY_THUMBNAIL_PRIORITY: u8 = 10;
const SIDEBAR_THUMBNAIL_PRIORITY: u8 = 0;

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
    let exif = File::open(path).ok().and_then(|file| {
        let mut reader = BufReader::new(file);
        exif::Reader::new().read_from_container(&mut reader).ok()
    });
    if let Some(dimensions) = image::image_dimensions(path).ok()
        .or_else(|| exif.as_ref().and_then(exif_dimensions))
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
    pub(crate) fn request_sidebar_thumbnail(
        &mut self,
        path: PathBuf,
        key: String,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cache_key = shared_thumbnail_key(&key, target_size);
        self.request_sized_sidebar_thumbnail(path, cache_key, target_size, SIDEBAR_THUMBNAIL_PRIORITY, ctx);
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
        ctx: &egui::Context,
    ) {
        let target_size = target_size.max(1);
        let cache_source_key = source_thumbnail_key(&entry);
        let cache_key = shared_thumbnail_key(&cache_source_key, target_size);
        match entry {
            crate::imaging::entry::ImageEntry::File(path) => {
                self.request_sized_sidebar_thumbnail(path, cache_key, target_size, MASONRY_THUMBNAIL_PRIORITY, ctx);
            }
            archive_entry => {
                self.request_sized_archive_thumbnail(archive_entry, cache_key, target_size, MASONRY_THUMBNAIL_PRIORITY, ctx);
            }
        }
    }

    fn request_sized_sidebar_thumbnail(
        &mut self,
        path: PathBuf,
        key: String,
        target_size: u32,
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
        self.sidebar_thumbnail_receivers.push(rx);

        let job_key = key.clone();
        let scheduler_key = key.clone();
        let repaint_ctx = ctx.clone();
        let job = Box::new(move || {
            let result = (|| -> Result<GeneratedThumbnail, String> {
                // Resolve original dimensions independently from the preview.
                // This is deliberately header/metadata-only when possible and
                // runs on the worker, never the UI thread.
                let mut source_dimensions = source_dimensions_from_file(&path);
                let mut source_aspect_ratio = source_dimensions
                    .map(|(width, height)| width as f32 / height as f32);
                let mut preloaded_source_bytes: Option<Vec<u8>> = None;

                // Ask the operating system for its existing/generated thumbnail
                // before reading and decoding the full source image. When the
                // source is smaller than the preview viewport, request its own
                // maximum dimension because upscaling cannot add detail.
                let mut native_request_size = thumbnail_output_limit(target_size, source_dimensions);
                if let Some(native) = crate::platform::thumbnails::load_or_request(&path, native_request_size) {
                    // If metadata readers could not identify the source ratio
                    // and KDE (or another provider) returned a square preview,
                    // inspect the source bytes before accepting that square as
                    // authoritative. Bound this exceptional fallback read and
                    // reuse the bytes if the full decoder is needed afterward.
                    if source_aspect_ratio.is_none() && native.width() == native.height() {
                        let source_size = fs::metadata(&path).ok().map(|metadata| metadata.len());
                        if source_size.is_some_and(|size| size <= MAX_SOURCE_RATIO_FALLBACK_READ_BYTES) {
                            if let Ok(bytes) = fs::read(&path) {
                                source_dimensions = source_dimensions_from_bytes(&bytes);
                                source_aspect_ratio = source_dimensions
                                    .map(|(width, height)| width as f32 / height as f32);
                                native_request_size = thumbnail_output_limit(target_size, source_dimensions);
                                preloaded_source_bytes = Some(bytes);
                            }
                        }
                    }

                    // Test actual provider resolution before aspect correction.
                    // Synthetic pixels added to the long side must not make a
                    // 128px preview appear to satisfy a 256px request.
                    let native_dimensions_are_trustworthy =
                        source_aspect_ratio.is_some() || native.width() != native.height();
                    let native_is_sufficient = native_dimensions_are_trustworthy
                        && native.width().max(native.height()) >= native_request_size;
                    if native_is_sufficient {
                        let source_aspect_ratio = source_aspect_ratio_or_preview(
                            source_aspect_ratio,
                            native.width(),
                            native.height(),
                        );
                        // The shared pipeline stores a single aspect-correct
                        // texture for every thumbnail consumer. For example,
                        // 128x128 from KDE becomes 228x128 for a 16:9 source.
                        let (width, height, pixels, resolution) = fit_thumbnail_to(
                            native,
                            native_request_size,
                            source_aspect_ratio,
                        )?;
                        return Ok(GeneratedThumbnail { width, height, pixels, resolution, source_aspect_ratio });
                    }
                }

                let bytes = match preloaded_source_bytes {
                    Some(bytes) => bytes,
                    None => fs::read(&path)
                        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?,
                };
                source_dimensions = source_dimensions.or_else(|| source_dimensions_from_bytes(&bytes));
                source_aspect_ratio = source_aspect_ratio
                    .or_else(|| source_dimensions.map(|(width, height)| width as f32 / height as f32));

                // RAW uses its embedded preview/thumbnail path; do not develop
                // the full sensor data merely to populate a Masonry tile.
                let is_raw = path.extension().and_then(|e| e.to_str())
                    .and_then(crate::imaging::core::ImageFormat::from_extension)
                    == Some(crate::imaging::core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);

                if !is_raw {
                    if let Some(embedded) = extract_embedded_metadata_thumbnail(&bytes) {
                        if let Ok(image) = image::load_from_memory(&embedded) {
                            // Check the embedded preview's real resolution before
                            // aspect correction, so synthetic long-side pixels do
                            // not make an undersized preview appear sufficient.
                            let image = image.to_rgba8();
                            let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                            let embedded_dimensions_are_trustworthy =
                                source_aspect_ratio.is_some() || image.width() != image.height();
                            let embedded_is_sufficient = embedded_dimensions_are_trustworthy
                                && image.width().max(image.height()) >= output_limit;
                            if embedded_is_sufficient {
                                let source_aspect_ratio = source_aspect_ratio_or_preview(
                                    source_aspect_ratio,
                                    image.width(),
                                    image.height(),
                                );
                                let (width, height, pixels, resolution) = fit_thumbnail_to(
                                    image,
                                    output_limit,
                                    source_aspect_ratio,
                                )?;
                                return Ok(GeneratedThumbnail { width, height, pixels, resolution, source_aspect_ratio });
                            }
                        }
                    }
                }
                let rgba = decode_checked_sidebar_image(&bytes, &path.display().to_string(), is_raw, Some(&path))?;
                let source_aspect_ratio = source_aspect_ratio_or_preview(
                    source_aspect_ratio,
                    rgba.width(),
                    rgba.height(),
                );
                let output_limit = thumbnail_output_limit(target_size, source_dimensions);
                let (width, height, pixels, resolution) = fit_thumbnail_to(rgba, output_limit, source_aspect_ratio)?;
                Ok(GeneratedThumbnail { width, height, pixels, resolution, source_aspect_ratio })
            })();

            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        match thumbnail_scheduler(self.settings_manager.get().thumbnail_workers).submit(scheduler_key, priority, job) {
            Ok(()) => {}
            Err(_job) => {
                self.sidebar_thumbnail_pending.remove(&job_key);
                self.sidebar_thumbnail_attempted.remove(&job_key);
                self.sidebar_thumbnail_receivers.pop();
            }
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
        self.request_sized_archive_thumbnail(entry, cache_key, target_size, SIDEBAR_THUMBNAIL_PRIORITY, ctx);
    }

    #[allow(unused_assignments)]
    fn request_sized_archive_thumbnail(
        &mut self,
        entry: crate::imaging::entry::ImageEntry,
        key: String,
        target_size: u32,
        priority: u8,
        ctx: &egui::Context,
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
        self.sidebar_thumbnail_receivers.push(rx);
        let job_key = key.clone();
        let scheduler_key = key.clone();
        let repaint_ctx = ctx.clone();
        let job = Box::new(move || {
            let result = (|| -> Result<GeneratedThumbnail, String> {
                use std::io::Read;
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
            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        match thumbnail_scheduler(self.settings_manager.get().thumbnail_workers).submit(scheduler_key, priority, job) {
            Ok(()) => {}
            Err(_job) => {
                self.sidebar_thumbnail_pending.remove(&job_key);
                self.sidebar_thumbnail_attempted.remove(&job_key);
                self.sidebar_thumbnail_receivers.pop();
            }
        }
    }
    pub fn collect_sidebar_thumbnails(&mut self, ctx: &egui::Context) {
        let mut completed = Vec::new();
        self.sidebar_thumbnail_receivers
            .retain(|receiver| match receiver.try_recv() {
                Ok(item) => {
                    completed.push(item);
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
            });
        for (key, result) in completed {
            self.sidebar_thumbnail_pending.remove(&key);
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
                Err(_) => {
                    self.sidebar_thumbnail_failed.insert(key);
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::{
        fit_thumbnail_to, normalize_thumbnail_aspect_ratio, oriented_dimensions,
        source_thumbnail_key, thumbnail_cache_size, thumbnail_fit_rect, thumbnail_output_limit,
        thumbnail_source_key_from_cache_key,
    };
    use eframe::egui;

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
