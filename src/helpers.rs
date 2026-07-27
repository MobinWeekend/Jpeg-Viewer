use std::path::Path;

const IMAGE_EXT: &[&str] = &[
    "jpg",
    "jpeg",
    "png",
    "gif",
    "bmp",
    "webp",
];

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