//! Progressive, CPU-prepared mipmaps for the Glow renderer.
//!
//! The base image remains an ordinary egui-managed texture. Lower mip levels are
//! generated on a worker thread and uploaded from a Glow paint callback in small,
//! shared per-frame batches. This deliberately keeps `TextureHandle` ownership
//! and texture lifetime under egui while avoiding `glGenerateMipmap` in the
//! renderer's texture-upload path.

use crate::app::types::LoadedImage;
use crate::imaging::entry::ImageEntry;
use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Shared app-wide limit. Every visible texture and virtual tile consumes from
/// the same budget, so several callbacks cannot each upload a full allowance.
pub(crate) const MAX_MIP_UPLOAD_BYTES_PER_FRAME: usize = 4 * 1024 * 1024;
const WORKER_CHANNEL_CAPACITY: usize = 1;
const REPAINT_WHEN_WAITING: Duration = Duration::from_millis(32);
const REPAINT_WHEN_UPLOADING: Duration = Duration::from_millis(16);

pub(crate) type MipUploadBudget = Arc<AtomicUsize>;
pub(crate) type SharedMipmapState = Arc<Mutex<MipmapUploadState>>;

struct MipLevel {
    level: i32,
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

struct PendingMipLevel {
    mip: MipLevel,
    next_row: u32,
    allocated: bool,
}

/// State is shared between the app/cache and a lightweight Glow callback.
/// Pixel data is streamed one level at a time; the full mip chain is never kept
/// in RAM alongside the source image.
pub(crate) struct MipmapUploadState {
    receiver: Receiver<MipLevel>,
    pending: Option<PendingMipLevel>,
    last_uploaded_level: i32,
    ready: bool,
}

impl MipmapUploadState {
    fn new(receiver: Receiver<MipLevel>) -> Self {
        Self {
            receiver,
            pending: None,
            last_uploaded_level: 0,
            ready: false,
        }
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }
}

/// Whether a mipmap task still has CPU preparation or GPU upload work pending.
pub(crate) fn is_mipmap_state_pending(state: &SharedMipmapState) -> bool {
    state
        .lock()
        .map(|state| !state.is_ready())
        .unwrap_or(false)
}

/// Start preparing mip levels from pixels that are already decoded.
pub(crate) fn start_mipmap_upload(
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
) -> SharedMipmapState {
    let (sender, receiver) = mpsc::sync_channel(WORKER_CHANNEL_CAPACITY);
    if let Err(error) = thread::Builder::new()
        .name("jpeg-viewer-mipmap".to_owned())
        .spawn(move || generate_mip_chain(width, height, pixels, sender))
    {
        // Dropping the failed worker closure also drops its sender. The paint
        // callback will observe a disconnected channel and keep linear filtering.
        eprintln!("Could not start mipmap preparation worker: {error}");
    }
    Arc::new(Mutex::new(MipmapUploadState::new(receiver)))
}

/// A preloaded texture intentionally does not retain a second full decoded
/// image in the GPU cache. When an idle cached-neighbour slot is available,
/// reopen and decode the source off-thread, then stream only its mip levels.
pub(crate) fn start_mipmap_upload_from_entry(
    entry: ImageEntry,
    virtual_texture_threshold: u32,
) -> SharedMipmapState {
    let (sender, receiver) = mpsc::sync_channel(WORKER_CHANNEL_CAPACITY);
    if let Err(error) = thread::Builder::new()
        .name("jpeg-viewer-mipmap-decode".to_owned())
        .spawn(move || {
            match crate::app::loading::load_entry_content(
                entry,
                virtual_texture_threshold,
                false,
            ) {
                Ok(LoadedImage::Static(image)) => {
                    generate_mip_chain(
                        image.width,
                        image.height,
                        Arc::new(image.data),
                        sender,
                    );
                }
                // GIF and virtual-texture entries manage their own frame/tile
                // resources and must not create a full-image mip chain here.
                Ok(LoadedImage::Animated(_, _) | LoadedImage::VirtualPending(_, _, _)) => {}
                Err(error) => {
                    eprintln!("Could not prepare cached image mipmaps: {error}");
                }
            }
        })
    {
        eprintln!("Could not start cached-image mipmap worker: {error}");
    }
    Arc::new(Mutex::new(MipmapUploadState::new(receiver)))
}

fn generate_mip_chain(
    mut width: u32,
    mut height: u32,
    mut source: Arc<Vec<u8>>,
    sender: SyncSender<MipLevel>,
) {
    let mut level = 1_i32;
    let mut source_is_premultiplied = false;
    while width > 1 || height > 1 {
        let Some((next_width, next_height, next_pixels)) = downsample_rgba_egui_premultiplied(
            width,
            height,
            source.as_slice(),
            source_is_premultiplied,
        ) else {
            eprintln!("Skipping mipmap generation because the decoded RGBA buffer is invalid");
            return;
        };
        let next_pixels = Arc::new(next_pixels);
        let mip = MipLevel {
            level,
            width: next_width,
            height: next_height,
            pixels: Arc::clone(&next_pixels),
        };
        if sender.send(mip).is_err() {
            // The cache entry/texture was evicted or replaced; stop promptly.
            return;
        }
        source = next_pixels;
        source_is_premultiplied = true;
        width = next_width;
        height = next_height;
        level += 1;
    }
}

/// Generate one conventional floor-halved mip level in the same representation
/// egui uploads: gamma-encoded, premultiplied RGBA8. The first source level is
/// ordinary decoded RGBA and is converted with Color32's exact premultiplication;
/// subsequent levels are already premultiplied. This is important because egui's
/// Glow renderer uploads ColorImage::pixels directly as premultiplied Color32.
fn downsample_rgba_egui_premultiplied(
    source_width: u32,
    source_height: u32,
    source: &[u8],
    source_is_premultiplied: bool,
) -> Option<(u32, u32, Vec<u8>)> {
    if source_width == 0 || source_height == 0 {
        return None;
    }
    let expected = (source_width as usize)
        .checked_mul(source_height as usize)?
        .checked_mul(4)?;
    if source.len() != expected {
        return None;
    }

    let width = (source_width / 2).max(1);
    let height = (source_height / 2).max(1);
    let output_len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let mut output = vec![0_u8; output_len];

    for y in 0..height {
        let y0 = (y as u64 * source_height as u64 / height as u64) as u32;
        let y1 = (((y + 1) as u64 * source_height as u64 / height as u64) as u32)
            .max(y0 + 1)
            .min(source_height);
        for x in 0..width {
            let x0 = (x as u64 * source_width as u64 / width as u64) as u32;
            let x1 = (((x + 1) as u64 * source_width as u64 / width as u64) as u32)
                .max(x0 + 1)
                .min(source_width);

            let mut channel_sums = [0_u64; 4];
            let mut samples = 0_u64;
            for source_y in y0..y1 {
                for source_x in x0..x1 {
                    let offset = ((source_y as usize * source_width as usize)
                        + source_x as usize)
                        * 4;
                    let rgba = if source_is_premultiplied {
                        [
                            source[offset],
                            source[offset + 1],
                            source[offset + 2],
                            source[offset + 3],
                        ]
                    } else {
                        egui::Color32::from_rgba_unmultiplied(
                            source[offset],
                            source[offset + 1],
                            source[offset + 2],
                            source[offset + 3],
                        )
                        .to_array()
                    };
                    for (sum, channel) in channel_sums.iter_mut().zip(rgba) {
                        *sum += u64::from(channel);
                    }
                    samples += 1;
                }
            }

            let target = ((y as usize * width as usize) + x as usize) * 4;
            for (channel, sum) in channel_sums.into_iter().enumerate() {
                output[target + channel] = ((sum + samples / 2) / samples) as u8;
            }
        }
    }

    Some((width, height, output))
}


/// Append a callback after painting the texture. egui/Glow has already applied
/// its base texture update by this point, so the callback can add mip levels to
/// that same managed texture and leave the TextureHandle/cache intact.
pub(crate) fn append_mipmap_upload_callback(
    painter: &egui::Painter,
    rect: egui::Rect,
    texture_id: egui::TextureId,
    state: SharedMipmapState,
    budget: MipUploadBudget,
    ctx: egui::Context,
) {
    let still_working = state
        .lock()
        .map(|state| !state.is_ready())
        .unwrap_or(false);
    if !still_working || !rect.is_positive() {
        return;
    }

    let callback = egui_glow::CallbackFn::new(move |_info, glow_painter| {
        upload_next_mipmap_chunk(
            glow_painter,
            texture_id,
            &state,
            &budget,
            &ctx,
        );
    });
    painter.add(egui::Shape::Callback(egui::PaintCallback {
        rect,
        callback: Arc::new(callback),
    }));
}

/// Atomically subtract `bytes` from the shared per-frame upload allowance.
/// Returns false without changing the budget when there is not enough room.
fn reserve_upload_budget(budget: &MipUploadBudget, bytes: usize) -> bool {
    let mut remaining = budget.load(Ordering::Relaxed);
    loop {
        let Some(updated) = remaining.checked_sub(bytes) else {
            return false;
        };
        match budget.compare_exchange_weak(
            remaining,
            updated,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(actual) => remaining = actual,
        }
    }
}

fn upload_next_mipmap_chunk(
    painter: &egui_glow::Painter,
    texture_id: egui::TextureId,
    shared_state: &SharedMipmapState,
    budget: &MipUploadBudget,
    ctx: &egui::Context,
) {
    let Some(texture) = painter.texture(texture_id) else {
        ctx.request_repaint_after(REPAINT_WHEN_WAITING);
        return;
    };
    let Ok(mut state) = shared_state.lock() else {
        return;
    };
    if state.ready {
        return;
    }

    if state.pending.is_none() {
        match state.receiver.try_recv() {
            Ok(mip) => {
                state.pending = Some(PendingMipLevel {
                    mip,
                    next_row: 0,
                    allocated: false,
                });
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(REPAINT_WHEN_WAITING);
                return;
            }
            Err(TryRecvError::Disconnected) => {
                // Every generated level has arrived and been uploaded. Only now
                // turn on trilinear filtering; until then the base texture uses
                // ordinary linear filtering and remains visible immediately.
                let gl = painter.gl();
                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAX_LEVEL,
                        state.last_uploaded_level,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR_MIPMAP_LINEAR as i32,
                    );
                    gl.bind_texture(glow::TEXTURE_2D, None);
                }
                state.ready = true;
                ctx.request_repaint();
                return;
            }
        }
    }

    let Some(mut pending) = state.pending.take() else {
        return;
    };
    let row_bytes = (pending.mip.width as usize).saturating_mul(4);
    let available_bytes = budget.load(Ordering::Relaxed);
    if row_bytes == 0 || available_bytes < row_bytes {
        state.pending = Some(pending);
        ctx.request_repaint_after(REPAINT_WHEN_UPLOADING);
        return;
    }

    let rows_by_budget = (available_bytes / row_bytes).max(1);
    let remaining_rows = (pending.mip.height - pending.next_row) as usize;
    let rows = rows_by_budget.min(remaining_rows);
    let upload_bytes = rows.saturating_mul(row_bytes);
    let start = pending.next_row as usize * row_bytes;
    let end = start.saturating_add(upload_bytes);
    let Some(pixel_slice) = pending.mip.pixels.get(start..end) else {
        // A malformed worker result should not crash the renderer or leave a
        // permanently scheduled callback in the UI.
        state.ready = true;
        ctx.request_repaint();
        return;
    };

    // Reserve the upload budget atomically before touching the GL texture. The
    // compare-exchange loop avoids relying on AtomicUsize update APIs whose
    // names have changed across recent Rust releases.
    if !reserve_upload_budget(budget, upload_bytes) {
        state.pending = Some(pending);
        ctx.request_repaint_after(REPAINT_WHEN_UPLOADING);
        return;
    }

    let gl = painter.gl();
    unsafe {
        let previous_unpack_alignment = gl.get_parameter_i32(glow::UNPACK_ALIGNMENT);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        if !pending.allocated {
            // egui_glow 0.33.3 creates desktop managed color textures as
            // RGBA8 (its `srgb_textures` setting is false). Use that same
            // format for every manually allocated mip level.
            let internal_format = glow::RGBA8 as i32;
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                pending.mip.level,
                internal_format,
                pending.mip.width as i32,
                pending.mip.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            pending.allocated = true;
        }
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            pending.mip.level,
            0,
            pending.next_row as i32,
            pending.mip.width as i32,
            rows as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixel_slice)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, previous_unpack_alignment);
    }

    pending.next_row += rows as u32;
    if pending.next_row >= pending.mip.height {
        state.last_uploaded_level = pending.mip.level;
    } else {
        state.pending = Some(pending);
    }
    ctx.request_repaint_after(REPAINT_WHEN_UPLOADING);
}

#[cfg(test)]
mod tests {
    use super::downsample_rgba_egui_premultiplied;

    #[test]
    fn mip_dimensions_follow_floor_halving_until_one_by_one() {
        let mut width = 5;
        let mut height = 3;
        let mut pixels = vec![128_u8; width * height * 4];
        let mut dimensions = Vec::new();
        while width > 1 || height > 1 {
            let (next_width, next_height, next_pixels) =
                downsample_rgba_egui_premultiplied(width as u32, height as u32, &pixels, false)
                    .expect("valid source should produce a mip level");
            dimensions.push((next_width, next_height));
            width = next_width as usize;
            height = next_height as usize;
            pixels = next_pixels;
        }
        assert_eq!(dimensions, vec![(2, 1), (1, 1)]);
    }

    #[test]
    fn mip_generation_rejects_invalid_rgba_lengths() {
        assert!(downsample_rgba_egui_premultiplied(2, 2, &[0; 12], false).is_none());
    }

    #[test]
    fn odd_source_edges_are_included_in_the_last_mip_sample() {
        let pixels = vec![
            0, 0, 0, 255,
            0, 0, 0, 255,
            255, 255, 255, 255,
        ];
        let (width, height, mip) =
            downsample_rgba_egui_premultiplied(3, 1, &pixels, false).unwrap();
        assert_eq!((width, height), (1, 1));
        // All three source pixels contribute to the gamma-space average.
        assert_eq!(mip[0], 85);
        assert_eq!(mip[3], 255);
    }

    #[test]
    fn transparent_source_pixels_are_premultiplied_like_egui_color32() {
        // Transparent red must not leak into the mip beside opaque blue.
        let pixels = vec![255, 0, 0, 0, 0, 0, 255, 255];
        let (width, height, mip) =
            downsample_rgba_egui_premultiplied(2, 1, &pixels, false).unwrap();
        assert_eq!((width, height), (1, 1));
        assert_eq!(mip, vec![0, 0, 128, 128]);
    }
}
