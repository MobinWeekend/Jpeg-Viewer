//! Background generation and collection of sidebar thumbnails.
use super::super::types::ViewerApp;
use eframe::egui;
use std::fs;
use std::path::PathBuf;

impl ViewerApp {
    pub(super) fn request_sidebar_thumbnail(&mut self, path: PathBuf, key: String) {
        if !path.is_file()
            || self.sidebar_thumbnail_attempted.contains(&key)
            || self.sidebar_thumbnail_pending.contains(&key)
        {
            return;
        }

        self.sidebar_thumbnail_attempted.insert(key.clone());

        // Developing a camera RAW file can allocate hundreds of megabytes.
        // The current RAW backend develops the full-resolution image, so doing
        // that again just to create a tiny sidebar thumbnail can compete with
        // the main image load and make loading appear to never finish.
        let is_raw = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(crate::image_core::ImageFormat::from_extension)
            == Some(crate::image_core::ImageFormat::CameraRaw);
        if is_raw {
            self.sidebar_thumbnail_failed.insert(key);
            return;
        }

        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        self.sidebar_thumbnail_receivers.push(rx);

        std::thread::spawn(move || {
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                let bytes = fs::read(&path)
                    .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

                let is_raw = path.extension().and_then(|e| e.to_str())
                    .and_then(crate::image_core::ImageFormat::from_extension)
                    == Some(crate::image_core::ImageFormat::CameraRaw);
                let decoded = if is_raw {
                    match crate::decoder::format_detection::load_bytes_with_detection(
                        bytes, Some(&path), 0, &crate::image_core::DecodeOptions::default(), None,
                    ).map_err(|e| format!("Failed to decode {}: {e}", path.display()))? {
                        crate::app::types::LoadedImage::Static(image) => image,
                        _ => return Err(format!("Could not generate a static RAW preview for {}", path.display())),
                    }
                } else {
                    crate::decoder::decode_bytes(&bytes)
                        .map_err(|e| format!("Failed to decode {}: {e}", path.display()))?
                };

                let source_width = decoded.width();
                let source_height = decoded.height();

                if source_width == 0 || source_height == 0 {
                    return Err(format!(
                        "Invalid source dimensions for {}: {}x{}",
                        path.display(),
                        source_width,
                        source_height
                    ));
                }

                let expected_bytes = (source_width as usize)
                    .checked_mul(source_height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .ok_or_else(|| "Source image dimensions overflow".to_string())?;

                let source_pixels = decoded.data();

                if source_pixels.len() != expected_bytes {
                    return Err(format!(
                        "Invalid RGBA data for {}: dimensions {}x{} require {} bytes, got {}",
                        path.display(),
                        source_width,
                        source_height,
                        expected_bytes,
                        source_pixels.len()
                    ));
                }
                /*
                eprintln!(
                    "[sidebar thumbnail] Source: {} — {}x{}",
                    path.display(),
                    source_width,
                    source_height
                );
                */
                let rgba =
                    image::RgbaImage::from_raw(source_width, source_height, source_pixels.to_vec())
                        .ok_or_else(|| {
                            format!("Could not construct RGBA image for {}", path.display())
                        })?;

                // Fit within a max box while preserving the source aspect ratio.
                // `image::imageops::thumbnail` does NOT preserve AR — it forces exactly
                // the given dimensions — so compute the contained size ourselves.
                const THUMB_MAX_W: u32 = 256;
                const THUMB_MAX_H: u32 = 256;

                let scale = (THUMB_MAX_W as f32 / source_width as f32)
                    .min(THUMB_MAX_H as f32 / source_height as f32);
                let target_w = ((source_width as f32 * scale).round() as u32).max(1);
                let target_h = ((source_height as f32 * scale).round() as u32).max(1);

                let thumbnail = image::imageops::thumbnail(&rgba, target_w, target_h);

                let width = thumbnail.width() as usize;
                let height = thumbnail.height() as usize;

                if width == 0 || height == 0 {
                    return Err(format!(
                        "Thumbnail has invalid dimensions for {}",
                        path.display()
                    ));
                }
                /*
                eprintln!(
                    "[sidebar thumbnail] Result: {} — {}x{}",
                    path.display(),
                    width,
                    height
                );
                 */
                Ok((width, height, thumbnail.into_raw()))
            })();

            let _ = tx.send((key, result));
        });
    }

    #[allow(unused_assignments)]
    pub(crate) fn request_sidebar_archive_thumbnail(
        &mut self,
        entry: crate::image_entry::ImageEntry,
        key: String,
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
        std::thread::spawn(move || {
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
                let decoded = crate::decoder::decode_bytes(&bytes).map_err(|e| e.to_string())?;
                let w = decoded.width();
                let h = decoded.height();
                if w == 0 || h == 0 {
                    return Err("Empty image".into());
                }
                let rgba = image::RgbaImage::from_raw(w, h, decoded.data().to_vec())
                    .ok_or_else(|| "Invalid decoded image data".to_string())?;
                let scale = (256.0 / w as f32).min(256.0 / h as f32);
                let tw = ((w as f32 * scale).round() as u32).max(1);
                let th = ((h as f32 * scale).round() as u32).max(1);
                let thumb = image::imageops::thumbnail(&rgba, tw, th);
                Ok((
                    thumb.width() as usize,
                    thumb.height() as usize,
                    thumb.into_raw(),
                ))
            })();
            let _ = tx.send((key, result));
        });
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
                    self.sidebar_thumbnails.insert(key, texture);
                }
                Err(_) => {
                    self.sidebar_thumbnail_failed.insert(key);
                }
            }
        }
    }
}
