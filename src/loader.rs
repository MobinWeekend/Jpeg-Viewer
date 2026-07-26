use image::DynamicImage;
use std::path::PathBuf;

pub fn load(path: PathBuf) -> Option<DynamicImage> {
    image::open(path).ok()
}