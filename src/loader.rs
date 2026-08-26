use crate::decoder::decode_bytes;
use crate::helpers::is_supported_image;
use crate::image_core::{DecodedImage, ImageError};
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
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

/// Load all supported images from a directory, sorted naturally
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

    #[cfg(target_os = "windows")]
    fn compare_filenames(a: &Path, b: &Path) -> std::cmp::Ordering {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::StrCmpLogicalW;
        use windows::core::PCWSTR;

        let a_name = a.file_name().unwrap_or_default();
        let b_name = b.file_name().unwrap_or_default();

        let a_wide: Vec<u16> = a_name.encode_wide().chain(std::iter::once(0)).collect();

        let b_wide: Vec<u16> = b_name.encode_wide().chain(std::iter::once(0)).collect();

        let result = unsafe { StrCmpLogicalW(PCWSTR(a_wide.as_ptr()), PCWSTR(b_wide.as_ptr())) };

        result.cmp(&0)
    }

    #[cfg(not(target_os = "windows"))]
    fn compare_filenames(a: &Path, b: &Path) -> std::cmp::Ordering {
        let a_name = a.file_name().unwrap_or_default().to_string_lossy();
        let b_name = b.file_name().unwrap_or_default().to_string_lossy();

        natord::compare(&a_name.to_lowercase(), &b_name.to_lowercase())
    }

    //Apply the sorting
    files.sort_by(|a, b| compare_filenames(a.as_path(), b.as_path()));
    files
}
