// src/decoder/mod.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageError};
pub mod format_detection;
mod heic;
mod image_rs;
mod registry;
mod zenavif;

pub use heic::HeicDecoder;
pub use image_rs::ImageRsDecoder;
pub use registry::DecoderRegistry;
pub use zenavif::ZenAvifDecoder;

/// Build the default registry with the built‑in `image-rs` decoder.
pub fn default_registry() -> DecoderRegistry {
    let mut registry = DecoderRegistry::new();
    // Base decoder for JPEG, PNG, GIF, WebP, BMP, TIFF
    registry.register(Box::new(ImageRsDecoder::new()));
    // Override AVIF with zenavif
    registry.register(Box::new(ZenAvifDecoder::new()));
    // Add HEIC (pure Rust)
    registry.register(Box::new(HeicDecoder::new()));
    registry
}

pub fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    let registry = default_registry();
    let format = format_detection::detect_format(bytes).ok_or(ImageError::UnknownFormat)?;
    registry.decode(bytes, format, &DecodeOptions::default())
}
