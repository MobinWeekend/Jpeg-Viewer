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
    // RAW containers often resemble TIFF and may not have a unique extension
    // available (for example, when opened from an archive). Prefer the RAW
    // classifier before generic signature detection.
    if zenraw::is_raw_file(bytes) {
        return Some(ImageFormat::CameraRaw);
    }

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

    // TGA has no dependable magic signature. Validate the fixed header rather
    // than checking only the image-type byte: valid files may be grayscale or
    // color-mapped, and may use either uncompressed or RLE encoding.
    if looks_like_tga(bytes) {
        return Some(ImageFormat::Tga);
    }

    // HEIC / HEIF (brands in ftyp box)
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if brand == b"heic" || brand == b"heix" || brand == b"hevc" || brand == b"mif1" {
            return Some(ImageFormat::Heic);
        }
    }

    // JPEG XL container
    if bytes.starts_with(&[0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ']) {
        return Some(ImageFormat::JpegXl);
    }

    // JPEG XL codestream
    if bytes.starts_with(&[0xFF, 0x0A]) {
        return Some(ImageFormat::JpegXl);
    }

    // SVG is XML/text rather than a binary format.
    if let Ok(text) = std::str::from_utf8(bytes) {
        let text = text.trim_start_matches('\u{FEFF}').trim_start();

        if text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg")) {
            return Some(ImageFormat::Svg);
        }
    }

    None
}

/// Conservative structural validation for the 18-byte TGA header.
/// This is a heuristic, not proof that the complete file is valid.
fn looks_like_tga(bytes: &[u8]) -> bool {
    if bytes.len() < 18 {
        return false;
    }

    let color_map_type = bytes[1];
    let image_type = bytes[2];
    let width = u16::from_le_bytes([bytes[12], bytes[13]]);
    let height = u16::from_le_bytes([bytes[14], bytes[15]]);
    let pixel_depth = bytes[16];
    let color_map_length = u16::from_le_bytes([bytes[5], bytes[6]]);
    let color_map_depth = bytes[7];

    if width == 0 || height == 0 || color_map_type > 1 {
        return false;
    }

    let depth_is_valid = match image_type {
        // Color-mapped, true-color, grayscale and their RLE variants.
        1 | 9 => matches!(pixel_depth, 8 | 16),
        2 | 10 => matches!(pixel_depth, 15 | 16 | 24 | 32),
        3 | 11 => matches!(pixel_depth, 8 | 16),
        _ => false,
    };
    if !depth_is_valid {
        return false;
    }

    let is_color_mapped = matches!(image_type, 1 | 9);
    if is_color_mapped && color_map_type != 1 {
        return false;
    }

    if color_map_type == 1
        && (color_map_length == 0 || !matches!(color_map_depth, 15 | 16 | 24 | 32))
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tga_header(image_type: u8, depth: u8, width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[2] = image_type;
        bytes[12..14].copy_from_slice(&width.to_le_bytes());
        bytes[14..16].copy_from_slice(&height.to_le_bytes());
        bytes[16] = depth;
        bytes
    }

    #[test]
    fn detects_uncompressed_and_rle_true_color_tga() {
        for image_type in [2, 10] {
            assert!(looks_like_tga(&tga_header(image_type, 24, 1, 1)));
        }
    }

    #[test]
    fn detects_grayscale_and_color_mapped_tga() {
        for image_type in [3, 11] {
            assert!(looks_like_tga(&tga_header(image_type, 8, 1, 1)));
        }

        let mut color_mapped = tga_header(1, 8, 1, 1);
        color_mapped[1] = 1;
        color_mapped[5..7].copy_from_slice(&1u16.to_le_bytes());
        color_mapped[7] = 24;
        assert!(looks_like_tga(&color_mapped));
    }

    #[test]
    fn rejects_invalid_or_incomplete_tga_headers() {
        assert!(!looks_like_tga(&[0u8; 17]));
        assert!(!looks_like_tga(&tga_header(2, 24, 0, 1)));
        assert!(!looks_like_tga(&tga_header(2, 7, 1, 1)));
        assert!(!looks_like_tga(&tga_header(99, 24, 1, 1)));
    }
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
    path_hint: Option<&std::path::Path>,
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
    let extension_format = path_hint
        .and_then(std::path::Path::extension)
        .and_then(|extension| extension.to_str())
        .and_then(ImageFormat::from_extension);
    // RAW files often use a TIFF-based container, so the filename extension
    // must take precedence over generic TIFF signature detection.
    let format = extension_format
        .filter(|format| *format == ImageFormat::CameraRaw)
        .or_else(|| detect_format(&bytes))
        .or(extension_format)
        .ok_or_else(|| ImageError::UnknownFormat.to_string())?;

    // RAW development already produces a complete decoded frame. Decode once
    // and bypass virtual-texture routing (which consumes original source bytes).
    if format == ImageFormat::CameraRaw {
        return registry.decode(&bytes, format, options)
            .map(LoadedImage::Static)
            .map_err(|error| error.to_string());
    }

    // 3. Get dimensions (fast path)
    let (width, height) = match registry.dimensions(&bytes, format) {
        Ok(dim) => dim,
        Err(e) => return Err(e.to_string()),
    };

    // 4. Virtual‑texture routing
    // RAW development currently produces a complete developed frame; do not
    // route its source bytes into the generic virtual-texture decoder path.
    if format != ImageFormat::CameraRaw && threshold > 0 && (width >= threshold || height >= threshold) {
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
