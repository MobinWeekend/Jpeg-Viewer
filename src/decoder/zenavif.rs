// src/decoder/zenavif.rs

use crate::imaging::core::{DecodeOptions, DecodedImage, ImageDecoder, ImageError, ImageFormat};

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use zenavif::{DecoderConfig, Unstoppable, decode_with};
use zenavif_parse::{AvifParser, DecodeConfig as ParseConfig};
use zenpixels_convert::PixelBufferConvertTypedExt;

const DECODE_STACK_SIZE: usize = 8 * 1024 * 1024;
const DECODE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ZenAvifDecoder {
    config: DecoderConfig,
}

impl ZenAvifDecoder {
    pub fn new() -> Self {
        Self {
            config: DecoderConfig::new()
                .prefer_8bit(true)
                .frame_size_limit(8192 * 8192)
                // Critical: 1 thread eliminates rav1d's DisjointMut race.
                // Single-threaded decode runs inline on the calling thread,
                // so we always run it inside decode_with_timeout's 8 MiB
                // worker thread to avoid the Windows stack overflow.
                .threads(1),
        }
    }

    /// Read image dimensions from AVIF container/sequence metadata without
    /// running the AV1 pixel decoder. This is important for thumbnail jobs: a
    /// native OS preview should remain usable even when the source image is
    /// too large for the full decoder's safety limits.
    fn metadata_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        // Parsing metadata does not allocate a full image buffer. Keep a parser
        // memory limit and tile/frame bounds, but do not apply the pixel limit
        // used for full decode to metadata discovery.
        let parse_config = ParseConfig::unlimited()
            .with_peak_memory_limit(256 * 1024 * 1024)
            .with_max_animation_frames(1)
            .with_max_grid_tiles(64);

        let parsed = catch_unwind(AssertUnwindSafe(|| {
            AvifParser::from_bytes_with_config(bytes, &parse_config, &zenavif_parse::Unstoppable)
        }));
        let parser = match parsed {
            Ok(Ok(parser)) => parser,
            Ok(Err(error)) => {
                return Err(ImageError::Decode(format!("AVIF metadata probe: {error}")));
            }
            Err(_) => {
                return Err(ImageError::Decode("AVIF metadata probe panicked".to_string()));
            }
        };

        let metadata = parser
            .primary_metadata()
            .map_err(|error| ImageError::Decode(format!("AVIF sequence metadata: {error}")))?;
        let (mut width, mut height) = if let Some(grid) = parser.grid_config() {
            if grid.output_width > 0 && grid.output_height > 0 {
                (grid.output_width, grid.output_height)
            } else {
                let width = metadata
                    .max_frame_width
                    .get()
                    .checked_mul(u32::from(grid.columns))
                    .ok_or_else(|| ImageError::Decode("AVIF grid width overflow".to_string()))?;
                let height = metadata
                    .max_frame_height
                    .get()
                    .checked_mul(u32::from(grid.rows))
                    .ok_or_else(|| ImageError::Decode("AVIF grid height overflow".to_string()))?;
                (width, height)
            }
        } else {
            (metadata.max_frame_width.get(), metadata.max_frame_height.get())
        };

        if width == 0 || height == 0 {
            return Err(ImageError::Decode(
                "AVIF metadata reported empty dimensions".to_string(),
            ));
        }

        // The AVIF irot property is applied after decode. Reflect that in the
        // reported display dimensions for quarter-turn rotations.
        if parser
            .rotation()
            .is_some_and(|rotation| matches!(rotation.angle, 90 | 270))
        {
            std::mem::swap(&mut width, &mut height);
        }

        Ok((width, height))
    }

    fn pre_validate(bytes: &[u8]) -> Result<(), ImageError> {
        let parse_config = ParseConfig::default()
            .with_peak_memory_limit(256 * 1024 * 1024)
            .with_total_megapixels_limit(64)
            .with_max_animation_frames(1)
            .with_max_grid_tiles(64);

        let result = catch_unwind(AssertUnwindSafe(|| {
            AvifParser::from_bytes_with_config(bytes, &parse_config, &zenavif_parse::Unstoppable)
        }));

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ImageError::Decode(format!("AVIF pre-validation: {e}"))),
            Err(_) => Err(ImageError::Decode(
                "AVIF pre-validation panicked".to_string(),
            )),
        }
    }

    fn decode_with_timeout(
        bytes: Vec<u8>,
        config: DecoderConfig,
    ) -> Result<DecodedImage, ImageError> {
        let (tx, rx) = mpsc::channel();

        thread::Builder::new()
            .stack_size(DECODE_STACK_SIZE)
            .spawn(move || {
                // n_threads=1 means decode runs inline here, on our 8 MiB stack.
                let result = catch_unwind(AssertUnwindSafe(|| {
                    decode_with(&bytes, &config, &Unstoppable)
                }));

                let image = match result {
                    Ok(Ok(img)) => img,
                    Ok(Err(e)) => {
                        let _ = tx.send(Err(ImageError::Decode(format!("zenavif decode: {e}"))));
                        return;
                    }
                    Err(_) => {
                        let _ = tx.send(Err(ImageError::Decode(
                            "zenavif decode panicked".to_string(),
                        )));
                        return;
                    }
                };

                let width = image.width();
                let height = image.height();
                let rgba = image.to_rgba8();
                let data = rgba.copy_to_contiguous_bytes();

                let expected_len = width
                    .checked_mul(height)
                    .and_then(|p| p.checked_mul(4))
                    .map(|n| n as usize);

                match expected_len {
                    Some(expected) if data.len() == expected => {
                        let _ = tx.send(Ok(DecodedImage {
                            width,
                            height,
                            data,
                        }));
                    }
                    Some(expected) => {
                        let _ = tx.send(Err(ImageError::Decode(format!(
                            "zenavif returned {} bytes, expected {}",
                            data.len(),
                            expected
                        ))));
                    }
                    None => {
                        let _ = tx.send(Err(ImageError::Decode("dimensions overflow".to_string())));
                    }
                }
            })
            .map_err(|e| ImageError::Decode(format!("spawn decode thread: {e}")))?;

        match rx.recv_timeout(DECODE_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ImageError::Decode(format!(
                "AVIF decode exceeded {}s timeout (possibly malformed bitstream)",
                DECODE_TIMEOUT.as_secs()
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ImageError::Decode(
                "decode thread died unexpectedly".to_string(),
            )),
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
        Self::pre_validate(bytes)?;
        Self::decode_with_timeout(bytes.to_vec(), self.config.clone())
    }

    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        Self::metadata_dimensions(bytes)
    }
}
