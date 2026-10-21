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
        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        self.sidebar_thumbnail_receivers.push(rx);

        std::thread::spawn(move || {
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                let bytes = fs::read(&path)
                    .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

                let decoded = crate::decoder::decode_bytes(&bytes)
                    .map_err(|e| format!("Failed to decode {}: {e}", path.display()))?;

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
