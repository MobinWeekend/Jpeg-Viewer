use crate::image_core::ImageFormat;
use std::path::Path;

pub fn get_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(ImageFormat::from_extension)
        .is_some()
}

/*
// this will be used one day...
pub fn is_supported_archive(path: &Path) -> bool {
     path.extension()
        .and_then(|ext| ext.to_str()) //Convert to UTF-8 Option<&OsStr> becomes Option<&Str>
        // support check
        .map(|ext| {
            ARCHIVE_EXT
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
}
*/
