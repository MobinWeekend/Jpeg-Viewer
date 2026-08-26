use std::time::Duration;

pub const ARCHIVE_EXT: &[&str] = &["zip", "7z", "rar"];

pub const OVERLAY_HIDE_DELAY: Duration = Duration::from_millis(1400);

/// Maximum tile size that this application will generate.
pub const MAX_TILE_SIZE: u32 = 16384;
