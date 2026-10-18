//! TGA decoding with an explicit format selection.
//!
//! TGA does not have a reliable magic signature, so asking `ImageReader` to
//! guess the format again after the registry has already selected TGA can fail.

use crate::image_core::{
    DecodeOptions, DecodedImage, ImageColorInfo, ImageDecoder, ImageError, ImageFormat,
};
use image::{DynamicImage, GenericImageView, ImageDecoder as _, ImageReader};
use std::io::Cursor;

pub struct TgaDecoder;

impl TgaDecoder {
    pub fn new() -> Self {
        Self
    }

    fn reader(bytes: &[u8]) -> ImageReader<Cursor<&[u8]>> {
        ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Tga)
    }
}

impl ImageDecoder for TgaDecoder {
    fn name(&self) -> &'static str {
        "tga"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::Tga]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        let mut decoder = Self::reader(bytes)
            .into_decoder()
            .map_err(|e| ImageError::Decode(format!("TGA decoder creation: {e}")))?;
        let orientation = decoder.orientation().ok();
        let mut image = DynamicImage::from_decoder(decoder)
            .map_err(|e| ImageError::Decode(format!("TGA decoding: {e}")))?;
        if let Some(orientation) = orientation {
            image.apply_orientation(orientation);
        }
        let (width, height) = image.dimensions();
        let data = image.to_rgba8().into_raw();
        Ok(DecodedImage {
            width,
            height,
            data,
        })
    }

    fn color_info(&self, bytes: &[u8]) -> Result<Option<ImageColorInfo>, ImageError> {
        let decoder = Self::reader(bytes)
            .into_decoder()
            .map_err(|e| ImageError::Decode(format!("TGA decoder creation: {e}")))?;
        let (description, bits_per_channel) = match decoder.color_type() {
            image::ColorType::L8 => ("Grayscale", 8),
            image::ColorType::La8 => ("Grayscale + Alpha", 8),
            image::ColorType::Rgb8 => ("RGB", 8),
            image::ColorType::Rgba8 => ("RGBA", 8),
            image::ColorType::L16 => ("Grayscale", 16),
            image::ColorType::La16 => ("Grayscale + Alpha", 16),
            image::ColorType::Rgb16 => ("RGB", 16),
            image::ColorType::Rgba16 => ("RGBA", 16),
            _ => ("Unknown", 0),
        };
        Ok(Some(ImageColorInfo {
            description: description.to_string(),
            bits_per_channel,
        }))
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        Self::reader(bytes)
            .into_dimensions()
            .map_err(|e| ImageError::Decode(format!("TGA dimensions: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::default_registry;
    use crate::decoder::format_detection::detect_format;

    // Minimal 1x1 uncompressed true-color TGA: 18-byte header + one BGR pixel.
    fn one_pixel_tga() -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[2] = 2; // Uncompressed true-color.
        bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
        bytes[16] = 24;
        bytes.extend_from_slice(&[30, 20, 10]); // B, G, R.
        bytes
    }

    #[test]
    fn detects_dimensions_and_decodes_tga_without_format_guessing() {
        let bytes = one_pixel_tga();
        assert_eq!(detect_format(&bytes), Some(ImageFormat::Tga));

        let registry = default_registry();
        assert_eq!(registry.dimensions(&bytes, ImageFormat::Tga).unwrap(), (1, 1));

        let decoded = registry
            .decode(&bytes, ImageFormat::Tga, &DecodeOptions::default())
            .unwrap();
        assert_eq!((decoded.width, decoded.height), (1, 1));
        assert_eq!(decoded.data, vec![10, 20, 30, 255]);
    }

    #[test]
    fn malformed_tga_returns_an_error() {
        let decoder = TgaDecoder::new();
        assert!(decoder.decode(&[0u8; 18], &DecodeOptions::default()).is_err());
    }
}
