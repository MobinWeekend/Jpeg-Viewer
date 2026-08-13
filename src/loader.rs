use crate::helpers::is_supported_image;
use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};
use image::{DynamicImage, ImageDecoder, ImageReader};
use sevenz_rust2::{ArchiveReader, Password};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use unrar::Archive as RarArchive;
use zip::ZipArchive;

// ========== Image Loading ==========

/// Load image from bytes with automatic format detection and EXIF orientation correction
pub fn load_image_from_bytes(
    bytes: &[u8],
    _path_hint: Option<&Path>,
) -> Result<DynamicImage, String> {
    // Use ImageReader to detect format and get decoder with orientation metadata
    let cursor = Cursor::new(bytes);
    let reader = ImageReader::new(cursor)
        .with_guessed_format()
        .map_err(|e| format!("Failed to guess image format: {}", e))?;

    // Get the decoder to access orientation metadata
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Failed to create decoder: {}", e))?;

    // Read orientation from EXIF metadata (if present and readable)
    // orientation() returns Result<Orientation, ImageError>
    let orientation = decoder.orientation().ok();

    // Decode the image
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    // Apply EXIF orientation if present (rotations/flips)
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    Ok(image)
}

/// Load full resolution image from a file path
pub fn load_full_resolution(path: PathBuf) -> Result<DynamicImage, String> {
    let bytes = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    load_image_from_bytes(&bytes, Some(&path))
}

// ========== Archive Loading ==========

/// Load image from ZIP archive
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
    load_image_from_bytes(&bytes, Some(Path::new(&image.name)))
}

/// Load image from 7z archive
pub fn load_7z_image(image: S7ArchiveImage) -> Result<DynamicImage, String> {
    let mut reader = ArchiveReader::open(&image.archive_path, Password::empty())
        .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
    let bytes = reader
        .read_file(&image.name)
        .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
    load_image_from_bytes(&bytes, Some(Path::new(&image.name)))
}

/// Load image from RAR archive
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
            return load_image_from_bytes(&bytes, Some(Path::new(&image.name)));
        }
        archive = header
            .skip()
            .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
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
