//! Working-copy image editing and validated, recoverable file output.
use image::{DynamicImage, ImageEncoder, ImageFormat, RgbaImage};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_HISTORY_STEPS: usize = 10;
const MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;
pub const ADJUSTMENT_PREVIEW_PIXELS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageAdjustments {
    /// -100..=100
    pub brightness: i32,
    /// -180..=180 degrees
    pub hue: i32,
    /// 0..=200 percent, where 100 is unchanged
    pub saturation: i32,
    /// -100..=100
    pub contrast: i32,
    /// When enabled, brightness and contrast use independent RGB channel values.
    pub per_channel: bool,
    pub brightness_rgb: [i32; 3],
    pub contrast_rgb: [i32; 3],
}

impl Default for ImageAdjustments {
    fn default() -> Self {
        Self {
            brightness: 0,
            hue: 0,
            saturation: 100,
            contrast: 0,
            per_channel: false,
            brightness_rgb: [0; 3],
            contrast_rgb: [0; 3],
        }
    }
}

impl ImageAdjustments {
    pub fn is_default(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditOutputFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
    Tiff,
    Bmp,
    Tga,
    Qoi,
}

impl EditOutputFormat {
    pub const ALL: [Self; 7] = [
        Self::Png,
        Self::Jpeg,
        Self::WebP,
        Self::Tiff,
        Self::Bmp,
        Self::Tga,
        Self::Qoi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
            Self::Bmp => "BMP",
            Self::Tga => "TGA",
            Self::Qoi => "QOI",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Tga => "tga",
            Self::Qoi => "qoi",
        }
    }

    /*
    pub fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::WebP => ImageFormat::WebP,
            Self::Tiff => ImageFormat::Tiff,
            Self::Bmp => ImageFormat::Bmp,
            Self::Tga => ImageFormat::Tga,
            Self::Qoi => ImageFormat::Qoi,
        }
    }
     */
}

#[derive(Clone)]
struct EditSnapshot {
    image: Arc<RgbaImage>,
    adjustments: ImageAdjustments,
}

pub struct EditSession {
    pub source_path: PathBuf,
    /// Shared original pixels avoid allocating a second full-resolution copy at edit start.
    pub original: Arc<RgbaImage>,
    /// Geometric edits are stored here. Colour adjustments are non-destructive
    /// until saved and are applied when rendering or writing the output.
    pub image: Arc<RgbaImage>,
    pub adjustments: ImageAdjustments,
    undo: Vec<EditSnapshot>,
    redo: Vec<EditSnapshot>,
    /// Cached downsampled source used repeatedly while colour sliders are dragged.
    /// Invalidated whenever the underlying working image changes.
    preview_base: Option<(u64, RgbaImage)>,
    structural_edit_applied: bool,
    pub dirty: bool,
}

impl EditSession {
    pub fn new(source_path: PathBuf, image: RgbaImage) -> Self {
        let image = Arc::new(image);
        Self {
            source_path,
            original: image.clone(),
            image,
            adjustments: ImageAdjustments::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            preview_base: None,
            structural_edit_applied: false,
            dirty: false,
        }
    }

    fn push_undo_snapshot(&mut self, snapshot: EditSnapshot) {
        self.undo.push(snapshot);
        self.redo.clear();
        while self.undo.len() > MAX_HISTORY_STEPS
            || self
                .undo
                .iter()
                .map(|entry| entry.image.as_raw().len())
                .sum::<usize>()
                > MAX_HISTORY_BYTES
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
    }

    pub fn record_current_state(&mut self) {
        self.push_undo_snapshot(EditSnapshot {
            image: self.image.clone(),
            adjustments: self.adjustments,
        });
    }

    fn replace_image(&mut self, image: RgbaImage) {
        self.record_current_state();
        self.image = Arc::new(image);
        self.preview_base = None;
        self.update_dirty();
    }

    /// Structural edits establish a new pixel baseline. Keep adjustment history,
    /// but rebase its snapshots onto the new geometry so undo never restores a
    /// pre-crop/pre-resize image. Pure transform snapshots with no adjustment
    /// change are discarded at this boundary.
    fn rebase_adjustment_history(&mut self, image: &Arc<RgbaImage>) {
        let current_adjustments = self.adjustments;
        self.undo.retain(|snapshot| snapshot.adjustments != current_adjustments);
        self.redo.retain(|snapshot| snapshot.adjustments != current_adjustments);
        for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            snapshot.image = image.clone();
        }
    }

    fn update_dirty(&mut self) {
        self.dirty = self.image.as_raw() != self.original.as_raw() || !self.adjustments.is_default();
    }

    pub fn set_adjustments(&mut self, adjustments: ImageAdjustments) {
        self.adjustments = ImageAdjustments {
            brightness: adjustments.brightness.clamp(-100, 100),
            hue: adjustments.hue.clamp(-180, 180),
            saturation: adjustments.saturation.clamp(0, 200),
            contrast: adjustments.contrast.clamp(-100, 100),
            per_channel: adjustments.per_channel,
            brightness_rgb: adjustments.brightness_rgb.map(|v| v.clamp(-100, 100)),
            contrast_rgb: adjustments.contrast_rgb.map(|v| v.clamp(-100, 100)),
        };
        self.update_dirty();
    }

    pub fn rotate_left(&mut self) {
        self.replace_image(image::imageops::rotate270(&*self.image));
    }

    pub fn rotate_right(&mut self) {
        self.replace_image(image::imageops::rotate90(&*self.image));
    }

    pub fn flip_horizontal(&mut self) {
        self.replace_image(image::imageops::flip_horizontal(&*self.image));
    }

    pub fn flip_vertical(&mut self) {
        self.replace_image(image::imageops::flip_vertical(&*self.image));
    }

    /// Resize the working copy. Resizing is a geometric edit and participates in undo/redo.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("Image width and height must be greater than zero.".into());
        }
        let pixels = u64::from(width).checked_mul(u64::from(height))
            .ok_or_else(|| "The requested dimensions are too large.".to_string())?;
        let bytes = pixels.checked_mul(4)
            .ok_or_else(|| "The requested image would exceed addressable memory.".to_string())?;
        if bytes > usize::MAX as u64 {
            return Err("The requested image would exceed addressable memory.".into());
        }
        if self.image.dimensions() == (width, height) {
            return Ok(());
        }
        let resized = image::imageops::resize(
            &*self.image,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        );
        // Crop/resize are structural edits: they deliberately do not enter the
        // step-by-step undo history. Rebase value-history snapshots so undo cannot
        // accidentally restore pixels from before a structural edit.
        self.image = Arc::new(resized);
        self.preview_base = None;
        self.structural_edit_applied = true;
        let image = self.image.clone();
        self.rebase_adjustment_history(&image);
        self.update_dirty();
        Ok(())
    }

    pub fn crop(&mut self, x: u32, y: u32, width: u32, height: u32) -> Result<(), String> {
        let image_width = self.image.width();
        let image_height = self.image.height();
        if width == 0 || height == 0 {
            return Err("Crop width and height must be greater than zero.".into());
        }
        let right = x.checked_add(width).ok_or("Crop bounds overflow.")?;
        let bottom = y.checked_add(height).ok_or("Crop bounds overflow.")?;
        if right > image_width || bottom > image_height {
            return Err(format!(
                "Crop must fit inside the image ({} × {}).",
                image_width, image_height
            ));
        }
        let cropped = image::imageops::crop_imm(&*self.image, x, y, width, height).to_image();
        // Structural edits are restored with Restore to original size, not Undo.
        self.image = Arc::new(cropped);
        self.preview_base = None;
        self.structural_edit_applied = true;
        let image = self.image.clone();
        self.rebase_adjustment_history(&image);
        self.update_dirty();
        Ok(())
    }

    /// Restore the source pixels and original dimensions while preserving all
    /// non-destructive adjustment values. This is intentionally not Reset All.
    pub fn restore_to_original_size(&mut self) {
        self.image = self.original.clone();
        self.preview_base = None;
        self.structural_edit_applied = false;
        let image = self.image.clone();
        self.rebase_adjustment_history(&image);
        self.update_dirty();
    }

    pub fn has_structural_edits(&self) -> bool {
        self.structural_edit_applied
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(EditSnapshot {
            image: self.image.clone(),
            adjustments: self.adjustments,
        });
        self.image = previous.image;
        self.preview_base = None;
        self.adjustments = previous.adjustments;
        self.update_dirty();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(EditSnapshot {
            image: self.image.clone(),
            adjustments: self.adjustments,
        });
        while self.undo.len() > MAX_HISTORY_STEPS
            || self
                .undo
                .iter()
                .map(|entry| entry.image.as_raw().len())
                .sum::<usize>()
                > MAX_HISTORY_BYTES
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
        self.image = next.image;
        self.preview_base = None;
        self.adjustments = next.adjustments;
        self.update_dirty();
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn reset(&mut self) {
        if self.dirty {
            self.image = self.original.clone();
            self.preview_base = None;
            self.structural_edit_applied = false;
            self.adjustments = ImageAdjustments::default();
            self.undo.clear();
            self.redo.clear();
            self.update_dirty();
        }
    }

    pub fn materialized_image(&self) -> RgbaImage {
        let mut image = (*self.image).clone();
        apply_adjustments(&mut image, self.adjustments);
        image
    }

    /// Build a lower-cost preview while a slider is being dragged. The geometry
    /// and aspect ratio stay unchanged, so the viewer can keep its zoom/pan.
    pub fn preview_image(&mut self, max_pixels: u64) -> RgbaImage {
        // Resizing the full-resolution image on every slider event was a major
        // source of latency. Cache the unadjusted preview base and only clone,
        // process, and upload this small buffer as slider values change.
        let cache_matches = self
            .preview_base
            .as_ref()
            .is_some_and(|(limit, _)| *limit == max_pixels);
        if !cache_matches {
            let pixels = u64::from(self.image.width()) * u64::from(self.image.height());
            let base = if max_pixels > 0 && pixels > max_pixels {
                let scale = (max_pixels as f64 / pixels as f64).sqrt();
                let width = ((f64::from(self.image.width()) * scale).round() as u32).max(1);
                let height = ((f64::from(self.image.height()) * scale).round() as u32).max(1);
                image::imageops::resize(
                    &*self.image,
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
            } else {
                (*self.image).clone()
            };
            self.preview_base = Some((max_pixels, base));
        }
        let mut image = self.preview_base.as_ref().expect("preview cache initialized").1.clone();
        apply_adjustments(&mut image, self.adjustments);
        image
    }

    pub fn mark_saved(&mut self, image: RgbaImage) {
        let image = Arc::new(image);
        self.image = image.clone();
        self.preview_base = None;
        self.original = image;
        self.structural_edit_applied = false;
        self.adjustments = ImageAdjustments::default();
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
    }
}

fn apply_adjustments(image: &mut RgbaImage, adjustments: ImageAdjustments) {
    if adjustments.is_default() {
        return;
    }

    // Process fixed-height bands. This bounds temporary working data and avoids
    // constructing any additional full-image intermediate buffers.
    const TILE_ROWS: u32 = 256;
    let width_bytes = image.width() as usize * 4;
    let rows_per_tile = TILE_ROWS as usize;
    let bytes = image.as_mut();
    for tile in bytes.chunks_mut(width_bytes * rows_per_tile) {
        apply_adjustment_bytes(tile, adjustments);
    }
}

fn apply_adjustment_bytes(bytes: &mut [u8], adjustments: ImageAdjustments) {
    use rayon::prelude::*;
    let brightness = adjustments.brightness as f32 * 2.55;
    let contrast = (100.0 + adjustments.contrast as f32) / 100.0;
    let channel_brightness = adjustments.brightness_rgb.map(|v| v as f32 * 2.55);
    let channel_contrast = adjustments.contrast_rgb.map(|v| (100.0 + v as f32) / 100.0);
    let saturation_multiplier = adjustments.saturation as f32 / 100.0;

    bytes.par_chunks_mut(4).for_each(|pixel| {
        let alpha = pixel[3];
        let mut rgb = [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32];
        for index in 0..3 {
            let (c, b) = if adjustments.per_channel {
                (channel_contrast[index], channel_brightness[index])
            } else { (contrast, brightness) };
            rgb[index] = ((rgb[index] - 127.5) * c + 127.5 + b).clamp(0.0, 255.0);
        }
        if adjustments.hue == 0 && adjustments.saturation == 100 {
            pixel[0] = rgb[0].round() as u8;
            pixel[1] = rgb[1].round() as u8;
            pixel[2] = rgb[2].round() as u8;
            pixel[3] = alpha;
            return;
        }
        let r = rgb[0] / 255.0;
        let g = rgb[1] / 255.0;
        let b = rgb[2] / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let mut hue = if delta <= f32::EPSILON {
            0.0
        } else if (max - r).abs() < f32::EPSILON {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if (max - g).abs() < f32::EPSILON {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        hue = (hue + adjustments.hue as f32).rem_euclid(360.0);
        let saturation = if max <= f32::EPSILON { 0.0 } else {
            (delta / max * saturation_multiplier).clamp(0.0, 1.0)
        };
        let value = max;
        let chroma = value * saturation;
        let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = value - chroma;
        let (r1, g1, b1) = match (hue / 60.0).floor() as i32 {
            0 => (chroma, x, 0.0), 1 => (x, chroma, 0.0), 2 => (0.0, chroma, x),
            3 => (0.0, x, chroma), 4 => (x, 0.0, chroma), _ => (chroma, 0.0, x),
        };
        pixel[0] = ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        pixel[1] = ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        pixel[2] = ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        pixel[3] = alpha;
    });
}

/// Encode and validate output before touching the destination. Quality affects
/// JPEG output; other formats use their standard encoder settings.

#[allow(dead_code)] //This acctually gets used!
pub fn save_image_safely(
    image: &RgbaImage,
    destination: &Path,
    overwrite: bool,
) -> Result<(), String> {
    save_image_safely_with_quality(image, destination, overwrite, 90)
}

pub fn save_image_safely_with_quality(
    image: &RgbaImage,
    destination: &Path,
    overwrite: bool,
    quality: u8,
) -> Result<(), String> {
    let format = ImageFormat::from_path(destination)
        .map_err(|_| "The destination needs a supported image extension.".to_string())?;
    if !is_supported_output_format(format) {
        return Err(format!(
            "Saving as {:?} is not supported by this editor.",
            format
        ));
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("The destination folder does not exist.".into());
    }
    if destination.exists() && !overwrite {
        return Err("The destination already exists; confirm overwrite first.".into());
    }

    // Encode directly to a temporary file instead of building a second, potentially
    // multi-gigabyte compressed Vec. Validate only the encoded dimensions; decoding
    // the entire output just to validate it would duplicate the image in memory.
    let temp_path = unique_sibling_path(destination, "tmp");
    let backup_path = unique_sibling_path(destination, "backup");
    let mut temp_file = File::options()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("Could not create temporary output: {e}"))?;

    let write_result = (|| -> Result<(), String> {
        encode_image_to_writer(image, format, quality, &mut temp_file)?;
        temp_file
            .sync_all()
            .map_err(|e| format!("Could not flush temporary output: {e}"))?;
        drop(temp_file);

        let dimensions = image::ImageReader::open(&temp_path)
            .map_err(|e| format!("Could not reopen temporary output: {e}"))?
            .with_guessed_format()
            .map_err(|e| format!("Could not identify temporary output: {e}"))?
            .into_dimensions()
            .map_err(|e| format!("Temporary output failed validation: {e}"))?;
        if dimensions != (image.width(), image.height()) {
            return Err("Encoded output dimensions do not match the edited image.".into());
        }

        if destination.exists() {
            if !overwrite {
                return Err("The destination already exists; confirm overwrite first.".into());
            }
            fs::rename(destination, &backup_path).map_err(|e| {
                format!("Could not safely move the existing destination aside: {e}")
            })?;
            if let Err(replace_error) = fs::rename(&temp_path, destination) {
                return match fs::rename(&backup_path, destination) {
                    Ok(()) => Err(format!(
                        "Could not replace destination ({replace_error}); the original was restored."
                    )),
                    Err(restore_error) => Err(format!(
                        "Could not replace destination ({replace_error}) and could not restore the backup ({restore_error}). The original remains at: {}",
                        backup_path.display()
                    )),
                };
            }
            if let Err(e) = fs::remove_file(&backup_path) {
                eprintln!("Saved image, but could not remove backup {}: {e}", backup_path.display());
            }
        } else {
            fs::rename(&temp_path, destination)
                .map_err(|e| format!("Could not move validated output into place: {e}"))?;
        }
        Ok(())
    })();

    if temp_path.exists() {
        if let Err(e) = fs::remove_file(&temp_path) {
            eprintln!("Could not remove temporary file {}: {e}", temp_path.display());
        }
    }
    write_result
}

fn encode_image_to_writer<W: Write + std::io::Seek>(
    image: &RgbaImage,
    format: ImageFormat,
    quality: u8,
    writer: &mut W,
) -> Result<(), String> {
    if format == ImageFormat::Jpeg {
        // JPEG has no alpha channel. Composite transparency against white rather
        // than allowing arbitrary transparent RGB values to become dark fringes.
        let mut rgb = image::RgbImage::new(image.width(), image.height());
        for (x, y, pixel) in image.enumerate_pixels() {
            let alpha = pixel[3] as u16;
            let inv_alpha = 255 - alpha;
            rgb.put_pixel(
                x,
                y,
                image::Rgb([
                    ((pixel[0] as u16 * alpha + 255 * inv_alpha) / 255) as u8,
                    ((pixel[1] as u16 * alpha + 255 * inv_alpha) / 255) as u8,
                    ((pixel[2] as u16 * alpha + 255 * inv_alpha) / 255) as u8,
                ]),
            );
        }
        image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality.clamp(1, 100))
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| format!("Could not encode JPEG: {e}"))?;
    } else {
        DynamicImage::ImageRgba8(image.clone())
            .write_to(writer, format)
            .map_err(|e| format!("Could not encode image: {e}"))?;
    }
    Ok(())
}

fn is_supported_output_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Bmp
            | ImageFormat::Gif
            | ImageFormat::Jpeg
            | ImageFormat::Png
            | ImageFormat::Pnm
            | ImageFormat::Qoi
            | ImageFormat::Tga
            | ImageFormat::Tiff
            | ImageFormat::WebP
    )
}

fn unique_sibling_path(destination: &Path, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let filename = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    destination.with_file_name(format!(
        ".{filename}.jpeg-viewer-{}-{stamp}.{suffix}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::{EditSession, ImageAdjustments, save_image_safely, save_image_safely_with_quality};
    use image::{Rgba, RgbaImage};
    use std::fs;

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8 * 80, y as u8 * 100, 7, 255]))
    }

    #[test]
    fn rotate_undo_and_redo_restore_dimensions() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.rotate_right();
        assert_eq!(edit.image.dimensions(), (2, 3));
        assert!(edit.undo());
        assert_eq!(edit.image.dimensions(), (3, 2));
        assert!(edit.redo());
        assert_eq!(edit.image.dimensions(), (2, 3));
    }

    #[test]
    fn multiple_redo_steps_are_preserved() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.rotate_right();
        edit.flip_horizontal();
        assert!(edit.undo());
        assert!(edit.undo());
        assert_eq!(edit.image.dimensions(), (3, 2));
        assert!(edit.redo());
        assert_eq!(edit.image.dimensions(), (2, 3));
        assert!(edit.redo());
        assert_eq!(edit.image.dimensions(), (2, 3));
    }

    #[test]
    fn horizontal_flip_is_undoable_and_marks_session_dirty() {
        let original = sample();
        let mut edit = EditSession::new("sample.png".into(), original.clone());
        edit.flip_horizontal();
        assert!(edit.dirty);
        assert_ne!(*edit.image, original);
        assert!(edit.undo());
        assert_eq!(*edit.image, original);
        assert!(!edit.dirty);
    }

    #[test]
    fn resize_is_not_undoable_and_restore_returns_to_original_dimensions() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.resize(6, 4).unwrap();
        assert_eq!(edit.image.dimensions(), (6, 4));
        assert!(!edit.can_undo());
        assert!(!edit.can_redo());
        edit.restore_to_original_size();
        assert_eq!(edit.image.dimensions(), (3, 2));
        assert!(!edit.has_structural_edits());
    }

    #[test]
    fn restore_to_original_size_preserves_adjustments() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.record_current_state();
        edit.set_adjustments(ImageAdjustments { brightness: 20, ..Default::default() });
        edit.resize(6, 4).unwrap();
        edit.restore_to_original_size();
        assert_eq!(edit.image.dimensions(), (3, 2));
        assert_eq!(edit.adjustments.brightness, 20);
        assert!(edit.can_undo());
        assert!(edit.dirty);
        assert!(edit.undo());
        assert!(edit.adjustments.is_default());
        assert_eq!(edit.image.dimensions(), (3, 2));
    }

    #[test]
    fn crop_is_not_undoable_but_adjustment_history_survives() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.record_current_state();
        edit.set_adjustments(ImageAdjustments { brightness: 20, ..Default::default() });
        edit.crop(0, 0, 2, 2).unwrap();
        assert_eq!(edit.image.dimensions(), (2, 2));
        assert!(edit.has_structural_edits());
        assert!(edit.can_undo());
        assert!(edit.undo());
        assert_eq!(edit.image.dimensions(), (2, 2));
        assert!(edit.adjustments.is_default());
    }

    #[test]
    fn resize_rejects_zero_dimensions() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        assert!(edit.resize(0, 10).is_err());
        assert_eq!(edit.image.dimensions(), (3, 2));
    }

    #[test]
    fn crop_rejects_out_of_bounds_regions() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        assert!(edit.crop(2, 1, 2, 2).is_err());
        assert_eq!(edit.image.dimensions(), (3, 2));
    }

    #[test]
    fn adjustments_are_undoable_and_preserve_alpha() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        edit.record_current_state();
        edit.set_adjustments(ImageAdjustments {
            brightness: 20,
            hue: 30,
            saturation: 140,
            contrast: -10,
            ..Default::default()
        });
        assert!(edit.dirty);
        let preview = edit.materialized_image();
        assert_ne!(preview, sample());
        assert_eq!(preview.get_pixel(0, 0)[3], 255);
        assert!(edit.undo());
        assert!(edit.adjustments.is_default());
        assert!(!edit.dirty);
    }

    #[test]
    fn save_refuses_existing_destination_without_confirmation() {
        let dir = std::env::temp_dir().join(format!(
            "jpeg-viewer-edit-no-overwrite-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.png");
        fs::write(&path, b"keep this file").unwrap();
        assert!(save_image_safely(&sample(), &path, false).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"keep this file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_as_writes_a_decodable_image() {
        let dir =
            std::env::temp_dir().join(format!("jpeg-viewer-edit-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edited.png");
        let _ = fs::remove_file(&path);
        save_image_safely(&sample(), &path, false).unwrap();
        let decoded = image::open(&path).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn jpeg_quality_writer_outputs_valid_jpeg() {
        let dir =
            std::env::temp_dir().join(format!("jpeg-viewer-edit-jpeg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edited.jpg");
        let _ = fs::remove_file(&path);
        save_image_safely_with_quality(&sample(), &path, false, 95).unwrap();
        assert_eq!(
            image::ImageFormat::from_path(&path).unwrap(),
            image::ImageFormat::Jpeg
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
