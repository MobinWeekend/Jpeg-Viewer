use crate::image_entry::{ArchiveImage, ImageEntry};
use crate::helpers::is_supported_image;
use std::fs::File;
use std::path::Path;
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

//pub fn scan_7z(path: &Path) -> Vec<ImageEntry>

//pub fn scan_rar(path: &Path) -> Vec<ImageEntry>
