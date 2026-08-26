// src/decoder/mod.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageError};

pub mod format_detection;

mod heic;
mod image_rs;
mod jpeg_xl;
mod registry;
mod svg;
mod zenavif;

pub use heic::HeicDecoder;
pub use image_rs::ImageRsDecoder;
pub use jpeg_xl::JpegXlDecoder;
pub use registry::DecoderRegistry;
pub use svg::SvgDecoder;
pub use zenavif::ZenAvifDecoder;

/// Build the default decoder registry.
///
/// Decoder registration order matters when multiple decoders support
/// the same format: a later registration overrides the previous decoder
/// for that format.
pub fn default_registry() -> DecoderRegistry {
    let mut registry = DecoderRegistry::new();

    // Standard formats:
    // JPEG, PNG, GIF, WebP, BMP, TIFF, etc.
    registry.register(Box::new(ImageRsDecoder::new()));

    // AVIF
    registry.register(Box::new(ZenAvifDecoder::new()));

    // HEIC
    registry.register(Box::new(HeicDecoder::new()));

    // JPEG XL
    registry.register(Box::new(JpegXlDecoder::new()));

    // SVG
    registry.register(Box::new(SvgDecoder::new()));

    registry
}

/// Convenience function for decoding a byte slice using the
/// default decoder registry.
pub fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    let registry = default_registry();

    let format = format_detection::detect_format(bytes).ok_or(ImageError::UnknownFormat)?;

    registry.decode(bytes, format, &DecodeOptions::default())
}
