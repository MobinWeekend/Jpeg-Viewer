// src/decoder/mod.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageError};
pub mod format_detection;
mod image_rs;
mod registry;

pub use image_rs::ImageRsDecoder;
pub use registry::DecoderRegistry;

/// Build the default registry with the built‑in `image-rs` decoder.
pub fn default_registry() -> DecoderRegistry {
    let mut registry = DecoderRegistry::new();
    registry.register(Box::new(ImageRsDecoder::new()));
    registry
}

pub fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    let registry = default_registry();
    let format = format_detection::detect_format(bytes).ok_or(ImageError::UnknownFormat)?;
    registry.decode(bytes, format, &DecodeOptions::default())
}
