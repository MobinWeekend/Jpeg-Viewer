// src/format_detection.rs
// Pure magic‑byte detection – no external crate dependencies.
use crate::app::types::LoadedImage;
use crate::constants::MAX_TILE_SIZE;
use crate::decoder::{DecoderRegistry, default_registry};
use crate::gif::animation::GifAnimation;
use crate::image_core::ImageFormat;
use crate::image_core::{DecodeOptions, ImageError};

/// Detect the image format from raw bytes.
///
/// Uses `infer` for common formats and falls back to custom
/// signature checks for formats not recognized by `infer`.
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    if let Some(kind) = infer::get(bytes) {
        if let Some(format) = map_infer_type(&kind) {
            return Some(format);
        }
    }

    detect_fallback_format(bytes)
}

/// Convert an `infer` type into our internal `ImageFormat`.
fn map_infer_type(kind: &infer::Type) -> Option<ImageFormat> {
    match kind.extension() {
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::Webp),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "avif" => Some(ImageFormat::Avif),
        "ico" => Some(ImageFormat::Ico),

        _ => None,
    }
}

/// Detect formats that are not handled by `infer`.
fn detect_fallback_format(bytes: &[u8]) -> Option<ImageFormat> {
    // DDS
    if bytes.starts_with(b"DDS ") {
        return Some(ImageFormat::Dds);
    }

    // OpenEXR
    if bytes.starts_with(b"\x76\x2F\x31\x01") {
        return Some(ImageFormat::Exr);
    }

    // Farbfeld
    if bytes.starts_with(b"farbfeld") {
        return Some(ImageFormat::Farbfeld);
    }

    // Radiance HDR
    if bytes.starts_with(b"#?RADIANCE") || bytes.starts_with(b"#?RGBE") {
        return Some(ImageFormat::Hdr);
    }

    // PNM (P1-P6)
    if bytes.len() >= 2 && bytes[0] == b'P' && (b'1'..=b'6').contains(&bytes[1]) {
        return Some(ImageFormat::Pnm);
    }

    // QOI
    if bytes.starts_with(b"qoif") {
        return Some(ImageFormat::Qoi);
    }

    // TGA
    //
    // TGA does not have a reliable magic signature, so this is only
    // a heuristic for common uncompressed/RLE RGB images.
    if bytes.len() >= 3 && bytes[1] == 0 && matches!(bytes[2], 2 | 10) && bytes[0] < 128 {
        return Some(ImageFormat::Tga);
    }

    None
}

/// Load an image from raw bytes, with:
/// - GIF animation detection (returns `LoadedImage::Animated`)
/// - Virtual‑texture routing for images larger than `threshold` (returns `LoadedImage::VirtualPending`)
/// - Fallback to static decode (returns `LoadedImage::Static`)
///
/// `threshold` is the tile size at which virtual texturing kicks in.
/// `options` are decoder‑agnostic settings (currently empty).
pub fn load_bytes_with_detection(
    bytes: Vec<u8>,
    _path_hint: Option<&std::path::Path>,
    threshold: u32,
    options: &DecodeOptions,
    registry: Option<&DecoderRegistry>, // optional – if None, use default registry
) -> Result<LoadedImage, String> {
    let registry = match registry {
        Some(r) => r,
        None => &default_registry(),
    };

    // 1. GIF animation detection (special case)
    if is_gif_bytes(&bytes) {
        if let Ok(gif) = GifAnimation::from_bytes_preview(&bytes) {
            return Ok(LoadedImage::Animated(gif, true));
        }
        if let Ok(gif) = GifAnimation::from_bytes(&bytes) {
            return Ok(LoadedImage::Animated(gif, false));
        }
        // If we can't parse as GIF, fall through to static decode.
    }

    let threshold = threshold.min(MAX_TILE_SIZE);

    // 2. Detect format
    let format = match detect_format(&bytes) {
        Some(f) => f,
        None => return Err(ImageError::UnknownFormat.to_string()),
    };

    // 3. Get dimensions (fast path)
    let (width, height) = match registry.dimensions(&bytes, format) {
        Ok(dim) => dim,
        Err(e) => return Err(e.to_string()),
    };

    // 4. Virtual‑texture routing
    if threshold > 0 && (width >= threshold || height >= threshold) {
        return Ok(LoadedImage::VirtualPending(bytes, width, height));
    }

    // 5. Normal static decode
    match registry.decode(&bytes, format, options) {
        Ok(decoded) => Ok(LoadedImage::Static(decoded)),
        Err(e) => Err(e.to_string()),
    }
}

/// Quick check if bytes start with a GIF signature.
fn is_gif_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"))
}
