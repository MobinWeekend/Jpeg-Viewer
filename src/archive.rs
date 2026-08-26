use crate::helpers::is_supported_image;
use crate::image_entry::{ArchiveImage, ImageEntry, RarArchiveImage, S7ArchiveImage};
use sevenz_rust2::Archive as SevenZipArchive;
use std::fs::File;
use std::path::Path;
use unrar::Archive as RarArchive;
use zip::ZipArchive;

pub fn scan_zip(path: &Path) -> Vec<ImageEntry> {
    let mut images = Vec::new();

    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Failed to open ZIP: {}", err);
            return images;
        }
    };

    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(err) => {
            eprintln!("Failed to read ZIP: {}", err);
            return images;
        }
    };

    for i in 0..archive.len() {
        let entry = match archive.by_index(i) {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();
        if is_supported_image(Path::new(&name)) {
            images.push(ImageEntry::Zip(ArchiveImage {
                archive_path: path.to_path_buf(),
                entry_index: i,
                name,
            }));
        }
    }

    println!("Found {} image(s) in ZIP.", images.len());

    images
}

pub fn scan_7z(path: &Path) -> Vec<ImageEntry> {
    let mut images = Vec::new();

    let archive = match SevenZipArchive::open(path) {
        Ok(archive) => archive,
        Err(err) => {
            eprintln!("Failed to open 7z archive: {}", err);
            return images;
        }
    };

    for entry in &archive.files {
        if entry.is_directory() {
            continue;
        }

        let name = entry.name().to_string();

        if is_supported_image(Path::new(&name)) {
            images.push(ImageEntry::S7z(S7ArchiveImage {
                archive_path: path.to_path_buf(),
                name,
            }));
        }
    }

    println!("Found {} image(s) in 7z.", images.len());

    images
}

pub fn scan_rar(path: &Path) -> Vec<ImageEntry> {
    let mut images = Vec::new();

    let mut archive = match RarArchive::new(path).open_for_listing() {
        Ok(archive) => archive,
        Err(err) => {
            eprintln!("Failed to open RAR: {}", err);
            return images;
        }
    };

    while let Some(header) = archive.read_header().unwrap_or(None) {
        let entry = header.entry();

        if !entry.is_directory() {
            let name = entry.filename.to_string_lossy().to_string();

            if is_supported_image(Path::new(&name)) {
                images.push(ImageEntry::Rar(RarArchiveImage {
                    archive_path: path.to_path_buf(),
                    name,
                }));
            }
        }

        archive = match header.skip() {
            Ok(next) => next,
            Err(err) => {
                eprintln!("Failed reading RAR entry: {}", err);
                break;
            }
        };
    }

    println!("Found {} image(s) in RAR.", images.len());

    images
}
