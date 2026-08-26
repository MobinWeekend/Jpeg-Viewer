// src/decoder/jpeg_xl.rs

use crate::image_core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};

use jxl_oxide::JxlImage;
use std::io::Cursor;

pub struct JpegXlDecoder;

impl JpegXlDecoder {
    pub fn new() -> Self {
        Self
    }

    /// Decode a JPEG XL image into the application's canonical RGBA8 format.
    fn decode_rgba8(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
        let image = JxlImage::builder()
            .read(Cursor::new(bytes))
            .map_err(|e| ImageError::Decode(format!("JPEG XL header parsing failed: {e}")))?;

        let width = image.width();
        let height = image.height();

        if width == 0 || height == 0 {
            return Err(ImageError::Decode(
                "JPEG XL image has zero dimensions".to_string(),
            ));
        }

        // Render the first keyframe.
        //
        // Render::stream() applies the image orientation automatically.
        let render = image
            .render_frame(0)
            .map_err(|e| ImageError::Decode(format!("JPEG XL frame rendering failed: {e}")))?;

        let mut stream = render.stream();

        let channels = stream.channels() as usize;

        if channels < 3 {
            return Err(ImageError::Decode(format!(
                "JPEG XL image has unsupported channel count: {channels}"
            )));
        }

        let width = stream.width();
        let height = stream.height();

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                ImageError::Decode("JPEG XL dimensions overflow pixel count".to_string())
            })?;

        let source_len = pixel_count.checked_mul(channels).ok_or_else(|| {
            ImageError::Decode("JPEG XL dimensions overflow source buffer size".to_string())
        })?;

        let mut source = vec![0u8; source_len];

        let written = stream.write_to_buffer(&mut source);

        if written != source.len() {
            return Err(ImageError::Decode(format!(
                "JPEG XL decoder wrote {written} bytes, expected {}",
                source.len()
            )));
        }

        let rgba_len = pixel_count.checked_mul(4).ok_or_else(|| {
            ImageError::Decode("JPEG XL dimensions overflow RGBA8 buffer size".to_string())
        })?;

        let mut rgba = Vec::with_capacity(rgba_len);

        match channels {
            // RGB
            3 => {
                for pixel in source.chunks_exact(3) {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }

            // RGBA
            4 => {
                rgba.extend_from_slice(&source);
            }

            // JPEG XL can contain additional channels such as black/extra
            // channels. We currently expose only the first three color
            // channels and alpha when it is the fourth channel.
            //
            // Do not silently treat arbitrary extra channels as alpha.
            _ => {
                return Err(ImageError::Decode(format!(
                    "JPEG XL image has {channels} rendered channels; \
                     only RGB/RGBA output is currently supported"
                )));
            }
        }

        if rgba.len() != rgba_len {
            return Err(ImageError::Decode(format!(
                "JPEG XL RGBA buffer size mismatch: got {}, expected {}",
                rgba.len(),
                rgba_len
            )));
        }

        Ok(DecodedImage {
            width,
            height,
            data: rgba,
        })
    }

    /// Read JPEG XL dimensions from the parsed image header.
    fn read_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        let image = JxlImage::builder()
            .read(Cursor::new(bytes))
            .map_err(|e| ImageError::Decode(format!("JPEG XL header parsing failed: {e}")))?;

        let width = image.width();
        let height = image.height();

        if width == 0 || height == 0 {
            return Err(ImageError::Decode(
                "JPEG XL image has zero dimensions".to_string(),
            ));
        }

        Ok((width, height))
    }
}

impl Default for JpegXlDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDecoder for JpegXlDecoder {
    fn name(&self) -> &'static str {
        "jxl-oxide"
    }

    fn supported_formats(&self) -> &'static [ImageFormat] {
        &[ImageFormat::JpegXl]
    }

    fn decode(&self, bytes: &[u8], _options: &DecodeOptions) -> Result<DecodedImage, ImageError> {
        Self::decode_rgba8(bytes)
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        Self::read_dimensions(bytes)
    }
}
