//! Background generation and collection of sidebar thumbnails.
use super::super::types::ViewerApp;
use eframe::egui;
use std::fs;
use std::path::PathBuf;


/// Decode a sidebar source without ever invoking full RAW sensor development.
/// RAW entries use Rawler's embedded thumbnail API, including archive entries.
fn decode_sidebar_image(bytes: &[u8], name: &str, is_raw: bool, source_path: Option<&std::path::Path>) -> Result<image::RgbaImage, String> {
    if is_raw {
        let thumbnail = crate::decoder::extract_raw_thumbnail(bytes, source_path)
            .ok_or_else(|| format!("No embedded RAW thumbnail found in {name}"))?;
        return image::load_from_memory(&thumbnail)
            .map(|image| image.to_rgba8())
            .map_err(|error| format!("Failed to decode embedded RAW thumbnail in {name}: {error}"));
    }

    let decoded = crate::decoder::decode_bytes(bytes).map_err(|e| e.to_string())?;
    let width = decoded.width();
    let height = decoded.height();
    if width == 0 || height == 0 {
        return Err(format!("Empty image: {name}"));
    }
    image::RgbaImage::from_raw(width, height, decoded.data().to_vec())
        .ok_or_else(|| format!("Invalid decoded image data: {name}"))
}

impl ViewerApp {
    pub(super) fn request_sidebar_thumbnail(&mut self, path: PathBuf, key: String) {
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

        std::thread::spawn(move || {
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
                let rgba = decode_sidebar_image(&bytes, &path.display().to_string(), is_raw, Some(&path))?;

                let source_width = rgba.width();
                let source_height = rgba.height();

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
                let rgba = decode_sidebar_image(&bytes, &name, is_raw, None)?;
                let w = rgba.width();
                let h = rgba.height();
                if w == 0 || h == 0 {
                    return Err("Empty image".into());
                }
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
