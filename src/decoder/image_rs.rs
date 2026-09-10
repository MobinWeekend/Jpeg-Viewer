// src/decoder/image_rs.rs
// This file is the sole place where `image` is used.

use crate::image_core::{
    DecodeOptions, DecodedImage, ImageColorInfo, ImageDecoder, ImageError, ImageFormat,
};
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

    fn color_info(&self, bytes: &[u8]) -> Result<Option<ImageColorInfo>, ImageError> {
        use std::io::Cursor;

        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| ImageError::Decode(format!("format detection: {}", e)))?;
        let decoder = reader
            .into_decoder()
            .map_err(|e| ImageError::Decode(format!("decoder creation: {}", e)))?;

        let info = match decoder.color_type() {
            image::ColorType::L8 => ("Grayscale", 8),
            image::ColorType::La8 => ("Grayscale + Alpha", 8),
            image::ColorType::Rgb8 => ("RGB", 8),
            image::ColorType::Rgba8 => ("RGBA", 8),
            image::ColorType::L16 => ("Grayscale", 16),
            image::ColorType::La16 => ("Grayscale + Alpha", 16),
            image::ColorType::Rgb16 => ("RGB", 16),
            image::ColorType::Rgba16 => ("RGBA", 16),
            image::ColorType::Rgb32F => ("RGB float", 32),
            image::ColorType::Rgba32F => ("RGBA float", 32),
            _ => ("Unknown", 0),
        };

        Ok(Some(ImageColorInfo {
            description: info.0.to_string(),
            bits_per_channel: info.1,
        }))
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
