use crate::gif_animation::GifAnimation;
use crate::helpers::is_supported_image;
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use image::{DynamicImage, ImageFormat, ImageReader};
use sevenz_rust2::{ArchiveReader, Password};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use unrar::Archive as RarArchive;
use zip::ZipArchive;

// Constants for virtual texturing
//pub const MAX_GPU_TEXTURE_SIZE: u32 = 16384;
//pub const LARGE_IMAGE_THRESHOLD: u64 = 50_000_000;

// ========== Helper: map extension to ImageFormat (EXCLUDING EXR) ==========
fn ext_to_format(ext: &str) -> Option<ImageFormat> {
    match ext.to_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "bmp" => Some(ImageFormat::Bmp),
        "tiff" => Some(ImageFormat::Tiff),
        "tga" => Some(ImageFormat::Tga),
        "ico" => Some(ImageFormat::Ico),
        "avif" => Some(ImageFormat::Avif),
        "hdr" => Some(ImageFormat::Hdr),
        "pnm" => Some(ImageFormat::Pnm),
        "qoi" => Some(ImageFormat::Qoi),
        "dds" => Some(ImageFormat::Dds),
        _ => None,
    }
}

/// Load image from bytes using built‑in detection first, then fallback to infer.
/// The `path_hint` is only used as a last resort fallback for the extension.
pub fn load_image_from_bytes(bytes: &[u8], path_hint: Option<&Path>) -> Result<DynamicImage, String> {
    // 1. Try the image crate's own detection (works for most formats)
    if let Ok(img) = image::load_from_memory(bytes) {
        return Ok(img);
    }

    // 2. Fallback: use infer to detect format and decode with explicit format
    //    Note: infer::get is a function that takes a &[u8] and returns Option<Type>
    if let Some(kind) = infer::get(bytes) {
        if let Some(format) = ext_to_format(kind.extension()) {
            // ImageReader::with_format is an associated function, not a method
            let reader = ImageReader::with_format(Cursor::new(bytes), format);
            if let Ok(img) = reader.decode() {
                return Ok(img);
            }
        }
    }

    // 3. Last resort: try using the extension from the path hint (if any)
    if let Some(hint) = path_hint {
        if let Some(ext) = hint.extension().and_then(|e| e.to_str()) {
            if let Some(format) = ext_to_format(ext) {
                let reader = ImageReader::with_format(Cursor::new(bytes), format);
                if let Ok(img) = reader.decode() {
                    return Ok(img);
                }
            }
        }
    }

    Err("The image format could not be determined".to_string())
}

// ========== Image Loading ==========

pub fn load_full_resolution(path: PathBuf) -> Result<DynamicImage, String> {
    let bytes = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    load_image_from_bytes(&bytes, Some(&path))
}

pub fn load_gif_preview(path: PathBuf) -> Option<GifAnimation> {
    fs::read(&path)
        .ok()
        .and_then(|data| GifAnimation::from_bytes_preview(&data).ok())
}

// ========== Archive Loading ==========

pub fn load_zip_image(image: ArchiveImage) -> Result<DynamicImage, String> {
    let file =
        File::open(&image.archive_path).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {}", e))?;
    let mut entry = archive
        .by_index(image.entry_index)
        .map_err(|e| format!("Failed to read entry: {}", e))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read data: {}", e))?;
    let path_hint = Path::new(&image.name);
    load_image_from_bytes(&bytes, Some(path_hint))
}

pub fn load_zip_gif_preview(image: ArchiveImage) -> Option<GifAnimation> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    GifAnimation::from_bytes_preview(&bytes).ok()
}

// ========== 7z Archive Loading ==========

pub fn load_7z_image(image: S7ArchiveImage) -> Result<DynamicImage, String> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty())
        .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
    let bytes = reader
        .read_file(&image.name)
        .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
    let path_hint = Path::new(&image.name);
    load_image_from_bytes(&bytes, Some(path_hint))
}

pub fn load_7z_gif_preview(image: S7ArchiveImage) -> Option<GifAnimation> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    GifAnimation::from_bytes_preview(&bytes).ok()
}

// ========== RAR Archive Loading ==========

pub fn load_rar_image(image: RarArchiveImage) -> Result<DynamicImage, String> {
    let archive = RarArchive::new(&image.archive_path)
        .open_for_processing()
        .map_err(|e| format!("Failed to open RAR archive: {}", e))?;
    let mut archive = archive;
    loop {
        let header = archive
            .read_header()
            .map_err(|e| format!("Failed to read RAR header: {}", e))?
            .ok_or_else(|| format!("File not found in archive: {}", image.name))?;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header
                .read()
                .map_err(|e| format!("Failed to read file from RAR: {}", e))?;
            let path_hint = Path::new(&image.name);
            return load_image_from_bytes(&bytes, Some(path_hint));
        }
        archive = header
            .skip()
            .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
    }
}

pub fn load_rar_gif_preview(image: RarArchiveImage) -> Option<GifAnimation> {
    let archive = RarArchive::new(&image.archive_path)
        .open_for_processing()
        .ok()?;
    let mut archive = archive;
    loop {
        let header = archive.read_header().ok()??;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header.read().ok()?;
            return GifAnimation::from_bytes_preview(&bytes).ok();
        }
        archive = header.skip().ok()?;
    }
}

// ========== Directory Loading ==========

pub fn load_directory_images(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| {
            entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.is_file() && is_supported_image(&path) {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    files.sort_by(|a, b| {
        let a_name = a.file_name().unwrap_or_default().to_string_lossy();
        let b_name = b.file_name().unwrap_or_default().to_string_lossy();
        natord::compare(&a_name.to_lowercase(), &b_name.to_lowercase())
    });

    files
}
