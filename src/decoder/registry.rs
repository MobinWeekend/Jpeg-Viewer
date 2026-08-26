// src/decoder/registry.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};
use std::collections::HashMap;

pub struct DecoderRegistry {
    decoders: Vec<Box<dyn ImageDecoder>>,
    format_index: HashMap<ImageFormat, usize>,
}

impl DecoderRegistry {
    pub fn new() -> Self {
        Self {
            decoders: Vec::new(),
            format_index: HashMap::new(),
        }
    }

    /// Register a decoder. If a format already exists, the new decoder overrides.
    pub fn register(&mut self, decoder: Box<dyn ImageDecoder>) {
        let idx = self.decoders.len();
        for fmt in decoder.supported_formats() {
            self.format_index.insert(*fmt, idx);
        }
        self.decoders.push(decoder);
    }

    pub fn get_decoder(&self, format: ImageFormat) -> Option<&dyn ImageDecoder> {
        self.format_index
            .get(&format)
            .and_then(|&idx| self.decoders.get(idx))
            .map(|d| d.as_ref())
    }

    pub fn decode(
        &self,
        bytes: &[u8],
        format: ImageFormat,
        options: &DecodeOptions,
    ) -> Result<DecodedImage, ImageError> {
        let decoder = self
            .get_decoder(format)
            .ok_or(ImageError::NoDecoder(format))?;
        decoder.decode(bytes, options)
    }

    pub fn dimensions(&self, bytes: &[u8], format: ImageFormat) -> Result<(u32, u32), ImageError> {
        let decoder = self
            .get_decoder(format)
            .ok_or(ImageError::NoDecoder(format))?;
        decoder.dimensions(bytes)
    }
}
