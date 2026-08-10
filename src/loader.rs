use crate::gif_animation::GifAnimation;
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use crate::helpers::is_supported_image;
use image::{DynamicImage, ImageReader, ImageDecoder, GenericImageView, metadata::Orientation};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use sevenz_rust2::{ArchiveReader, Password};
use unrar::Archive as RarArchive;

// Constants matching those in virtual_texture.rs
pub const MAX_GPU_TEXTURE_SIZE: u32 = 16384;
pub const LARGE_IMAGE_THRESHOLD: u64 = 50_000_000;

// ========== Image Loading ==========

// Load full resolution image - returns Result with error message
pub fn load_full_resolution(path: PathBuf) -> Result<DynamicImage, String> {
    let mut reader = match ImageReader::open(&path) {
        Ok(reader) => match reader.into_decoder() {
            Ok(decoder) => decoder,
            Err(e) => return Err(format!("Failed to decode image: {}", e)),
        },
        Err(e) => return Err(format!("Failed to open image: {}", e)),
    };
    
    let orientation = match reader.orientation() {
        Ok(orient) => orient,
        Err(_) => Orientation::NoTransforms,
    };
    
    let mut img = match DynamicImage::from_decoder(reader) {
        Ok(img) => img,
        Err(e) => return Err(format!("Failed to load image: {}", e)),
    };
    
    img.apply_orientation(orientation);
    
    // Check for extreme dimensions - but ONLY if they would be too large for virtual texture
    let (width, height) = img.dimensions();
    let pixel_count = width as u64 * height as u64;
    
    // If the image is large, we'll use virtual texturing - no need to check size here
    let use_virtual = pixel_count > LARGE_IMAGE_THRESHOLD
        || width > MAX_GPU_TEXTURE_SIZE
        || height > MAX_GPU_TEXTURE_SIZE;
    
    if !use_virtual {
        // Only check size for images that won't use virtual texturing
        const MAX_TEXTURE_SIZE: u32 = 32768;
        if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
            return Err(format!(
                "Image too large: {}x{}\nMaximum supported size: {}x{}",
                width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
            ));
        }
    }
    
    Ok(img)
}

// ========== GIF Loading ==========

// Load full GIF (all frames)
pub fn load_gif(path: PathBuf) -> Option<GifAnimation> {
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(gif) = GifAnimation::from_bytes(&data) {
            return Some(gif);
        }
    }
    None
}

// Load only first frame of GIF for preview (fast loading)
pub fn load_gif_preview(path: PathBuf) -> Option<GifAnimation> {
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(gif) = GifAnimation::from_bytes_preview(&data) {
            return Some(gif);
        }
    }
    None
}

// ========== Archive Loading ==========

pub fn load_zip_image(image: ArchiveImage) -> Result<DynamicImage, String> {
    let file = File::open(&image.archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("Failed to read archive: {}", e))?;
    let mut entry = archive.by_index(image.entry_index)
        .map_err(|e| format!("Failed to read entry: {}", e))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read data: {}", e))?;
    
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    
    // Check size - but only if not using virtual texture
    let (width, height) = img.dimensions();
    let pixel_count = width as u64 * height as u64;
    let use_virtual = pixel_count > LARGE_IMAGE_THRESHOLD
        || width > MAX_GPU_TEXTURE_SIZE
        || height > MAX_GPU_TEXTURE_SIZE;
    
    if !use_virtual {
        const MAX_TEXTURE_SIZE: u32 = 32768;
        if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
            return Err(format!(
                "Image too large: {}x{}\nMaximum supported size: {}x{}",
                width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
            ));
        }
    }
    
    Ok(img)
}

pub fn load_zip_gif(image: ArchiveImage) -> Option<GifAnimation> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    GifAnimation::from_bytes(&bytes).ok()
}

// Load only first frame of GIF from ZIP for preview
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
    let bytes = reader.read_file(&image.name)
        .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
    
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    
    // Check size - but only if not using virtual texture
    let (width, height) = img.dimensions();
    let pixel_count = width as u64 * height as u64;
    let use_virtual = pixel_count > LARGE_IMAGE_THRESHOLD
        || width > MAX_GPU_TEXTURE_SIZE
        || height > MAX_GPU_TEXTURE_SIZE;
    
    if !use_virtual {
        const MAX_TEXTURE_SIZE: u32 = 32768;
        if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
            return Err(format!(
                "Image too large: {}x{}\nMaximum supported size: {}x{}",
                width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
            ));
        }
    }
    
    Ok(img)
}

pub fn load_7z_gif(image: S7ArchiveImage) -> Option<GifAnimation> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    GifAnimation::from_bytes(&bytes).ok()
}

// Load only first frame of GIF from 7z for preview
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
        let header = archive.read_header()
            .map_err(|e| format!("Failed to read RAR header: {}", e))?
            .ok_or_else(|| format!("File not found in archive: {}", image.name))?;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header.read()
                .map_err(|e| format!("Failed to read file from RAR: {}", e))?;
            
            let img = image::load_from_memory(&bytes)
                .map_err(|e| format!("Failed to decode image: {}", e))?;
            
            // Check size - but only if not using virtual texture
            let (width, height) = img.dimensions();
            let pixel_count = width as u64 * height as u64;
            let use_virtual = pixel_count > LARGE_IMAGE_THRESHOLD
                || width > MAX_GPU_TEXTURE_SIZE
                || height > MAX_GPU_TEXTURE_SIZE;
            
            if !use_virtual {
                const MAX_TEXTURE_SIZE: u32 = 32768;
                if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
                    return Err(format!(
                        "Image too large: {}x{}\nMaximum supported size: {}x{}",
                        width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
                    ));
                }
            }
            
            return Ok(img);
        }
        archive = header.skip()
            .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
    }
}

pub fn load_rar_gif(image: RarArchiveImage) -> Option<GifAnimation> {
    let archive = RarArchive::new(&image.archive_path)
        .open_for_processing()
        .ok()?;
    let mut archive = archive;
    loop {
        let header = archive.read_header().ok()??;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header.read().ok()?;
            return GifAnimation::from_bytes(&bytes).ok();
        }
        archive = header.skip().ok()?;
    }
}

// Load only first frame of GIF from RAR for preview
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
                        return Some(path);
                    }
                    None
                })
                .collect::<Vec<_>>()
        })
        .collect();

    files.sort_by(|a, b| {
        let a_name = a.file_name().unwrap_or_default().to_string_lossy();
        let b_name = b.file_name().unwrap_or_default().to_string_lossy();
        // Case-insensitive natural order sorting
        natord::compare(&a_name.to_lowercase(), &b_name.to_lowercase())
    });

    files
}