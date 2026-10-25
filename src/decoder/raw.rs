//! Full-resolution in-process camera RAW decoding.
//!
//! The main viewer develops the actual RAW sensor data at full resolution.
//! Embedded JPEG previews remain reserved for sidebar thumbnails.

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};
use enough::Unstoppable;
use zenraw::{DemosaicMethod, OutputMode, OutputPrimaries, RawDecodeConfig};

/// Decoder for digital-camera RAW formats.
pub struct CameraRawDecoder;

impl CameraRawDecoder {
    pub const fn new() -> Self {
        Self
    }

    /// Develop the RAW sensor data and return a full-resolution display image.
    fn develop(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
        if bytes.is_empty() {
            return Err(ImageError::InvalidData("empty camera RAW file".into()));
        }

        // Develop the actual sensor data. The decoder output includes the
        // resolved image metadata, so normal display decoding does not need a
        // separate metadata probe before the expensive RAW decode.
        let config = RawDecodeConfig::default()
            // Use the higher-quality Bayer interpolation algorithm for the
            // main image. Bilinear can be faster, but may amplify false color
            // and color artifacts around fine detail.
            .with_demosaic(DemosaicMethod::MalvarHeCutler)
            .with_output(OutputMode::Develop)
            .with_target(OutputPrimaries::Srgb)
            .with_max_pixels(200_000_000);
        let output = zenraw::decode(bytes, &config, &Unstoppable)
            .map_err(|error| ImageError::Decode(format!("camera RAW decode failed: {error}")))?;

        let width = output.pixels.width();
        let height = output.pixels.height();
        if width != output.info.width || height != output.info.height {
            return Err(ImageError::InvalidData(format!(
                "RAW decoder returned a {width}x{height} pixel buffer, but its decode metadata reports {}x{}",
                output.info.width, output.info.height
            )));
        }
        let rgb16 = output.pixels.into_vec();
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::InvalidData("RAW image dimensions overflow".into()))?;
        let expected_rgb16_len = pixel_count
            .checked_mul(6)
            .ok_or_else(|| ImageError::InvalidData("RAW image dimensions overflow".into()))?;
        if rgb16.len() != expected_rgb16_len {
            return Err(ImageError::InvalidData(format!(
                "RAW decoder returned {} bytes for a {width}x{height} 16-bit RGB image (expected {expected_rgb16_len})",
                rgb16.len()
            )));
        }

        // zenraw returns interleaved 16-bit RGB bytes. Convert to the 8-bit
        // RGBA layout used by the viewer.
        let rgba_len = pixel_count
            .checked_mul(4)
            .ok_or_else(|| ImageError::InvalidData("RAW image dimensions overflow".into()))?;
        let mut rgba = vec![255u8; rgba_len];
        for (source, destination) in rgb16.chunks_exact(6).zip(rgba.chunks_exact_mut(4)) {
            // Develop mode is interleaved native-endian RGB16 sRGB. Scale the
            // full 0..65535 range to 0..255 instead of discarding the low byte.
            let r = u16::from_ne_bytes([source[0], source[1]]) as u32;
            let g = u16::from_ne_bytes([source[2], source[3]]) as u32;
            let b = u16::from_ne_bytes([source[4], source[5]]) as u32;
            destination[0] = ((r * 255 + 32_767) / 65_535) as u8;
            destination[1] = ((g * 255 + 32_767) / 65_535) as u8;
            destination[2] = ((b * 255 + 32_767) / 65_535) as u8;
        }

        Ok(DecodedImage {
            width,
            height,
            data: rgba,
        })
    }
}

/// Extract an embedded JPEG directly from the RAW byte stream.
///
/// The scanner walks JPEG marker segments (including entropy-coded scan data)
/// instead of treating every `FF D8` / `FF D9` pair as a file boundary. Each
/// candidate must have a valid marker structure and a readable JPEG header.
/// The candidate with the largest pixel area is used for the main viewport.

#[allow(dead_code)] //This acctually gets used!
pub fn extract_raw_preview(bytes: &[u8]) -> Option<Vec<u8>> {
    extract_raw_preview_diagnostic(bytes, None).ok().flatten()
}

/// Diagnostic variant used by the main loading path. `source_path` is retained
/// for call-site compatibility; extraction operates on the bytes, so it also
/// works for RAW files stored inside archives.
pub fn extract_raw_preview_diagnostic(
    bytes: &[u8],
    _source_path: Option<&std::path::Path>,
) -> Result<Option<Vec<u8>>, String> {
    if bytes.is_empty() {
        return Err("RAW source is empty".into());
    }

    if let Some(jpeg) = find_embedded_jpeg(bytes, JpegSelection::Largest) {
        eprintln!(
            "[RAW preview] embedded JPEG scanner extracted largest candidate ({} bytes)",
            jpeg.len()
        );
        return Ok(Some(jpeg));
    }

    eprintln!("[RAW preview] no valid embedded JPEG found by scanner; trying Rawler");
    extract_raw_preview_with_rawler(bytes)
}

#[derive(Clone, Copy)]
enum JpegSelection {
    Largest,
    Smallest,
}

/// Find the largest or smallest valid embedded JPEG by pixel area.
///
/// Advancing to the end of a successfully parsed JPEG avoids mistaking marker
/// patterns inside its metadata payload for separate embedded images. If a
/// candidate is malformed, resume one byte after its SOI and keep searching.
fn find_embedded_jpeg(bytes: &[u8], selection: JpegSelection) -> Option<Vec<u8>> {
    let mut search_from = 0usize;
    let mut best: Option<(usize, usize, usize, usize)> = None; // start, end, width, height

    while search_from + 1 < bytes.len() {
        let relative = bytes[search_from..]
            .windows(2)
            .position(|pair| pair == [0xFF, 0xD8]);
        let Some(relative) = relative else { break };
        let start = search_from + relative;

        match parse_embedded_jpeg(bytes, start) {
            Some((end, width, height))
                if jpeg_header_dimensions(&bytes[start..end]) == Some((width, height)) =>
            {
                let candidate_area = (width as u64) * (height as u64);
                let replace = match best {
                    None => true,
                    Some((best_start, best_end, best_width, best_height)) => {
                        let best_area = (best_width as u64) * (best_height as u64);
                        match selection {
                            JpegSelection::Largest => {
                                candidate_area > best_area
                                    || (candidate_area == best_area
                                        && end - start > best_end - best_start)
                            }
                            JpegSelection::Smallest => {
                                candidate_area < best_area
                                    || (candidate_area == best_area
                                        && end - start < best_end - best_start)
                            }
                        }
                    }
                };
                if replace {
                    best = Some((start, end, width as usize, height as usize));
                }
                search_from = end;
            }
            Some(_) | None => search_from = start + 1,
        }
    }

    best.map(|(start, end, _, _)| bytes[start..end].to_vec())
}

/// Parse one JPEG from SOI to EOI, returning its exclusive end offset and SOF
/// dimensions. JPEG scan data is byte-stuffed: `FF 00` and restart markers do
/// not terminate a scan. Progressive JPEGs may contain multiple SOS segments.
fn parse_embedded_jpeg(bytes: &[u8], start: usize) -> Option<(usize, u32, u32)> {
    if bytes.get(start..start.checked_add(2)?)? != [0xFF, 0xD8] {
        return None;
    }

    let mut pos = start + 2;
    let mut in_scan = false;
    let mut dimensions: Option<(u32, u32)> = None;
    let mut saw_scan = false;

    loop {
        if in_scan {
            // Entropy-coded data has no length field. Only unstuffed marker
            // codes end the scan; restart markers are part of the scan.
            let mut found_marker = false;
            while pos < bytes.len() {
                if bytes[pos] != 0xFF {
                    pos += 1;
                    continue;
                }
                let marker_start = pos;
                pos += 1;
                while pos < bytes.len() && bytes[pos] == 0xFF {
                    pos += 1;
                }
                let marker = *bytes.get(pos)?;
                pos += 1;
                if marker == 0x00 || (0xD0..=0xD7).contains(&marker) {
                    continue;
                }
                // The marker belongs to the outer marker stream. Rewind to
                // its first FF so the normal segment parser handles it.
                pos = marker_start;
                in_scan = false;
                found_marker = true;
                break;
            }
            if !found_marker {
                return None;
            }
        }

        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        while pos < bytes.len() && bytes[pos] == 0xFF {
            pos += 1;
        }
        let marker = *bytes.get(pos)?;
        pos += 1;

        match marker {
            0xD9 => {
                let (width, height) = dimensions?;
                if !saw_scan || width == 0 || height == 0 {
                    return None;
                }
                return Some((pos, width, height));
            }
            0xD8 => return None, // nested SOI before EOI is not a single JPEG
            0x01 | 0xD0..=0xD7 => continue, // standalone markers have no length
            0x00 => return None, // stuffed FF00 is only legal inside scan data
            _ => {
                let length_hi = *bytes.get(pos)? as usize;
                let length_lo = *bytes.get(pos + 1)? as usize;
                let segment_len = (length_hi << 8) | length_lo;
                if segment_len < 2 {
                    return None;
                }
                let segment_start = pos.checked_add(2)?;
                let segment_end = pos.checked_add(segment_len)?;
                if segment_end > bytes.len() {
                    return None;
                }

                if is_start_of_frame(marker) {
                    // SOF payload: precision (1), height (2), width (2), components (1).
                    if segment_len < 8 {
                        return None;
                    }
                    let height = u16::from_be_bytes([
                        *bytes.get(segment_start + 1)?,
                        *bytes.get(segment_start + 2)?,
                    ]) as u32;
                    let width = u16::from_be_bytes([
                        *bytes.get(segment_start + 3)?,
                        *bytes.get(segment_start + 4)?,
                    ]) as u32;
                    if width == 0 || height == 0 {
                        return None;
                    }
                    dimensions = Some((width, height));
                }

                pos = segment_end;
                if marker == 0xDA {
                    saw_scan = true;
                    in_scan = true;
                }
            }
        }
    }
}

fn is_start_of_frame(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF)
}

/// Ask the existing JPEG decoder to parse the candidate header. This rejects
/// marker-like data that happens to resemble a structurally plausible JPEG.
fn jpeg_header_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg)
        .into_dimensions()
        .ok()
}

/// Try Rawler's format-aware preview APIs if direct embedded-JPEG extraction
/// fails. The methods are independent because camera decoders expose different
/// preview layouts; return the first usable result.
fn extract_raw_preview_with_rawler(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    use std::sync::{Arc, mpsc};

    #[derive(Clone, Copy)]
    enum PreviewMethod {
        Preview,
        Full,
        Thumbnail,
    }

    impl PreviewMethod {
        fn label(self) -> &'static str {
            match self {
                Self::Preview => "preview_image()",
                Self::Full => "full_image()",
                Self::Thumbnail => "thumbnail_image()",
            }
        }
    }

    let bytes = Arc::new(bytes.to_vec());
    let (tx, rx) = mpsc::channel();
    for method in [
        PreviewMethod::Preview,
        PreviewMethod::Full,
        PreviewMethod::Thumbnail,
    ] {
        let bytes = Arc::clone(&bytes);
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Option<Vec<u8>>, String> {
                let source = rawler::rawsource::RawSource::new_from_slice(bytes.as_slice());
                let decoder = rawler::get_decoder(&source).map_err(|error| {
                    format!("Rawler could not identify the RAW decoder: {error}")
                })?;
                let params = rawler::decoders::RawDecodeParams::default();
                let image = match method {
                    PreviewMethod::Preview => decoder.preview_image(&source, &params),
                    PreviewMethod::Full => decoder.full_image(&source, &params),
                    PreviewMethod::Thumbnail => decoder.thumbnail_image(&source, &params),
                }
                .map_err(|error| format!("{} failed: {error}", method.label()))?;

                match image {
                    Some(image) => encode_embedded_image(image).map(Some).map_err(|error| {
                        format!("{} could not be encoded: {error}", method.label())
                    }),
                    None => Ok(None),
                }
            })();
            let _ = tx.send((method, result));
        });
    }
    drop(tx);

    let mut diagnostics = Vec::new();
    for _ in 0..3 {
        let (method, result) = rx
            .recv()
            .map_err(|error| format!("RAW preview workers stopped unexpectedly: {error}"))?;
        match result {
            Ok(Some(encoded)) => {
                eprintln!(
                    "[RAW preview] Rawler {} fallback returned an image ({} bytes)",
                    method.label(),
                    encoded.len()
                );
                return Ok(Some(encoded));
            }
            Ok(None) => diagnostics.push(format!("{} returned no image", method.label())),
            Err(error) => diagnostics.push(error),
        }
    }

    if diagnostics
        .iter()
        .all(|item| item.contains("returned no image"))
    {
        Ok(None)
    } else {
        Err(diagnostics.join("; "))
    }
}

/// Extract the smallest embedded JPEG for a sidebar thumbnail, falling back
/// to Rawler's format-aware thumbnail API if the byte scanner finds none.
pub fn extract_raw_thumbnail(
    bytes: &[u8],
    _source_path: Option<&std::path::Path>,
) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }

    if let Some(jpeg) = find_embedded_jpeg(bytes, JpegSelection::Smallest) {
        eprintln!(
            "[RAW thumbnail] embedded JPEG scanner extracted smallest candidate ({} bytes)",
            jpeg.len()
        );
        return Some(jpeg);
    }

    eprintln!("[RAW thumbnail] no valid embedded JPEG found by scanner; trying Rawler");
    extract_raw_thumbnail_with_rawler(bytes)
}

fn extract_raw_thumbnail_with_rawler(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }

    let source = rawler::rawsource::RawSource::new_from_slice(bytes);
    let decoder = match rawler::get_decoder(&source) {
        Ok(decoder) => decoder,
        Err(error) => {
            eprintln!("[RAW thumbnail] Rawler could not identify the RAW decoder: {error}");
            return None;
        }
    };
    let params = rawler::decoders::RawDecodeParams::default();
    let image = match decoder.thumbnail_image(&source, &params) {
        Ok(Some(image)) => image,
        Ok(None) => {
            eprintln!("[RAW thumbnail] Rawler thumbnail_image() returned no image");
            return None;
        }
        Err(error) => {
            eprintln!("[RAW thumbnail] Rawler thumbnail_image() failed: {error}");
            return None;
        }
    };

    match encode_embedded_image(image) {
        Ok(encoded) => Some(encoded),
        Err(error) => {
            eprintln!("[RAW thumbnail] Could not encode Rawler thumbnail: {error}");
            None
        }
    }
}

fn encode_embedded_image(image: image::DynamicImage) -> Result<Vec<u8>, String> {
    // Rawler returns decoded pixels (DynamicImage), not necessarily the
    // original JPEG bytes. Encode them to fit the existing byte-based loader.
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .map_err(|error| error.to_string())?;
    Ok(encoded.into_inner())
}

impl ImageDecoder for CameraRawDecoder {
    fn name(&self) -> &'static str {
        "camera-raw-zenraw"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::CameraRaw]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        // Decode actual sensor data at full resolution; embedded JPEG previews
        // are never used in the main view.
        Self::develop(bytes)
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        if bytes.is_empty() {
            return Err(ImageError::InvalidData("empty camera RAW file".into()));
        }

        // Metadata-only probe: this does not demosaic or develop the image.
        let info = zenraw::probe(bytes, &Unstoppable)
            .map_err(|error| ImageError::Decode(format!("camera RAW probe failed: {error}")))?;
        Ok((info.width, info.height))
    }
}

#[cfg(test)]
mod preview_tests {
    use super::{extract_raw_preview, extract_raw_thumbnail};

    #[test]
    fn rejects_empty_or_unrecognized_data() {
        assert!(extract_raw_preview(b"").is_none());
        assert!(extract_raw_preview(b"not a camera RAW file").is_none());
        assert!(extract_raw_thumbnail(b"", None).is_none());
        assert!(extract_raw_thumbnail(b"not a camera RAW file", None).is_none());
    }

    #[test]
    fn parses_jpeg_markers_and_stuffed_scan_bytes() {
        use super::parse_embedded_jpeg;

        // Minimal marker structure for a 16x8 JPEG. The scan includes FF00
        // (stuffed data) and FFD0 (restart marker), neither of which ends it.
        let jpeg = [
            0xFF, 0xD8, // SOI
            0xFF, 0xC0, 0x00, 0x0B, // SOF0, length 11
            0x08, 0x00, 0x08, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xDA, 0x00,
            0x08, // SOS, length 8
            0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF,
            0xD9, // EOI
        ];
        assert_eq!(parse_embedded_jpeg(&jpeg, 0), Some((jpeg.len(), 16, 8)));
    }
}
