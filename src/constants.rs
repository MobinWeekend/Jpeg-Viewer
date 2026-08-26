use std::time::Duration;

pub const IMAGE_EXT: &[&str] = &[
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
    "avif", // zenavif
    "heic", // heic
];

pub const ARCHIVE_EXT: &[&str] = &["zip", "7z", "rar"];

pub const OVERLAY_HIDE_DELAY: Duration = Duration::from_millis(1400);

/// Maximum tile size that this application will generate.
pub const MAX_TILE_SIZE: u32 = 16384;
