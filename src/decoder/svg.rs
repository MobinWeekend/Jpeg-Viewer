// src/decoder/svg.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};

use resvg::{tiny_skia, usvg};

const MAX_RASTER_DIMENSION: u32 = 4096;

pub struct SvgDecoder;

impl SvgDecoder {
    pub fn new() -> Self {
        Self
    }

    /// Parse SVG data into a usvg tree.
    fn parse(bytes: &[u8]) -> Result<usvg::Tree, ImageError> {
        let options = usvg::Options::default();

        usvg::Tree::from_data(bytes, &options)
            .map_err(|e| ImageError::Decode(format!("SVG parsing failed: {e}")))
    }

    /// Get the SVG's intrinsic dimensions.
    fn get_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        let tree = Self::parse(bytes)?;
        let size = tree.size();

        let width = size.width().ceil() as u32;
        let height = size.height().ceil() as u32;

        if width == 0 || height == 0 {
            return Err(ImageError::Decode("SVG has zero dimensions".to_string()));
        }

        Ok((width, height))
    }

    /// Calculate the rasterization dimensions while preserving aspect ratio.
    ///
    /// SVGs larger than 4096x4096 are scaled down so neither dimension
    /// exceeds MAX_RASTER_DIMENSION.
    fn raster_dimensions(width: u32, height: u32) -> (u32, u32) {
        if width <= MAX_RASTER_DIMENSION && height <= MAX_RASTER_DIMENSION {
            return (width, height);
        }

        let scale = (MAX_RASTER_DIMENSION as f64 / width as f64)
            .min(MAX_RASTER_DIMENSION as f64 / height as f64);

        let raster_width = ((width as f64 * scale).round() as u32).max(1);
        let raster_height = ((height as f64 * scale).round() as u32).max(1);

        (
            raster_width.min(MAX_RASTER_DIMENSION),
            raster_height.min(MAX_RASTER_DIMENSION),
        )
    }

    /// Rasterize the SVG into the application's canonical RGBA8 format.
    fn rasterize(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
        let tree = Self::parse(bytes)?;
        let size = tree.size();

        let intrinsic_width = size.width().ceil() as u32;
        let intrinsic_height = size.height().ceil() as u32;

        if intrinsic_width == 0 || intrinsic_height == 0 {
            return Err(ImageError::Decode("SVG has zero dimensions".to_string()));
        }

        let (width, height) = Self::raster_dimensions(intrinsic_width, intrinsic_height);

        let scale_x = width as f32 / intrinsic_width as f32;
        let scale_y = height as f32 / intrinsic_height as f32;

        let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);

        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
            ImageError::Decode(format!(
                "failed to allocate SVG raster buffer: {}x{}",
                width, height
            ))
        })?;

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        // tiny-skia stores premultiplied RGBA8.
        // DecodedImage uses straight RGBA8.
        let mut data = pixmap.data().to_vec();

        unpremultiply_rgba8(&mut data);

        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                ImageError::Decode("SVG dimensions overflow RGBA8 buffer size".to_string())
            })?;

        if data.len() != expected_len {
            return Err(ImageError::Decode(format!(
                "SVG raster buffer size mismatch: got {}, expected {}",
                data.len(),
                expected_len
            )));
        }

        Ok(DecodedImage {
            width,
            height,
            data,
        })
    }
}

impl Default for SvgDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDecoder for SvgDecoder {
    fn name(&self) -> &'static str {
        "resvg"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::Svg]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        Self::rasterize(bytes)
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        Self::get_dimensions(bytes)
    }
}

/// Convert premultiplied RGBA8 pixels into straight/unpremultiplied RGBA8.
///
/// tiny-skia stores:
///
///     channel = original_channel * alpha / 255
///
/// DecodedImage expects straight RGBA8.
fn unpremultiply_rgba8(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3];

        match alpha {
            0 => {
                pixel[0] = 0;
                pixel[1] = 0;
                pixel[2] = 0;
            }

            255 => {}

            alpha => {
                let alpha = alpha as u16;

                pixel[0] = ((pixel[0] as u16 * 255 + alpha / 2) / alpha).min(255) as u8;

                pixel[1] = ((pixel[1] as u16 * 255 + alpha / 2) / alpha).min(255) as u8;

                pixel[2] = ((pixel[2] as u16 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}
