// src/decoder/heic.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};

use heic::{DecoderConfig, PixelLayout};

pub struct HeicDecoder {
    config: DecoderConfig,
}

impl HeicDecoder {
    pub fn new() -> Self {
        Self {
            config: DecoderConfig::new(),
        }
    }
}

impl Default for HeicDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDecoder for HeicDecoder {
    fn name(&self) -> &'static str {
        "heic"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::Heic]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        let output = self
            .config
            .decode(bytes, PixelLayout::Rgba8)
            .map_err(|e| ImageError::Decode(format!("HEIC decode: {e}")))?;

        Ok(DecodedImage {
            width: output.width,
            height: output.height,
            data: output.data,
        })
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        let info = heic::ImageInfo::from_bytes(bytes)
            .map_err(|e| ImageError::Decode(format!("HEIC probe: {e}")))?;

        Ok((info.width as u32, info.height as u32))
    }
}
