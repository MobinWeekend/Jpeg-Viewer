use crate::constants::IMAGE_EXT;
use std::path::Path;

pub fn get_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str()) //Convert to UTF-8 Option<&OsStr> becomes Option<&Str>
        // support check
        .map(|ext| {
            IMAGE_EXT
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
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
