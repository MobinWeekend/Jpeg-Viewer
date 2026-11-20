// src/app/virtual_texture.rs
//! Virtual texture manager – uses `DecodedImage` (plain RGBA8) and generates tiles on demand.

use crate::app::constants::MAX_TILE_SIZE;
use crate::app::texture_mipmap::{
    SharedMipmapState, append_mipmap_upload_callback, start_mipmap_upload,
};
use crate::imaging::core::DecodedImage;
use eframe::egui;
use rayon::spawn;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

// Atomic state constants
const STATE_NOT_STARTED: u8 = 0;
const STATE_PREPARING: u8 = 1;
const STATE_COMPLETE: u8 = 2;

/// Snapshot of preparation progress.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct PreparationProgress {
    pub total_tiles: usize,
    pub prepared_tiles: usize,
    pub is_preparing: bool,
    pub is_complete: bool,
}

impl Default for PreparationProgress {
    fn default() -> Self {
        Self {
            total_tiles: 0,
            prepared_tiles: 0,
            is_preparing: false,
            is_complete: false,
        }
    }
}

/// Metadata for one virtual tile. Pixel data stays in the source image buffer
/// until this tile is actually visible and needs to be uploaded to the GPU.
struct Tile {
    grid_x: u32,
    grid_y: u32,
    width: u32,
    height: u32,
    texture: Option<egui::TextureHandle>,
    mipmap_state: Option<SharedMipmapState>,
    pending: Option<Receiver<Result<Vec<u8>, String>>>,
    failed: bool,
    last_used_frame: u64,
}

/// Virtual texture manager – holds the full image and generates tiles on demand.
pub struct VirtualTexture {
    id: usize,
    data: Arc<Vec<u8>>, // one shared RGBA8 source buffer; tile copies are temporary
    width: u32,
    height: u32,
    tile_size: u32,
    tiles_x: usize,
    tiles_y: usize,
    total_tiles: usize,
    tiles: Vec<Tile>,
    prepared_tiles: AtomicUsize,
    state: AtomicU8,
    frame_index: u64,
}

impl VirtualTexture {
    pub fn new(img: DecodedImage, tile_size: u32) -> Self {
        assert!(
            tile_size > 0 && tile_size <= MAX_TILE_SIZE,
            "Invalid virtual texture tile size"
        );

        let width = img.width();
        let height = img.height();
        let data = Arc::new(img.into_data()); // one shared source buffer for asynchronous tile extraction

        let tiles_x = (width as usize).div_ceil(tile_size as usize);
        let tiles_y = (height as usize).div_ceil(tile_size as usize);
        let total_tiles = tiles_x * tiles_y;

        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            data,
            width,
            height,
            tile_size,
            tiles_x,
            tiles_y,
            total_tiles,
            tiles: Vec::new(),
            prepared_tiles: AtomicUsize::new(0),
            state: AtomicU8::new(STATE_NOT_STARTED),
            frame_index: 0,
        }
    }

    /// Build lightweight tile metadata without copying the source pixels.
    /// Actual tile extraction and GPU upload are deferred until render() finds
    /// a tile visible (or within its small prefetch margin).
    pub fn prepare(&mut self) {
        if self.state.load(Ordering::Acquire) == STATE_COMPLETE {
            return;
        }

        self.prepared_tiles.store(0, Ordering::Relaxed);
        self.state.store(STATE_PREPARING, Ordering::Release);

        let capacity = self.total_tiles;
        let mut tiles = Vec::with_capacity(capacity);
        for ty in 0..self.tiles_y {
            for tx in 0..self.tiles_x {
                let x = (tx as u64 * u64::from(self.tile_size)) as u32;
                let y = (ty as u64 * u64::from(self.tile_size)) as u32;
                let tile_width = self.tile_size.min(self.width.saturating_sub(x));
                let tile_height = self.tile_size.min(self.height.saturating_sub(y));
                if tile_width == 0 || tile_height == 0 {
                    continue;
                }
                tiles.push(Tile {
                    grid_x: tx as u32,
                    grid_y: ty as u32,
                    width: tile_width,
                    height: tile_height,
                    texture: None,
                    mipmap_state: None,
                    pending: None,
                    failed: false,
                    last_used_frame: 0,
                });
            }
        }

        self.tiles = tiles;
        self.prepared_tiles
            .store(self.tiles.len(), Ordering::Relaxed);
        self.state.store(STATE_COMPLETE, Ordering::Release);
    }

    /// Returns a snapshot of the current preparation progress.
    pub fn progress(&self) -> PreparationProgress {
        PreparationProgress {
            total_tiles: self.total_tiles,
            prepared_tiles: self.prepared_tiles.load(Ordering::Relaxed),
            is_preparing: self.state.load(Ordering::Acquire) == STATE_PREPARING,
            is_complete: self.state.load(Ordering::Acquire) == STATE_COMPLETE,
        }
    }

    /// Check if the virtual texture is ready.
    pub fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_COMPLETE
    }

    /// Total number of tiles.
    pub fn total_tiles(&self) -> usize {
        self.total_tiles
    }

    /// Number of prepared tiles.
    pub fn prepared_tiles_count(&self) -> usize {
        self.prepared_tiles.load(Ordering::Relaxed)
    }

    /// Schedule one visible tile for background extraction. The image source
    /// buffer is shared, and only the requested tile's temporary pixel buffer
    /// is allocated. GPU texture creation remains on the UI thread.
    fn request_tile(
        tile: &mut Tile,
        source: Arc<Vec<u8>>,
        source_width: u32,
        tile_size: u32,
        ctx: &egui::Context,
    ) {
        if tile.texture.is_some() || tile.pending.is_some() || tile.failed {
            return;
        }

        let tile_x = tile.grid_x;
        let tile_y = tile.grid_y;
        let width = tile.width as usize;
        let height = tile.height as usize;
        let source_width = source_width as usize;
        let tile_size = tile_size as usize;
        let (sender, receiver) = mpsc::channel();
        tile.pending = Some(receiver);
        let repaint = ctx.clone();

        spawn(move || {
            let result = (|| -> Result<Vec<u8>, String> {
                let x = (tile_x as usize)
                    .checked_mul(tile_size)
                    .ok_or_else(|| "Virtual tile X offset overflowed.".to_owned())?;
                let y = (tile_y as usize)
                    .checked_mul(tile_size)
                    .ok_or_else(|| "Virtual tile Y offset overflowed.".to_owned())?;
                let row_bytes = width
                    .checked_mul(4)
                    .ok_or_else(|| "Virtual tile row size overflowed.".to_owned())?;
                let capacity = row_bytes
                    .checked_mul(height)
                    .ok_or_else(|| "Virtual tile allocation size overflowed.".to_owned())?;
                let mut pixels = Vec::with_capacity(capacity);

                for row in 0..height {
                    let source_row = y
                        .checked_add(row)
                        .ok_or_else(|| "Virtual tile row offset overflowed.".to_owned())?;
                    let pixel_offset = source_row
                        .checked_mul(source_width)
                        .and_then(|value| value.checked_add(x))
                        .ok_or_else(|| "Virtual tile pixel offset overflowed.".to_owned())?;
                    let start = pixel_offset
                        .checked_mul(4)
                        .ok_or_else(|| "Virtual tile byte offset overflowed.".to_owned())?;
                    let end = start
                        .checked_add(row_bytes)
                        .ok_or_else(|| "Virtual tile byte range overflowed.".to_owned())?;
                    let row_data = source.get(start..end).ok_or_else(|| {
                        "Virtual tile falls outside the decoded pixel buffer.".to_owned()
                    })?;
                    pixels.extend_from_slice(row_data);
                }
                Ok(pixels)
            })();
            let _ = sender.send(result);
            repaint.request_repaint();
        });
    }

    /// Poll one tile worker and upload its completed pixels through egui.
    fn collect_tile(
        tile: &mut Tile,
        ctx: &egui::Context,
        options: egui::TextureOptions,
        id: usize,
    ) {
        let Some(receiver) = tile.pending.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(pixels)) => {
                let pixels = Arc::new(pixels);
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [tile.width as usize, tile.height as usize],
                    pixels.as_slice(),
                );
                let wants_mipmaps = options.mipmap_mode.is_some();
                let mut base_options = options;
                base_options.mipmap_mode = None;
                tile.texture = Some(ctx.load_texture(
                    format!("vt_{}_tile_{}_{}", id, tile.grid_x, tile.grid_y),
                    color_image,
                    base_options,
                ));
                tile.mipmap_state = if wants_mipmaps {
                    Some(start_mipmap_upload(
                        tile.width,
                        tile.height,
                        Arc::clone(&pixels),
                    ))
                } else {
                    None
                };
            }
            Ok(Err(_error)) => tile.failed = true,
            Err(TryRecvError::Empty) => tile.pending = Some(receiver),
            Err(TryRecvError::Disconnected) => tile.failed = true,
        }
    }

    /// Render the visible tiles using the given painter.
    ///
    /// Adds a 1‑tile prefetch margin to reduce stutter during panning.
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        zoom: f32,
        pan: egui::Vec2,
        rect: egui::Rect,
        options: egui::TextureOptions,
        mipmap_upload_budget: Arc<AtomicUsize>,
    ) {
        if !self.is_ready() {
            return;
        }

        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        let current_frame = self.frame_index;
        debug_assert!(zoom > 0.0, "zoom must be positive");

        const PREFETCH: i32 = 1;

        let img_w = self.width as f32;
        let img_h = self.height as f32;

        let rect_center = rect.center() + pan * zoom;

        let inv_zoom = 1.0 / zoom;
        let left_img = (rect.min.x - rect_center.x) * inv_zoom + img_w / 2.0;
        let right_img = (rect.max.x - rect_center.x) * inv_zoom + img_w / 2.0;
        let top_img = (rect.min.y - rect_center.y) * inv_zoom + img_h / 2.0;
        let bottom_img = (rect.max.y - rect_center.y) * inv_zoom + img_h / 2.0;

        let left = left_img.clamp(0.0, img_w);
        let right = right_img.clamp(0.0, img_w);
        let top = top_img.clamp(0.0, img_h);
        let bottom = bottom_img.clamp(0.0, img_h);

        let tile_size_f = self.tile_size as f32;
        let mut start_gx = (left / tile_size_f).floor() as i32 - PREFETCH;
        let mut end_gx = (right / tile_size_f).ceil() as i32 + PREFETCH;
        let mut start_gy = (top / tile_size_f).floor() as i32 - PREFETCH;
        let mut end_gy = (bottom / tile_size_f).ceil() as i32 + PREFETCH;

        let tiles_x = self.tiles_x as i32;
        let tiles_y = self.tiles_y as i32;

        start_gx = start_gx.max(0);
        end_gx = end_gx.min(tiles_x);
        start_gy = start_gy.max(0);
        end_gy = end_gy.min(tiles_y);

        let source_pixels = Arc::clone(&self.data);
        let source_width = self.width;
        let tile_size = self.tile_size;
        let texture_id = self.id;
        for gy in start_gy..end_gy {
            for gx in start_gx..end_gx {
                let idx = (gy * tiles_x + gx) as usize;
                if let Some(tile) = self.tiles.get_mut(idx) {
                    tile.last_used_frame = current_frame;
                    Self::collect_tile(tile, ctx, options, texture_id);
                    if tile.texture.is_none() && !tile.failed {
                        Self::request_tile(
                            tile,
                            Arc::clone(&source_pixels),
                            source_width,
                            tile_size,
                            ctx,
                        );
                    }

                    let tile_x = gx as f32 * tile_size_f;
                    let tile_y = gy as f32 * tile_size_f;
                    let tile_w = tile.width as f32;
                    let tile_h = tile.height as f32;

                    let screen_x = rect_center.x + (tile_x - img_w / 2.0) * zoom;
                    let screen_y = rect_center.y + (tile_y - img_h / 2.0) * zoom;
                    let screen_w = tile_w * zoom;
                    let screen_h = tile_h * zoom;

                    let screen_rect = egui::Rect::from_min_size(
                        egui::pos2(screen_x, screen_y),
                        egui::vec2(screen_w, screen_h),
                    );

                    if let Some(texture) = &tile.texture {
                        painter.image(
                            texture.id(),
                            screen_rect,
                            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                            egui::Color32::WHITE,
                        );
                        if let Some(state) = tile.mipmap_state.clone() {
                            append_mipmap_upload_callback(
                                painter,
                                screen_rect.intersect(painter.clip_rect()),
                                texture.id(),
                                state,
                                Arc::clone(&mipmap_upload_budget),
                                ctx.clone(),
                            );
                        }
                    }
                }
            }
        }

        // Keep a bounded GPU texture working set. Tiles used this frame are
        // never evicted; older off-screen textures are dropped least-recently
        // used first so repeated panning stays smooth without retaining every
        // tile from a very large source in GPU memory.
        const GPU_TILE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
        const MAX_RESIDENT_TILES: usize = 128;
        let base_tile_bytes = (self.tile_size as usize)
            .saturating_mul(self.tile_size as usize)
            .saturating_mul(4)
            .max(1);
        // A complete RGBA mip chain adds approximately one third to the base
        // level's GPU storage. Include that overhead so the resident budget
        // remains meaningful when Mipmap filtering is enabled.
        let tile_bytes = if options.mipmap_mode.is_some() {
            base_tile_bytes.saturating_add(base_tile_bytes / 3)
        } else {
            base_tile_bytes
        };
        let resident_limit = (GPU_TILE_BUDGET_BYTES / tile_bytes).clamp(1, MAX_RESIDENT_TILES);
        let resident_count = self
            .tiles
            .iter()
            .filter(|tile| tile.texture.is_some())
            .count();
        if resident_count > resident_limit {
            let mut eviction_candidates = self
                .tiles
                .iter()
                .enumerate()
                .filter(|(_, tile)| tile.texture.is_some() && tile.last_used_frame != current_frame)
                .map(|(index, tile)| (index, tile.last_used_frame))
                .collect::<Vec<_>>();
            eviction_candidates.sort_unstable_by_key(|(_, last_used)| *last_used);
            let mut to_evict = resident_count.saturating_sub(resident_limit);
            for (index, _) in eviction_candidates {
                if to_evict == 0 {
                    break;
                }
                self.tiles[index].texture = None;
                self.tiles[index].mipmap_state = None;
                to_evict -= 1;
            }
        }
    }

    /// Render a virtual texture rotated around `center`. Only tiles whose rotated
    /// bounds intersect the viewport are requested and uploaded. This is used by
    /// Compare's anchor-based rotation without allocating a second full image.
    pub fn render_rotated(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        zoom: f32,
        center: egui::Pos2,
        viewport: egui::Rect,
        rotation: f32,
        options: egui::TextureOptions,
        mipmap_upload_budget: Arc<AtomicUsize>,
    ) {
        if !self.is_ready() || !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        let current_frame = self.frame_index;
        let (sin, cos) = rotation.sin_cos();
        let image_width = self.width as f32;
        let image_height = self.height as f32;
        let source = Arc::clone(&self.data);
        let source_width = self.width;
        let tile_size = self.tile_size;
        let texture_id = self.id;
        for tile in &mut self.tiles {
            let left = tile.grid_x as f32 * tile_size as f32 - image_width * 0.5;
            let top = tile.grid_y as f32 * tile_size as f32 - image_height * 0.5;
            let right = left + tile.width as f32;
            let bottom = top + tile.height as f32;
            let rotate_point =
                |x: f32, y: f32| center + egui::vec2(x * cos - y * sin, x * sin + y * cos) * zoom;
            let points = [
                rotate_point(left, top),
                rotate_point(right, top),
                rotate_point(right, bottom),
                rotate_point(left, bottom),
            ];
            let bounds = egui::Rect::from_min_max(
                egui::pos2(
                    points
                        .iter()
                        .map(|point| point.x)
                        .fold(f32::INFINITY, f32::min),
                    points
                        .iter()
                        .map(|point| point.y)
                        .fold(f32::INFINITY, f32::min),
                ),
                egui::pos2(
                    points
                        .iter()
                        .map(|point| point.x)
                        .fold(f32::NEG_INFINITY, f32::max),
                    points
                        .iter()
                        .map(|point| point.y)
                        .fold(f32::NEG_INFINITY, f32::max),
                ),
            );
            if !bounds.intersects(viewport) {
                continue;
            }
            tile.last_used_frame = current_frame;
            Self::collect_tile(tile, ctx, options, texture_id);
            if tile.texture.is_none() && !tile.failed {
                Self::request_tile(tile, Arc::clone(&source), source_width, tile_size, ctx);
            }
            if let Some(texture) = &tile.texture {
                let mut mesh = egui::Mesh::with_texture(texture.id());
                let uvs = [
                    egui::pos2(0.0, 0.0),
                    egui::pos2(1.0, 0.0),
                    egui::pos2(1.0, 1.0),
                    egui::pos2(0.0, 1.0),
                ];
                for vertex in 0..4 {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: points[vertex],
                        uv: uvs[vertex],
                        color: egui::Color32::WHITE,
                    });
                }
                mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                painter.add(egui::Shape::mesh(mesh));
                if let Some(state) = tile.mipmap_state.clone() {
                    append_mipmap_upload_callback(
                        painter,
                        bounds.intersect(painter.clip_rect()),
                        texture.id(),
                        state,
                        Arc::clone(&mipmap_upload_budget),
                        ctx.clone(),
                    );
                }
            }
        }

        const GPU_TILE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
        const MAX_RESIDENT_TILES: usize = 128;
        let base_tile_bytes = (self.tile_size as usize)
            .saturating_mul(self.tile_size as usize)
            .saturating_mul(4)
            .max(1);
        // A complete RGBA mip chain adds approximately one third to the base
        // level's GPU storage. Include that overhead so the resident budget
        // remains meaningful when Mipmap filtering is enabled.
        let tile_bytes = if options.mipmap_mode.is_some() {
            base_tile_bytes.saturating_add(base_tile_bytes / 3)
        } else {
            base_tile_bytes
        };
        let resident_limit = (GPU_TILE_BUDGET_BYTES / tile_bytes).clamp(1, MAX_RESIDENT_TILES);
        let resident_count = self
            .tiles
            .iter()
            .filter(|tile| tile.texture.is_some())
            .count();
        if resident_count > resident_limit {
            let mut candidates = self
                .tiles
                .iter()
                .enumerate()
                .filter(|(_, tile)| tile.texture.is_some() && tile.last_used_frame != current_frame)
                .map(|(index, tile)| (index, tile.last_used_frame))
                .collect::<Vec<_>>();
            candidates.sort_unstable_by_key(|(_, last_used)| *last_used);
            let mut remaining = resident_count.saturating_sub(resident_limit);
            for (index, _) in candidates {
                if remaining == 0 {
                    break;
                }
                self.tiles[index].texture = None;
                self.tiles[index].mipmap_state = None;
                remaining -= 1;
            }
        }
    }

    /// Get the dimensions of the image.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
