use crate::image_entry::ArchiveImage;
use crate::helpers::is_supported_image;
use image::DynamicImage;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub fn load(path: PathBuf) -> Option<DynamicImage> {
    image::open(path).ok()
}

pub fn load_zip_image(image: ArchiveImage) -> Option<image::DynamicImage> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;

    let mut entry = archive.by_index(image.entry_index).ok()?;

    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;

    image::load_from_memory(&bytes).ok()
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
    files.sort();
    files
}
