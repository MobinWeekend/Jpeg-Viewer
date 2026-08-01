use std::path::Path;

pub const IMAGE_EXT: &[&str] = &[
    "avif", // default feature: avif
    "bmp",  // default feature: bmp
    "dds",  // default feature: dds
    "exr",  // default feature: exr
    "gif",  // default feature: gif
    "hdr",  // default feature: hdr
    "ico",  // default feature: ico
    "jpg",  // default feature: jpeg (the flag is "jpeg")
    "jpeg", // default feature: jpeg
    "png",  // default feature: png
    "pnm",  // default feature: pnm
    "qoi",  // default feature: qoi
    "tga",  // default feature: tga
    "tiff", // default feature: tiff
    "webp", // default feature: webp
];

pub const ARCHIVE_EXT: &[&str] = &[
    "zip",
    "7z",
    "rar",
];

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