// src/decoder/image_rs.rs
// This file is the sole place where `image` is used.

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};
use image::{DynamicImage, GenericImageView, ImageDecoder as _, ImageReader};

pub struct ImageRsDecoder;

impl ImageRsDecoder {
    pub fn new() -> Self {
        Self
    }
}

impl ImageDecoder for ImageRsDecoder {
    fn name(&self) -> &'static str {
        "image-rs"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Gif,
            ImageFormat::Webp,
            ImageFormat::Bmp,
            ImageFormat::Tiff,
            //ImageFormat::Avif,
            ImageFormat::Dds,
            ImageFormat::Farbfeld,
            ImageFormat::Hdr,
            ImageFormat::Ico,
            ImageFormat::Pnm,
            ImageFormat::Qoi,
            ImageFormat::Tga,
            ImageFormat::Exr,
        ]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        use std::io::Cursor;
        let cursor = Cursor::new(bytes);
        let reader = ImageReader::new(cursor)
            .with_guessed_format()
            .map_err(|e| ImageError::Decode(format!("format detection: {}", e)))?;
        let mut decoder = reader
            .into_decoder()
            .map_err(|e| ImageError::Decode(format!("decoder creation: {}", e)))?;
        let orientation = decoder.orientation().ok();
        let mut image = DynamicImage::from_decoder(decoder)
            .map_err(|e| ImageError::Decode(format!("decoding: {}", e)))?;
        if let Some(orient) = orientation {
            image.apply_orientation(orient);
        }
        // Now `dimensions()` is available via `GenericImageView`
        let (w, h) = image.dimensions();
        let data = image.to_rgba8().into_raw();
        Ok(DecodedImage {
            width: w,
            height: h,
            data,
        })
    }

    // Fast dimension path – uses `into_dimensions()` from `image::ImageReader`
    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        use std::io::Cursor;
        let cursor = Cursor::new(bytes);
        let reader = ImageReader::new(cursor)
            .with_guessed_format()
            .map_err(|e| ImageError::Decode(format!("format detection: {}", e)))?;
        reader
            .into_dimensions()
            .map_err(|e| ImageError::Decode(format!("dimensions: {}", e)))
    }
}
