//! Background generation and collection of sidebar thumbnails.
use super::super::types::ViewerApp;
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::sync::{mpsc::{self, SyncSender, TrySendError}, Arc, Mutex, OnceLock};

type ThumbnailJob = Box<dyn FnOnce() + Send + 'static>;

/// A small dedicated pool keeps thumbnail work from flooding the CPU and avoids
/// creating one OS thread per visible image. The bounded queue applies backpressure.
fn thumbnail_job_sender(worker_count: usize) -> &'static SyncSender<ThumbnailJob> {
    static SENDER: OnceLock<SyncSender<ThumbnailJob>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<ThumbnailJob>(32);
        let receiver = Arc::new(Mutex::new(receiver));
        for worker_index in 0..worker_count.clamp(1, 8) {
            let receiver = Arc::clone(&receiver);
            let _ = std::thread::Builder::new()
                .name(format!("thumbnail-worker-{worker_index}"))
                .spawn(move || loop {
                    let job = receiver.lock().expect("thumbnail receiver poisoned").recv();
                    match job { Ok(job) => job(), Err(_) => break }
                });
        }
        sender
    })
}

const THUMB_MAX_SIDE: u32 = 128;
/// Reject suspiciously large decoded sources before asking a decoder to allocate
/// a full-resolution RGBA buffer. This is a safety ceiling, not a target size.
// Permit large photographic sources while keeping worst-case RGBA allocation bounded.
// The thumbnail is immediately reduced to 128 px after decode.
const MAX_THUMB_SOURCE_PIXELS: u64 = 100_000_000;

fn fit_thumbnail(rgba: image::RgbaImage) -> Result<(usize, usize, Vec<u8>), String> {
    let source_width = rgba.width();
    let source_height = rgba.height();
    if source_width == 0 || source_height == 0 {
        return Err("Empty image".into());
    }
    let scale = (THUMB_MAX_SIDE as f64 / source_width as f64)
        .min(THUMB_MAX_SIDE as f64 / source_height as f64)
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
    let decoded = registry.decode(bytes, format, &crate::image_core::DecodeOptions::default())
        .map_err(|e| e.to_string())?;
    image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.into_data())
        .ok_or_else(|| format!("Invalid decoded image data: {name}"))
}



impl ViewerApp {
    pub(super) fn request_sidebar_thumbnail(&mut self, path: PathBuf, key: String, ctx: &egui::Context) {
        if !path.is_file()
            || self.sidebar_thumbnail_attempted.contains(&key)
            || self.sidebar_thumbnail_pending.contains(&key)
        {
            return;
        }

        self.sidebar_thumbnail_attempted.insert(key.clone());

        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        self.sidebar_thumbnail_receivers.push(rx);

        let job_key = key.clone();
        let repaint_ctx = ctx.clone();
        let job = Box::new(move || {
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                let bytes = fs::read(&path)
                    .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

                // Detect RAW by extension and file content. Use Rawler's dedicated
                // thumbnail API for sidebar cards; never develop the full sensor data
                // just to populate a small thumbnail.
                let is_raw = path.extension().and_then(|e| e.to_str())
                    .and_then(crate::image_core::ImageFormat::from_extension)
                    == Some(crate::image_core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);

                // RAW already uses its embedded preview/thumbnail path. For other
                // formats, try an EXIF thumbnail first so huge source images don't
                // need a full-resolution decode just to populate the sidebar.
                if !is_raw {
                    if let Some(embedded) = extract_embedded_metadata_thumbnail(&bytes) {
                        if let Ok(image) = image::load_from_memory(&embedded) {
                            return fit_thumbnail(image.to_rgba8());
                        }
                    }
                }
                let rgba = decode_checked_sidebar_image(&bytes, &path.display().to_string(), is_raw, Some(&path))?;
                fit_thumbnail(rgba)
            })();

            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        match thumbnail_job_sender(self.settings_manager.get().thumbnail_workers).try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.sidebar_thumbnail_pending.remove(&job_key);
                self.sidebar_thumbnail_attempted.remove(&job_key);
                self.sidebar_thumbnail_receivers.pop();
            }
        }
    }

    #[allow(unused_assignments)]
    pub(crate) fn request_sidebar_archive_thumbnail(
        &mut self,
        entry: crate::image_entry::ImageEntry,
        key: String,
        ctx: &egui::Context,
    ) {
        if self.sidebar_thumbnail_attempted.contains(&key)
            || self.sidebar_thumbnail_pending.contains(&key)
        {
            return;
        }
        self.sidebar_thumbnail_attempted.insert(key.clone());
        self.sidebar_thumbnail_pending.insert(key.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.sidebar_thumbnail_receivers.push(rx);
        let job_key = key.clone();
        let repaint_ctx = ctx.clone();
        let job = Box::new(move || {
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                use std::io::Read;
                let bytes = match &entry {
                    crate::image_entry::ImageEntry::File(path) => {
                        std::fs::read(path).map_err(|e| e.to_string())?
                    }
                    crate::image_entry::ImageEntry::Zip(item) => {
                        let file =
                            std::fs::File::open(&item.archive_path).map_err(|e| e.to_string())?;
                        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                        let mut item_file = archive
                            .by_index(item.entry_index)
                            .map_err(|e| e.to_string())?;
                        let mut bytes = Vec::new();
                        item_file
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        bytes
                    }
                    crate::image_entry::ImageEntry::S7z(item) => {
                        let mut archive = sevenz_rust2::ArchiveReader::open(
                            &item.archive_path,
                            sevenz_rust2::Password::empty(),
                        )
                        .map_err(|e| e.to_string())?;
                        archive.read_file(&item.name).map_err(|e| e.to_string())?
                    }
                    crate::image_entry::ImageEntry::Rar(item) => {
                        let mut archive = unrar::Archive::new(&item.archive_path)
                            .open_for_processing()
                            .map_err(|e| e.to_string())?;
                        loop {
                            let header = archive
                                .read_header()
                                .map_err(|e| e.to_string())?
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
                    crate::image_entry::ImageEntry::File(path) => path.display().to_string(),
                    crate::image_entry::ImageEntry::Zip(item) => item.name.clone(),
                    crate::image_entry::ImageEntry::S7z(item) => item.name.clone(),
                    crate::image_entry::ImageEntry::Rar(item) => item.name.clone(),
                };
                let is_raw = std::path::Path::new(&name)
                    .extension().and_then(|e| e.to_str())
                    .and_then(crate::image_core::ImageFormat::from_extension)
                    == Some(crate::image_core::ImageFormat::CameraRaw)
                    || zenraw::is_raw_file(&bytes);
                let rgba = decode_checked_sidebar_image(&bytes, &name, is_raw, None)?;
                fit_thumbnail(rgba)
            })();
            let _ = tx.send((key, result));
            repaint_ctx.request_repaint();
        }) as ThumbnailJob;
        match thumbnail_job_sender(self.settings_manager.get().thumbnail_workers).try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
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
                    // Keep the texture cache within its ~16 MiB worst-case pixel
                    // budget (256 thumbnails × 128 × 128 × 4 bytes). Explicitly
                    // observe evictions so an evicted item can be requested again.
                    if self.sidebar_thumbnails.len() >= 256 {
                        if let Some((evicted_key, _texture)) = self.sidebar_thumbnails.pop_lru() {
                            self.sidebar_thumbnail_attempted.remove(&evicted_key);
                        }
                    }
                    self.sidebar_thumbnails.put(key, texture);
                }
                Err(_) => {
                    self.sidebar_thumbnail_failed.insert(key);
                }
            }
        }
    }
}
