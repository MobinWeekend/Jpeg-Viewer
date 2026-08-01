use crate::gif_animation::GifAnimation;
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use crate::helpers::is_supported_image;
use image::{DynamicImage, ImageReader, ImageDecoder};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use sevenz_rust2::{ArchiveReader, Password};
use unrar::Archive as RarArchive;

// ========== Image Loading ==========

// Load full resolution image
pub fn load_full_resolution(path: PathBuf) -> Option<DynamicImage> {
    // Open image reader and decode
    let mut reader = match ImageReader::open(&path) {
        Ok(reader) => match reader.into_decoder() {
            Ok(decoder) => decoder,
            Err(_) => return None,
        },
        Err(_) => return None,
    };
    
    // Get orientation - you need to handle the Result properly
    let orientation = match reader.orientation() {
        Ok(orient) => orient,
        Err(_) => return None, // Or use Orientation::Normal as default
    };
    
    // Decode image
    let mut img = match DynamicImage::from_decoder(reader) {
        Ok(img) => img,
        Err(_) => return None,
    };
    
    // Apply orientation - this is a method on DynamicImage
    img.apply_orientation(orientation);
    
    Some(img)
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

pub fn load_zip_image(image: ArchiveImage) -> Option<DynamicImage> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    image::load_from_memory(&bytes).ok()
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

pub fn load_7z_image(image: S7ArchiveImage) -> Option<DynamicImage> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    image::load_from_memory(&bytes).ok()
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

pub fn load_rar_image(image: RarArchiveImage) -> Option<DynamicImage> {
    let archive = RarArchive::new(&image.archive_path)
        .open_for_processing()
        .ok()?;
    let mut archive = archive;
    loop {
        let header = archive.read_header().ok()??;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header.read().ok()?;
            return image::load_from_memory(&bytes).ok();
        }
        archive = header.skip().ok()?;
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