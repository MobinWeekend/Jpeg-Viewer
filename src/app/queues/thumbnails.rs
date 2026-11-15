//! Background generation and collection of sidebar thumbnails.
use super::super::types::ViewerApp;
use eframe::egui;
use std::fs;
use std::path::PathBuf;
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

const THUMB_MAX_SIDE: u32 = 128;

fn shared_thumbnail_key(key: &str, size: u32) -> String {
    format!("thumb:{}:{}", size.clamp(64, 512), key)
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
// The thumbnail is immediately reduced to 128 px after decode.
const MAX_THUMB_SOURCE_PIXELS: u64 = 100_000_000;

fn fit_thumbnail_to(rgba: image::RgbaImage, max_side: u32) -> Result<(usize, usize, Vec<u8>), String> {
    let source_width = rgba.width();
    let source_height = rgba.height();
    let max_side = max_side.clamp(64, 512);
    if source_width == 0 || source_height == 0 {
        return Err("Empty image".into());
    }
    let scale = (max_side as f64 / source_width as f64)
        .min(max_side as f64 / source_height as f64)
        .min(1.0);
    let target_w = ((source_width as f64 * scale).round() as u32).max(1);
    let target_h = ((source_height as f64 * scale).round() as u32).max(1);
    // Triangle is a fast downscaling filter; Lanczos is unnecessary for tiny previews.
    let thumbnail = image::imageops::resize(
        &rgba, target_w, target_h, image::imageops::FilterType::Triangle,
    );
    Ok((thumbnail.width() as usize, thumbnail.height() as usize, thumbnail.into_raw()))
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



impl ViewerApp {
    pub(crate) fn request_sidebar_thumbnail(&mut self, path: PathBuf, key: String, ctx: &egui::Context) {
        let cache_key = shared_thumbnail_key(&key, THUMB_MAX_SIDE);
        self.request_sized_sidebar_thumbnail(path, cache_key, THUMB_MAX_SIDE, SIDEBAR_THUMBNAIL_PRIORITY, ctx);
    }

    /// Resolve the best cached resolution for a shared key. Prefer the smallest
    /// texture at or above the requested size, then fall back to the largest
    /// lower-resolution preview while an upgrade is generated.
    pub(crate) fn get_shared_thumbnail(
        &mut self,
        key: &str,
        requested_size: u32,
    ) -> Option<egui::TextureHandle> {
        let requested_size = requested_size.clamp(64, 512);
        let sizes = [64_u32, 128, 256, 512];
        let mut candidates = sizes
            .iter()
            .copied()
            .filter(|size| *size >= requested_size)
            .collect::<Vec<_>>();
        candidates.extend(
            sizes.iter().copied().filter(|size| *size < requested_size).rev(),
        );
        for size in candidates {
            let cache_key = shared_thumbnail_key(key, size);
            if let Some(texture) = self.sidebar_thumbnails.get(&cache_key).cloned() {
                return Some(texture);
            }
        }
        // Read older cache keys during an in-session transition; new requests
        // always use the shared resolution-qualified key format.
        self.sidebar_thumbnails.get(key).cloned()
    }

    pub(crate) fn has_shared_thumbnail_at_least(&mut self, key: &str, requested_size: u32) -> bool {
        let requested_size = requested_size.clamp(64, 512);
        [64_u32, 128, 256, 512]
            .into_iter()
            .filter(|size| *size >= requested_size)
            .any(|size| self.sidebar_thumbnails.get(&shared_thumbnail_key(key, size)).is_some())
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
        let target_size = target_size.clamp(64, 512);
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
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                // Ask the operating system for its existing/generated thumbnail
                // before reading and decoding the full source image. This remains
                // on the shared worker pool; a native-shell failure falls through
                // to JPEG Viewer's embedded-preview and decoder path below.
                if let Some(native) = crate::platform::thumbnails::load_or_request(&path, target_size) {
                    return fit_thumbnail_to(native, target_size);
                }

                let bytes = fs::read(&path)
                    .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

                // RAW uses its embedded preview/thumbnail path; do not develop
                // the full sensor data merely to populate a Masonry tile.
                let is_raw = path.extension().and_then(|e| e.to_str())
                    .and_then(crate::imaging::core::ImageFormat::from_extension)
                    == Some(crate::imaging::core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);

                if !is_raw {
                    if let Some(embedded) = extract_embedded_metadata_thumbnail(&bytes) {
                        if let Ok(image) = image::load_from_memory(&embedded) {
                            return fit_thumbnail_to(image.to_rgba8(), target_size);
                        }
                    }
                }
                let rgba = decode_checked_sidebar_image(&bytes, &path.display().to_string(), is_raw, Some(&path))?;
                fit_thumbnail_to(rgba, target_size)
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
        ctx: &egui::Context,
    ) {
        let cache_key = shared_thumbnail_key(&key, THUMB_MAX_SIDE);
        self.request_sized_archive_thumbnail(entry, cache_key, THUMB_MAX_SIDE, SIDEBAR_THUMBNAIL_PRIORITY, ctx);
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
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
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
                let rgba = decode_checked_sidebar_image(&bytes, &name, is_raw, None)?;
                fit_thumbnail_to(rgba, target_size)
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
                Ok((width, height, pixels)) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
                    let texture = ctx.load_texture(
                        format!("sidebar_thumb_{key}"),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    // Enforce the shared cache's entry cap; a second pass below
                    // enforces its byte budget because Masonry stores 64–512px
                    // previews in this same LRU cache.
                    if self.sidebar_thumbnails.len() >= 256 {
                        if let Some((evicted_key, _texture)) = self.sidebar_thumbnails.pop_lru() {
                            self.sidebar_thumbnail_attempted.remove(&evicted_key);
                        }
                    }
                    self.sidebar_thumbnails.put(key, texture);

                    // Masonry can request 256/512px tiles, so bound the shared
                    // sidebar/Masonry cache by GPU texture bytes as well as count.
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
                            self.sidebar_thumbnail_attempted.remove(&evicted_key);
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
