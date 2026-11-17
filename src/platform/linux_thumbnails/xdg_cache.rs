//! Shared Freedesktop thumbnail cache access.
//!
//! Existing desktop cache entries are validated before reuse. When JPEG Viewer
//! itself runs an installed standard thumbnailer, it writes the resulting PNG
//! back with the required URI/mtime metadata and an atomic rename.

use std::path::{Path, PathBuf};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::time::UNIX_EPOCH;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CACHE_TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn file_uri(path: &Path) -> Option<String> {
    let absolute = path.canonicalize().ok()?;
    url::Url::from_file_path(absolute).ok().map(|uri| uri.to_string())
}

fn cache_root() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CACHE_HOME") {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Some(path.join("thumbnails"));
        }
    }

    dirs::home_dir().map(|home| home.join(".cache").join("thumbnails"))
}

pub(super) fn source_stamp(path: &Path) -> Option<(u64, u64)> {
    let metadata = path.metadata().ok()?;
    let mtime = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((mtime, metadata.len()))
}

/// Check the PNG tEXt fields required by the Freedesktop thumbnail standard.
/// We reject malformed, mismatched, and stale cache files before decoding them.
fn valid_thumbnail_metadata(bytes: &[u8], uri: &str, mtime: u64, source_size: u64) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_SIGNATURE) {
        return false;
    }

    let mut offset = 8usize;
    let mut found_uri: Option<&str> = None;
    let mut found_mtime: Option<&str> = None;
    let mut found_size: Option<&str> = None;

    while offset.saturating_add(12) <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let Some(chunk_end) = offset.checked_add(12).and_then(|end| end.checked_add(length)) else {
            return false;
        };
        if chunk_end > bytes.len() {
            return false;
        }

        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + length];
        if kind == b"tEXt" {
            if let Some(separator) = data.iter().position(|byte| *byte == 0) {
                let key = std::str::from_utf8(&data[..separator]).ok();
                let value = std::str::from_utf8(&data[separator + 1..]).ok();
                match (key, value) {
                    (Some("Thumb::URI"), Some(value)) => found_uri = Some(value),
                    (Some("Thumb::MTime"), Some(value)) => found_mtime = Some(value),
                    (Some("Thumb::Size"), Some(value)) => found_size = Some(value),
                    _ => {}
                }
            }
        }

        offset = chunk_end;
        if kind == b"IEND" {
            break;
        }
    }

    found_uri == Some(uri)
        && found_mtime.and_then(|value| value.parse::<u64>().ok()) == Some(mtime)
        // Thumb::Size is optional in the spec, but when present it must match.
        && found_size.is_none_or(|value| value.parse::<u64>().ok() == Some(source_size))
}

fn cached_paths(uri: &str, source_stamp: (u64, u64)) -> Vec<PathBuf> {
    let Some(root) = cache_root() else {
        return Vec::new();
    };
    let (mtime, source_size) = source_stamp;
    let digest = format!("{:x}", md5::compute(uri.as_bytes()));

    // Larger flavors can be downscaled, so check the largest one first.
    ["xx-large", "x-large", "large", "normal"]
        .into_iter()
        .filter_map(|size| {
            let candidate = root.join(size).join(format!("{digest}.png"));
            let metadata = std::fs::metadata(&candidate).ok()?;
            // Shared caches can contain files created by other applications.
            // Bound the read/decode cost even when a malformed cache entry is
            // unexpectedly large.
            if metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
                return None;
            }
            let bytes = std::fs::read(&candidate).ok()?;
            valid_thumbnail_metadata(&bytes, uri, mtime, source_size).then_some(candidate)
        })
        .collect()
}

pub(super) fn read_cached(path: &Path, uri: &str, requested_side: u32) -> Option<image::RgbaImage> {
    let expected_stamp = source_stamp(path)?;
    for candidate in cached_paths(uri, expected_stamp) {
        let Ok(reader) = image::ImageReader::open(&candidate) else {
            continue;
        };
        let Ok(reader) = reader.with_guessed_format() else {
            continue;
        };
        let Ok((width, height)) = reader.into_dimensions() else {
            continue;
        };
        let Some(pixel_count) = (width as u64).checked_mul(height as u64) else {
            continue;
        };
        if width == 0
            || height == 0
            || width.max(height) > 4096
            || pixel_count > 16_777_216
            || width.max(height) < requested_side
        {
            continue;
        }
        let Ok(decoded) = image::open(candidate) else {
            continue;
        };
        if source_stamp(path) != Some(expected_stamp) {
            continue;
        }
        return Some(decoded.to_rgba8());
    }
    None
}

/// Store a JPEG Viewer-generated thumbnail in the shared XDG cache using the
/// metadata and directory flavor required by the Freedesktop thumbnail spec.
/// This makes the result reusable by file managers and future application runs.
pub(super) fn store_generated(
    path: &Path,
    uri: &str,
    requested_side: u32,
    expected_source_mtime: u64,
    expected_source_size: u64,
    rgba: &image::RgbaImage,
) {
    // Do not publish a thumbnail if the source changed while it was being made.
    if source_stamp(path) != Some((expected_source_mtime, expected_source_size)) {
        return;
    }
    let Some(root) = cache_root() else {
        return;
    };
    let flavor_size = match requested_side {
        0..=128 => 128,
        129..=256 => 256,
        257..=512 => 512,
        _ => 1024,
    };
    let flavor = match flavor_size {
        128 => "normal",
        256 => "large",
        512 => "x-large",
        _ => "xx-large",
    };
    let directory = root.join(flavor);
    let root_existed = root.exists();
    let directory_existed = directory.exists();
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    // Keep application-created cache directories private to the current user.
    // Existing directories are left untouched to avoid changing other tools'
    // permissions or ownership assumptions.
    if !root_existed {
        let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
    }
    if !directory_existed {
        let _ = std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700));
    }
    let digest = format!("{:x}", md5::compute(uri.as_bytes()));
    let destination = directory.join(format!("{digest}.png"));

    let thumbnail = image::imageops::thumbnail(rgba, flavor_size, flavor_size);
    if thumbnail.width() == 0 || thumbnail.height() == 0 {
        return;
    }

    let id = NEXT_CACHE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        ".jpeg-viewer-{}-{id}.tmp",
        std::process::id()
    ));
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
    else {
        return;
    };

    let write_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut encoder = png::Encoder::new(&mut file, thumbnail.width(), thumbnail.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Thumb::URI".to_owned(), uri.to_owned())?;
        encoder.add_text_chunk("Thumb::MTime".to_owned(), expected_source_mtime.to_string())?;
        encoder.add_text_chunk("Thumb::Size".to_owned(), expected_source_size.to_string())?;
        encoder.add_text_chunk("Software".to_owned(), "JPEG Viewer".to_owned())?;
        let mut writer = encoder.write_header()?;
        writer.write_image_data(thumbnail.as_raw())?;
        writer.finish()?;
        Ok(())
    })();
    if write_result.is_err()
        || source_stamp(path) != Some((expected_source_mtime, expected_source_size))
    {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return;
    }
    drop(file);

    // Atomic rename prevents other applications from observing a partial PNG.
    if std::fs::rename(&temporary, &destination).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

pub(super) fn wait_for_cached(
    path: &Path,
    uri: &str,
    requested_side: u32,
    timeout: std::time::Duration,
) -> Option<image::RgbaImage> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(image) = read_cached(path, uri, requested_side) {
            return Some(image);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::{file_uri, valid_thumbnail_metadata};

    #[test]
    fn file_uri_has_file_scheme_and_escapes_spaces() {
        let path = std::env::temp_dir().join(format!(
            "jpeg-viewer-uri-test-{}-a file.jpg",
            std::process::id()
        ));
        std::fs::write(&path, b"test").expect("create URI test file");
        let uri = file_uri(&path).expect("temporary file should have a URI");
        assert!(uri.starts_with("file:///"));
        assert!(uri.contains("%20"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cache_metadata_must_match_source_uri_and_mtime() {
        let uri = "file:///tmp/a%20file.jpg";
        let mtime = 1_700_000_000u64;
        let source_size = 12_345u64;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .add_text_chunk("Thumb::URI".to_owned(), uri.to_owned())
                .expect("write URI metadata");
            encoder
                .add_text_chunk("Thumb::MTime".to_owned(), mtime.to_string())
                .expect("write mtime metadata");
            encoder
                .add_text_chunk("Thumb::Size".to_owned(), source_size.to_string())
                .expect("write source size metadata");
            let mut writer = encoder.write_header().expect("write PNG header");
            writer.write_image_data(&[255, 0, 0, 255]).expect("write pixel");
            writer.finish().expect("finish PNG");
        }

        assert!(valid_thumbnail_metadata(&bytes, uri, mtime, source_size));
        assert!(!valid_thumbnail_metadata(&bytes, uri, mtime + 1, source_size));
        assert!(!valid_thumbnail_metadata(&bytes, uri, mtime, source_size + 1));
        assert!(!valid_thumbnail_metadata(&bytes, "file:///different.jpg", mtime, source_size));
    }
}
