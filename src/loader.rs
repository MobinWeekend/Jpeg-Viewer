use crate::gif_animation::GifAnimation;
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use crate::helpers::is_supported_image;
use image::DynamicImage;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use sevenz_rust2::{ArchiveReader, Password};
use unrar::Archive as RarArchive;
use natord::compare;

pub fn load(path: PathBuf) -> Option<DynamicImage> {
    if let Some(ext) = path.extension() {
        if ext.eq_ignore_ascii_case("gif") {
            return None;
        }
    }
    
    image::open(path).ok()
}

// Load full GIF
pub fn load_gif(path: PathBuf) -> Option<GifAnimation> {
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(gif) = GifAnimation::from_bytes(&data) {
            return Some(gif);
        }
    }
    None
}

// Load GIF preview (first frame only)
pub fn load_gif_preview(path: PathBuf) -> Option<GifAnimation> {
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(gif) = GifAnimation::from_bytes_preview(&data) {
            return Some(gif);
        }
    }
    None
}

// Full GIF from ZIP
pub fn load_zip_gif(image: ArchiveImage) -> Option<GifAnimation> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    GifAnimation::from_bytes(&bytes).ok()
}

// GIF preview from ZIP
pub fn load_zip_gif_preview(image: ArchiveImage) -> Option<GifAnimation> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    GifAnimation::from_bytes_preview(&bytes).ok()
}

// Full GIF from 7z
pub fn load_7z_gif(image: S7ArchiveImage) -> Option<GifAnimation> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    GifAnimation::from_bytes(&bytes).ok()
}

// GIF preview from 7z
pub fn load_7z_gif_preview(image: S7ArchiveImage) -> Option<GifAnimation> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    GifAnimation::from_bytes_preview(&bytes).ok()
}

// Full GIF from RAR
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

// GIF preview from RAR
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

pub fn load_zip_image(image: ArchiveImage) -> Option<DynamicImage> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_index(image.entry_index).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    image::load_from_memory(&bytes).ok()
}

pub fn load_7z_image(image: S7ArchiveImage) -> Option<DynamicImage> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty()).ok()?;
    let bytes = reader.read_file(&image.name).ok()?;
    image::load_from_memory(&bytes).ok()
}

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
        compare(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });

    files
}