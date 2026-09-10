use crate::decoder::decode_bytes;
use crate::helpers::is_supported_image;
use crate::image_core::{DecodedImage, ImageError};
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use crate::sort::{SortMethod, sort_images};
use sevenz_rust2::{ArchiveReader, Password};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use unrar::Archive as RarArchive;
use zip::ZipArchive;

// ========== Archive Loading ==========
/// Load image from ZIP archive
pub fn load_zip_image(image: ArchiveImage) -> Result<DecodedImage, ImageError> {
    let file = File::open(&image.archive_path)
        .map_err(|e| ImageError::Io(format!("Failed to open archive: {}", e)))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| ImageError::Io(format!("Failed to read archive: {}", e)))?;
    let mut entry = archive
        .by_index(image.entry_index)
        .map_err(|e| ImageError::Io(format!("Failed to read entry: {}", e)))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| ImageError::Io(format!("Failed to read data: {}", e)))?;
    decode_bytes(&bytes)
}

/// Load image from 7z archive
pub fn load_7z_image(image: S7ArchiveImage) -> Result<DecodedImage, ImageError> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty())
        .map_err(|e| ImageError::Io(format!("Failed to open 7z archive: {}", e)))?;
    let bytes = reader
        .read_file(&image.name)
        .map_err(|e| ImageError::Io(format!("Failed to read file from 7z: {}", e)))?;
    decode_bytes(&bytes)
}

/// Load image from RAR archive
pub fn load_rar_image(image: RarArchiveImage) -> Result<DecodedImage, ImageError> {
    let archive = RarArchive::new(&image.archive_path)
        .open_for_processing()
        .map_err(|e| ImageError::Io(format!("Failed to open RAR archive: {}", e)))?;
    let mut archive = archive;
    loop {
        let header = archive
            .read_header()
            .map_err(|e| ImageError::Io(format!("Failed to read RAR header: {}", e)))?
            .ok_or_else(|| ImageError::Io(format!("File not found in archive: {}", image.name)))?;
        let filename = header.entry().filename.to_string_lossy().to_string();
        if filename == image.name {
            let (bytes, _) = header
                .read()
                .map_err(|e| ImageError::Io(format!("Failed to read file from RAR: {}", e)))?;
            return decode_bytes(&bytes);
        }
        archive = header
            .skip()
            .map_err(|e| ImageError::Io(format!("Failed to skip RAR entry: {}", e)))?;
    }
}

// ========== Directory Loading ==========

/// Load all supported images from a directory with optional sorting
pub fn load_directory_images(path: &Path, sort_method: SortMethod) -> Vec<PathBuf> {
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

    sort_images(&mut files, sort_method);
    files
}
