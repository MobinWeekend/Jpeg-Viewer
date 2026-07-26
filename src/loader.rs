use image::DynamicImage;
use std::path::PathBuf;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;
use crate::app::ZipImage;

pub fn load(path: PathBuf) -> Option<DynamicImage> {
    image::open(path).ok()
}


pub fn load_zip_image(image: ZipImage) -> Option<image::DynamicImage> {
    let file = File::open(&image.archive_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;

    let mut entry = archive.by_index(image.entry_index).ok()?;

    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    
    image::load_from_memory(&bytes).ok()
}