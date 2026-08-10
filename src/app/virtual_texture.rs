// src/app/virtual_texture.rs

use eframe::egui;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::sync::{Arc, Mutex};

/// Maximum texture size that can be safely uploaded to the GPU.
pub const MAX_GPU_TEXTURE_SIZE: u32 = 16384;
/// Pixel count threshold above which we always use virtual texturing.
pub const LARGE_IMAGE_THRESHOLD: u64 = 50_000_000;

/// Progress tracking structure - thread-safe
#[derive(Clone, Debug)]
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

/// A tile from the image.
#[derive(Clone)]
struct Tile {
    pub grid_x: u32,
    pub grid_y: u32,
    pub image: Arc<RgbaImage>,
    pub texture: Option<egui::TextureHandle>,
    pub dirty: bool,
}

/// Virtual texture manager – holds the full image and generates tiles on demand.
pub struct VirtualTexture {
    full_image: Option<DynamicImage>,
    width: u32,
    height: u32,
    tile_size: u32,
    tiles: Vec<Tile>,
    texture_filter: egui::TextureFilter,
    is_ready: bool,
    // Progress tracking - shared with background thread
    pub progress: Arc<Mutex<PreparationProgress>>,
}

impl VirtualTexture {
    /// Create a new virtual texture from a loaded image.
    /// This is fast - it just stores the image and marks it as not ready.
    pub fn new(img: DynamicImage) -> Self {
        let (width, height) = img.dimensions();
        let tile_size = 256;

        // Count total tiles
        let tiles_x = (width + tile_size - 1) / tile_size;
        let tiles_y = (height + tile_size - 1) / tile_size;
        let total_tiles = (tiles_x * tiles_y) as usize;

        let progress = Arc::new(Mutex::new(PreparationProgress {
            total_tiles,
            prepared_tiles: 0,
            is_preparing: false,
            is_complete: false,
        }));

        Self {
            full_image: Some(img),
            width,
            height,
            tile_size,
            tiles: Vec::new(),
            texture_filter: egui::TextureFilter::Linear,
            is_ready: false,
            progress,
        }
    }

    /// Prepare the virtual texture (split into tiles).
    /// This runs in a background thread and updates progress.
    pub fn prepare(&mut self) {
        if self.is_ready {
            return;
        }

        let (w, h) = (self.width, self.height);
        let tile_size = self.tile_size;
        
        // Take ownership of the image
        let img = self.full_image.take().expect("Image already taken");
        let rgba = img.to_rgba8();
        
        let mut tiles = Vec::new();
        let tiles_x = (w + tile_size - 1) / tile_size;
        let tiles_y = (h + tile_size - 1) / tile_size;

        // Update progress: preparing started
        {
            let mut progress = self.progress.lock().unwrap();
            progress.is_preparing = true;
        }

        let total_tiles = (tiles_x * tiles_y) as usize;
        let mut prepared = 0;

        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let x = tx * tile_size;
                let y = ty * tile_size;
                let tile_w = (x + tile_size).min(w) - x;
                let tile_h = (y + tile_size).min(h) - y;

                let mut tile_data = RgbaImage::new(tile_w, tile_h);
                for py in 0..tile_h {
                    for px in 0..tile_w {
                        let src_pixel = rgba.get_pixel((x + px) as u32, (y + py) as u32);
                        tile_data.put_pixel(px, py, *src_pixel);
                    }
                }

                tiles.push(Tile {
                    grid_x: tx,
                    grid_y: ty,
                    image: Arc::new(tile_data),
                    texture: None,
                    dirty: true,
                });

                prepared += 1;
                
                // Update progress every few tiles to avoid locking too often
                if prepared % 10 == 0 || prepared == total_tiles {
                    let mut progress = self.progress.lock().unwrap();
                    progress.prepared_tiles = prepared;
                    progress.total_tiles = total_tiles;
                }
            }
        }

        self.tiles = tiles;
        self.is_ready = true;

        // Mark progress as complete
        {
            let mut progress = self.progress.lock().unwrap();
            progress.prepared_tiles = total_tiles;
            progress.is_preparing = false;
            progress.is_complete = true;
        }
    }

    /// Check if the virtual texture is ready.
    pub fn is_ready(&self) -> bool {
        self.is_ready
    }

    /// Get preparation progress (0.0 to 1.0)
    pub fn preparation_progress(&self) -> f32 {
        let progress = self.progress.lock().unwrap();
        if progress.total_tiles == 0 {
            return 0.0;
        }
        progress.prepared_tiles as f32 / progress.total_tiles as f32
    }

    /// Get total tiles count
    pub fn total_tiles(&self) -> usize {
        let progress = self.progress.lock().unwrap();
        progress.total_tiles
    }

    /// Get prepared tiles count
    pub fn prepared_tiles_count(&self) -> usize {
        let progress = self.progress.lock().unwrap();
        progress.prepared_tiles
    }

    /// Update the texture filter.
    pub fn set_texture_filter(&mut self, filter: egui::TextureFilter, _ctx: &egui::Context) {
        if self.texture_filter != filter && self.is_ready {
            self.texture_filter = filter;
            for tile in &mut self.tiles {
                tile.dirty = true;
            }
        }
    }

    /// Upload a single tile's texture.
    fn upload_tile(tile: &mut Tile, ctx: &egui::Context, filter: egui::TextureFilter) {
        if tile.dirty || tile.texture.is_none() {
            let rgba = &*tile.image;
            let width = rgba.width();
            let height = rgba.height();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                rgba.as_raw(),
            );
            let options = egui::TextureOptions {
                magnification: filter,
                minification: filter,
                mipmap_mode: None,
                ..Default::default()
            };
            let texture = ctx.load_texture(
                &format!("tile_{}_{}", tile.grid_x, tile.grid_y),
                color_image,
                options,
            );
            tile.texture = Some(texture);
            tile.dirty = false;
        }
    }

    /// Render the visible tiles using the given painter.
    pub fn render(&mut self, ctx: &egui::Context, painter: &egui::Painter, zoom: f32, pan: egui::Vec2, viewport_size: egui::Vec2) {
        if !self.is_ready {
            return;
        }

        let img_w = self.width as f32;
        let img_h = self.height as f32;

        // Compute visible rectangle in image coordinates.
        let center = viewport_size / 2.0;
        let rect_center = center + pan * zoom;

        let inv_zoom = 1.0 / zoom;
        let left_img = (0.0 - rect_center.x) * inv_zoom + img_w / 2.0;
        let right_img = (viewport_size.x - rect_center.x) * inv_zoom + img_w / 2.0;
        let top_img = (0.0 - rect_center.y) * inv_zoom + img_h / 2.0;
        let bottom_img = (viewport_size.y - rect_center.y) * inv_zoom + img_h / 2.0;

        let left = left_img.clamp(0.0, img_w);
        let right = right_img.clamp(0.0, img_w);
        let top = top_img.clamp(0.0, img_h);
        let bottom = bottom_img.clamp(0.0, img_h);

        // Determine tile grid range.
        let tile_size_f = self.tile_size as f32;
        let start_gx = (left / tile_size_f).floor() as i32;
        let end_gx = (right / tile_size_f).ceil() as i32;
        let start_gy = (top / tile_size_f).floor() as i32;
        let end_gy = (bottom / tile_size_f).ceil() as i32;

        // Render visible tiles
        for gy in start_gy..=end_gy {
            for gx in start_gx..=end_gx {
                if gx < 0 || gy < 0 {
                    continue;
                }
                if let Some(tile) = self.tiles.iter_mut().find(|t| t.grid_x == gx as u32 && t.grid_y == gy as u32) {
                    Self::upload_tile(tile, ctx, self.texture_filter);

                    // Compute screen rect for this tile.
                    let tile_x = gx as f32 * tile_size_f;
                    let tile_y = gy as f32 * tile_size_f;
                    let tile_w = tile.image.width() as f32;
                    let tile_h = tile.image.height() as f32;
                    
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

    /// Get a reference to the progress (for UI)
    pub fn progress_ref(&self) -> &Arc<Mutex<PreparationProgress>> {
        &self.progress
    }
}