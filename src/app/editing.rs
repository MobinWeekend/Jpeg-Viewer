//! Working-copy image editing and validated, recoverable file output.
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_HISTORY_STEPS: usize = 10;
const MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_EDIT_PIXELS: u64 = 25_000_000;

pub struct EditSession {
    pub source_path: PathBuf,
    pub original: RgbaImage,
    pub image: RgbaImage,
    undo: Vec<RgbaImage>,
    redo: Vec<RgbaImage>,
    pub dirty: bool,
}

impl EditSession {
    pub fn new(source_path: PathBuf, image: RgbaImage) -> Self {
        Self {
            source_path,
            original: image.clone(),
            image,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
        }
    }

    fn record_before_change(&mut self) {
        self.undo.push(self.image.clone());
        self.redo.clear();
        while self.undo.len() > MAX_HISTORY_STEPS
            || self.undo.iter().map(|image| image.as_raw().len()).sum::<usize>()
                > MAX_HISTORY_BYTES
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
    }

    fn replace_image(&mut self, image: RgbaImage) {
        self.record_before_change();
        self.image = image;
        self.dirty = self.image != self.original;
    }

    pub fn rotate_left(&mut self) {
        self.replace_image(image::imageops::rotate270(&self.image));
    }

    pub fn rotate_right(&mut self) {
        self.replace_image(image::imageops::rotate90(&self.image));
    }

    pub fn flip_horizontal(&mut self) {
        self.replace_image(image::imageops::flip_horizontal(&self.image));
    }

    pub fn flip_vertical(&mut self) {
        self.replace_image(image::imageops::flip_vertical(&self.image));
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
        let cropped = image::imageops::crop_imm(&self.image, x, y, width, height).to_image();
        self.replace_image(cropped);
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.image.clone());
        self.image = previous;
        self.dirty = self.image != self.original;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.image.clone());
        if self.undo.len() > MAX_HISTORY_STEPS {
            self.undo.remove(0);
        }
        self.image = next;
        self.dirty = self.image != self.original;
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
            self.replace_image(self.original.clone());
        }
    }
}

/// Encode and validate the output before touching the destination. The temporary
/// file is created in the destination directory so the final rename stays on the
/// same filesystem. Existing files are moved aside and restored if replacement fails.
pub fn save_image_safely(
    image: &RgbaImage,
    destination: &Path,
    overwrite: bool,
) -> Result<(), String> {
    let format = ImageFormat::from_path(destination)
        .map_err(|_| "The destination needs a supported image extension.".to_string())?;
    if !is_supported_output_format(format) {
        return Err(format!("Saving as {:?} is not supported by this editor.", format));
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

    // Encode in memory first. This ensures unsupported encoders and encoding errors
    // happen before the destination is touched.
    let mut encoded = Vec::new();
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut std::io::Cursor::new(&mut encoded), format)
        .map_err(|e| format!("Could not encode image: {e}"))?;

    let validated = image::load_from_memory_with_format(&encoded, format)
        .map_err(|e| format!("Encoded output failed validation: {e}"))?;
    if (validated.width(), validated.height()) != (image.width(), image.height()) {
        return Err("Encoded output dimensions do not match the edited image.".into());
    }

    let temp_path = unique_sibling_path(destination, "tmp");
    let backup_path = unique_sibling_path(destination, "backup");
    let mut temp_file = File::options()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("Could not create temporary output: {e}"))?;

    let write_result = (|| -> Result<(), String> {
        temp_file
            .write_all(&encoded)
            .map_err(|e| format!("Could not write temporary output: {e}"))?;
        temp_file
            .sync_all()
            .map_err(|e| format!("Could not flush temporary output: {e}"))?;
        drop(temp_file);

        let temp_bytes =
            fs::read(&temp_path).map_err(|e| format!("Could not read back temporary output: {e}"))?;
        let decoded = image::load_from_memory_with_format(&temp_bytes, format)
            .map_err(|e| format!("Temporary output failed validation: {e}"))?;
        if (decoded.width(), decoded.height()) != (image.width(), image.height()) {
            return Err("Temporary output dimensions failed validation.".into());
        }

        if destination.exists() {
            if !overwrite {
                return Err("The destination already exists; confirm overwrite first.".into());
            }
            fs::rename(destination, &backup_path)
                .map_err(|e| format!("Could not safely move the existing destination aside: {e}"))?;
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
            // A leftover backup is safer than reporting a failed save after the
            // replacement succeeded; cleanup failure is logged for recovery.
            if let Err(e) = fs::remove_file(&backup_path) {
                eprintln!(
                    "Saved image, but could not remove backup {}: {e}",
                    backup_path.display()
                );
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
    use super::{EditSession, save_image_safely};
    use image::{Rgba, RgbaImage};
    use std::fs;

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8, y as u8, 7, 255]))
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
    fn horizontal_flip_is_undoable_and_marks_session_dirty() {
        let original = sample();
        let mut edit = EditSession::new("sample.png".into(), original.clone());
        edit.flip_horizontal();
        assert!(edit.dirty);
        assert_ne!(edit.image, original);
        assert!(edit.undo());
        assert_eq!(edit.image, original);
        assert!(!edit.dirty);
    }

    #[test]
    fn crop_rejects_out_of_bounds_regions() {
        let mut edit = EditSession::new("sample.png".into(), sample());
        assert!(edit.crop(2, 1, 2, 2).is_err());
        assert_eq!(edit.image.dimensions(), (3, 2));
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
        let dir = std::env::temp_dir().join(format!(
            "jpeg-viewer-edit-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edited.png");
        let _ = fs::remove_file(&path);
        save_image_safely(&sample(), &path, false).unwrap();
        let decoded = image::open(&path).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        fs::remove_dir_all(dir).unwrap();
    }
}
