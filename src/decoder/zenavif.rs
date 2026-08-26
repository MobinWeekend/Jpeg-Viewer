// src/decoder/zenavif.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};

use zenavif::{DecoderConfig, Unstoppable, decode_with};
use zenpixels_convert::PixelBufferConvertTypedExt;

pub struct ZenAvifDecoder {
    config: DecoderConfig,
}

impl ZenAvifDecoder {
    pub fn new() -> Self {
        Self {
            // Force the decoder to produce 8-bit output when possible.
            //
            // This is useful because your application-side DecodedImage
            // representation is always RGBA8.
            config: DecoderConfig::new().prefer_8bit(true),
        }
    }
}

impl Default for ZenAvifDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDecoder for ZenAvifDecoder {
    fn name(&self) -> &'static str {
        "zenavif"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::Avif]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        let image = decode_with(bytes, &self.config, &Unstoppable)
            .map_err(|e| ImageError::Decode(format!("zenavif decode: {e}")))?;

        let width = image.width();
        let height = image.height();

        // Convert whatever native pixel representation zenavif produced
        // into canonical RGBA8.
        //
        // This handles:
        // - RGB/RGBA
        // - different channel layouts
        // - native bit depths
        // - chroma subsampling
        //
        // without us having to know zenavif's internal PixelBuffer format.
        let rgba = image.to_rgba8();

        let data = rgba.copy_to_contiguous_bytes();

        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                ImageError::Decode("AVIF dimensions overflow RGBA8 buffer size".to_string())
            })? as usize;

        if data.len() != expected_len {
            return Err(ImageError::Decode(format!(
                "zenavif returned {} RGBA8 bytes, expected {}",
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

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        // zenavif 0.1.6 does not expose a separate cheap dimension-only
        // decoder through the high-level decode API.
        //
        // Therefore, use the normal decoder here. This can be optimized
        // later if the crate exposes a suitable metadata-only API.
        let image = decode_with(bytes, &self.config, &Unstoppable)
            .map_err(|e| ImageError::Decode(format!("zenavif decode for dimensions: {e}")))?;

        Ok((image.width(), image.height()))
    }
}
