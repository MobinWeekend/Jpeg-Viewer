//! Shared Freedesktop thumbnail cache access.
//!
//! Existing desktop cache entries are validated before reuse. When JPEG Viewer
//! itself runs an installed standard thumbnailer, it writes the resulting PNG
//! back with the required URI/mtime metadata and an atomic rename.

use std::path::{Path, PathBuf};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::time::UNIX_EPOCH;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

static NEXT_CACHE_TEMP_ID: AtomicU64 = AtomicU64::new(0);
static PUBLISH_ENABLED: AtomicBool = AtomicBool::new(true);
static CACHE_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn set_publish_enabled(enabled: bool) {
    PUBLISH_ENABLED.store(enabled, Ordering::Release);
}

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
    crate::io_metrics::metadata_query();
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


fn standard_cache_flavors() -> [&'static str; 4] {
    ["normal", "large", "x-large", "xx-large"]
}

fn preferred_cache_flavor_index(requested_side: u32) -> usize {
    match requested_side {
        0..=128 => 0,
        129..=256 => 1,
        257..=512 => 2,
        _ => 3,
    }
}

fn cached_candidates(uri: &str, requested_side: u32) -> Vec<PathBuf> {
    let Some(root) = cache_root() else {
        return Vec::new();
    };
    let digest = format!("{:x}", md5::compute(uri.as_bytes()));
    let flavors = standard_cache_flavors();
    let preferred = preferred_cache_flavor_index(requested_side);

    // Start at the smallest flavor capable of satisfying this request, then
    // consider larger flavors. Smaller flavors cannot pass the size check, so
    // opening them first would be guaranteed extra I/O.
    flavors[preferred..]
        .iter()
        .map(|size| root.join(size).join(format!("{digest}.png")))
        .collect()
}

/// Lightweight candidate signatures let wait_for_cached poll for desktop
/// output without rereading unchanged PNG files every interval.
fn cached_candidate_state(uri: &str, requested_side: u32) -> Vec<Option<(u64, std::time::SystemTime)>> {
    cached_candidates(uri, requested_side)
        .into_iter()
        .map(|candidate| {
            crate::io_metrics::metadata_query();
            std::fs::metadata(candidate).ok().and_then(|metadata| {
                Some((metadata.len(), metadata.modified().ok()?))
            })
        })
        .collect()
}

pub(super) fn read_cached(path: &Path, uri: &str, requested_side: u32) -> Option<image::RgbaImage> {
    let expected_stamp = source_stamp(path)?;
    for candidate in cached_candidates(uri, requested_side) {
        crate::io_metrics::metadata_query();
        let Ok(metadata) = std::fs::metadata(&candidate) else {
            continue;
        };
        // Shared caches can contain files created by other applications. Bound
        // the read/decode cost even when a malformed entry is unexpectedly large.
        if metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
            continue;
        }
        let bytes = match std::fs::read(&candidate) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        crate::io_metrics::thumbnail_cache_read(bytes.len());
        if !valid_thumbnail_metadata(&bytes, uri, expected_stamp.0, expected_stamp.1) {
            continue;
        }
        // Decode the same in-memory bytes that were just validated. This avoids
        // separate ImageReader/open calls for dimensions and pixel decoding.
        let Ok(decoded) = image::load_from_memory(&bytes) else {
            continue;
        };
        let (width, height) = (decoded.width(), decoded.height());
        let Some(pixel_count) = (width as u64).checked_mul(height as u64) else {
            continue;
        };
        if width == 0 || height == 0 || pixel_count > 16_777_216
            || width.max(height) < requested_side
        {
            continue;
        }
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
    // Publishing is optional. Existing XDG entries are still read when this is
    // disabled; only Jpeg-Viewer's own persistent cache writes are suppressed.
    if !PUBLISH_ENABLED.load(Ordering::Acquire) {
        return;
    }
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

    // Serialize publish checks so concurrent requests for the same source do
    // not each rewrite the same shared cache file. A valid existing entry wins.
    let _write_guard = CACHE_WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::io_metrics::metadata_query();
    if std::fs::metadata(&destination)
        .is_ok_and(|metadata| metadata.len() > 0 && metadata.len() <= 64 * 1024 * 1024)
    {
        if let Ok(existing) = std::fs::read(&destination) {
            crate::io_metrics::thumbnail_cache_read(existing.len());
            if valid_thumbnail_metadata(
                &existing,
                uri,
                expected_source_mtime,
                expected_source_size,
            ) {
                return;
            }
        }
    }

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
    } else {
        crate::io_metrics::thumbnail_cache_write();
    }
}

pub(super) fn wait_for_cached(
    path: &Path,
    uri: &str,
    requested_side: u32,
    timeout: std::time::Duration,
) -> Option<image::RgbaImage> {
    let deadline = std::time::Instant::now() + timeout;
    let mut previous_state: Option<Vec<Option<(u64, std::time::SystemTime)>>> = None;
    loop {
        let state = cached_candidate_state(uri, requested_side);
        if previous_state.as_ref() != Some(&state) {
            if let Some(image) = read_cached(path, uri, requested_side) {
                return Some(image);
            }
            previous_state = Some(state);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        // Desktop thumbnail services are asynchronous. Poll metadata at 250ms,
        // but only reopen/decode candidates when their size/mtime changes.
        std::thread::sleep(std::time::Duration::from_millis(250));
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


#[cfg(test)]
mod cache_flavor_tests {
    use super::{preferred_cache_flavor_index, standard_cache_flavors};

    #[test]
    fn standard_flavors_are_ordered_from_smallest_to_largest() {
        assert_eq!(
            standard_cache_flavors(),
            ["normal", "large", "x-large", "xx-large"],
        );
    }

    #[test]
    fn candidate_lookup_starts_at_the_smallest_suitable_flavor() {
        assert_eq!(preferred_cache_flavor_index(128), 0);
        assert_eq!(preferred_cache_flavor_index(256), 1);
        assert_eq!(preferred_cache_flavor_index(512), 2);
        assert_eq!(preferred_cache_flavor_index(1024), 3);
    }
}
