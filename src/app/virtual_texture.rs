// src/app/virtual_texture.rs
//! Virtual texture manager – uses `DecodedImage` (plain RGBA8) and generates tiles on demand.

use crate::constants::MAX_TILE_SIZE;
use crate::image_core::DecodedImage;
use eframe::egui;
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

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

/// A single tile – raw RGBA8 data and optional GPU texture.
struct Tile {
    grid_x: u32,
    grid_y: u32,
    data: Vec<u8>, // RGBA8 tile data
    width: u32,
    height: u32,
    texture: Option<egui::TextureHandle>,
}

/// Virtual texture manager – holds the full image and generates tiles on demand.
pub struct VirtualTexture {
    id: usize,
    data: Vec<u8>, // RGBA8 data of the full image
    width: u32,
    height: u32,
    tile_size: u32,
    tiles_x: usize,
    tiles_y: usize,
    total_tiles: usize,
    tiles: Vec<Tile>,
    prepared_tiles: Arc<AtomicUsize>,
    state: AtomicU8,
}

impl VirtualTexture {
    pub fn new(img: DecodedImage, tile_size: u32) -> Self {
        assert!(
            tile_size > 0 && tile_size <= MAX_TILE_SIZE,
            "Invalid virtual texture tile size"
        );

        let width = img.width();
        let height = img.height();
        let data = img.into_data(); // consumes DecodedImage, returns RGBA8 Vec<u8>

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
            prepared_tiles: Arc::new(AtomicUsize::new(0)),
            state: AtomicU8::new(STATE_NOT_STARTED),
        }
    }

    /// Synchronous tile preparation – call off the UI thread.
    pub fn prepare(&mut self) {
        if self.state.load(Ordering::Acquire) == STATE_COMPLETE {
            return;
        }

        let w = self.width;
        let h = self.height;
        let tile_size = self.tile_size;
        let tiles_x = self.tiles_x;
        let total_tiles = self.total_tiles;

        // Share the data reference; we will not mutate it during tile generation.
        let data = &self.data;
        //let row_stride = (w * 4) as usize; // bytes per row

        // Reset progress and mark as preparing.
        self.prepared_tiles.store(0, Ordering::Relaxed);
        self.state.store(STATE_PREPARING, Ordering::Release);

        let prepared_tiles = Arc::clone(&self.prepared_tiles);

        let tiles: Vec<Tile> = (0..total_tiles)
            .into_par_iter()
            .map(|index| {
                let tx = index % tiles_x;
                let ty = index / tiles_x;
                let x = (tx * tile_size as usize) as u32;
                let y = (ty * tile_size as usize) as u32;
                let tile_w = (x + tile_size).min(w) - x;
                let tile_h = (y + tile_size).min(h) - y;

                // Extract tile data from the full RGBA8 buffer.
                let mut tile_data = Vec::with_capacity((tile_w * tile_h * 4) as usize);
                let src_start = (y * w + x) as usize * 4;
                for row in 0..tile_h {
                    let src_off = src_start + (row * w) as usize * 4;
                    //let dst_off = row as usize * (tile_w as usize * 4);
                    tile_data.extend_from_slice(&data[src_off..src_off + (tile_w as usize * 4)]);
                }

                prepared_tiles.fetch_add(1, Ordering::Relaxed);

                Tile {
                    grid_x: tx as u32,
                    grid_y: ty as u32,
                    data: tile_data,
                    width: tile_w,
                    height: tile_h,
                    texture: None,
                }
            })
            .collect();

        self.tiles = tiles;
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

    /// Upload a single tile's texture (if not already uploaded).
    fn upload_tile(tile: &mut Tile, ctx: &egui::Context, options: egui::TextureOptions, id: usize) {
        if tile.texture.is_none() {
            let width = tile.width;
            let height = tile.height;
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &tile.data,
            );
            let texture = ctx.load_texture(
                &format!("vt_{}_tile_{}_{}", id, tile.grid_x, tile.grid_y),
                color_image,
                options,
            );
            tile.texture = Some(texture);
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
    ) {
        if !self.is_ready() {
            return;
        }

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

        for gy in start_gy..end_gy {
            for gx in start_gx..end_gx {
                let idx = (gy * tiles_x + gx) as usize;
                if let Some(tile) = self.tiles.get_mut(idx) {
                    Self::upload_tile(tile, ctx, options, self.id);

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
                    }
                }
            }
        }
    }

    /// Get the dimensions of the image.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
