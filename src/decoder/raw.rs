//! Full-resolution in-process camera RAW decoding.
//!
//! The main viewer must display the actual image, not the camera's small
//! embedded thumbnail/preview. Sidebar RAW thumbnails are intentionally
//! skipped separately in the thumbnail queue to avoid duplicate full RAW
//! development.

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};
use enough::Unstoppable;
use zenraw::{DemosaicMethod, OutputMode, RawDecodeConfig};

/// Decoder for digital-camera RAW formats.
pub struct CameraRawDecoder;

impl CameraRawDecoder {
    pub const fn new() -> Self {
        Self
    }

    /// Develop the RAW sensor data at its full decoded resolution.
    fn develop(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
        if bytes.is_empty() {
            return Err(ImageError::InvalidData("empty camera RAW file".into()));
        }

        // Probe the actual RAW dimensions first. This metadata-only operation
        // gives us a reference to detect accidentally displaying a thumbnail.
        let probed = zenraw::probe(bytes, &Unstoppable)
            .map_err(|error| ImageError::Decode(format!("camera RAW probe failed: {error}")))?;

        // Bilinear demosaicing favors browsing speed, but does not resize the
        // output or select an embedded JPEG preview. Explicitly request the
        // full sensor image budget so the config cannot silently be mistaken
        // for a preview-size setting.
        let config = RawDecodeConfig::default()
            .with_demosaic(DemosaicMethod::Bilinear)
            .with_output(OutputMode::Develop)
            .with_max_pixels(200_000_000);
        let output = zenraw::decode(bytes, &config, &Unstoppable)
            .map_err(|error| ImageError::Decode(format!("camera RAW decode failed: {error}")))?;

        let width = output.pixels.width();
        let height = output.pixels.height();
        let matches_probe = (width == probed.width && height == probed.height)
            || (width == probed.height && height == probed.width);
        if !matches_probe {
            return Err(ImageError::InvalidData(format!(
                "RAW decoder returned {width}x{height} pixels, but RAW metadata reports {}x{}; refusing to display a reduced embedded preview as the main image",
                probed.width, probed.height
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
        // RGBA layout used by the viewer, preserving all pixels at full size.
        let rgba_len = pixel_count
            .checked_mul(4)
            .ok_or_else(|| ImageError::InvalidData("RAW image dimensions overflow".into()))?;
        let mut rgba = vec![255u8; rgba_len];
        for (source, destination) in rgb16.chunks_exact(6).zip(rgba.chunks_exact_mut(4)) {
            destination[0] = (u16::from_ne_bytes([source[0], source[1]]) >> 8) as u8;
            destination[1] = (u16::from_ne_bytes([source[2], source[3]]) >> 8) as u8;
            destination[2] = (u16::from_ne_bytes([source[4], source[5]]) >> 8) as u8;
        }

        Ok(DecodedImage {
            width,
            height,
            data: rgba,
        })
    }
}

impl ImageDecoder for CameraRawDecoder {
    fn name(&self) -> &'static str {
        "camera-raw-zenraw"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::CameraRaw]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        // Always decode the RAW sensor data for the main viewer. Embedded JPEG
        // previews are often much smaller than the actual photo and must not
        // silently replace the full-resolution image.
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
