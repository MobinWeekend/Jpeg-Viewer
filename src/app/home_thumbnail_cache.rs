//! Small persistent cache used exclusively by the Home page's shortcut/recent cards.
//!
//! The cache lives beside settings.ini in `cache/`. Entries are compact RGBA
//! buffers with a source-stamp-derived filename, so replacing a file or archive
//! naturally selects a new cache entry without probing/deleting stale files on
//! every Home render.

use crate::app::types::GeneratedThumbnail;
use crate::imaging::entry::ImageEntry;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAGIC: &[u8; 8] = b"JVHTHMB1";
const HEADER_LEN: usize = 28;
const MAX_THUMBNAIL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 512;

pub(crate) struct CachedHomeThumbnail {
    pub width: usize,
    pub height: usize,
    pub resolution: u32,
    pub pixels: Vec<u8>,
}

/// Resolve a persistent cache file for this exact source revision and requested
/// physical size. Files inside archives use the archive's own timestamp/length
/// together with the archive-member identity from `source_thumbnail_key`.
pub(crate) fn cache_path(
    settings_path: &Path,
    entry: &ImageEntry,
    requested_size: u32,
) -> Option<PathBuf> {
    let (source_path, source_key) = match entry {
        ImageEntry::File(path) => (path.as_path(), crate::app::queues::source_thumbnail_key(entry)),
        ImageEntry::Zip(item) => (
            item.archive_path.as_path(),
            crate::app::queues::source_thumbnail_key(entry),
        ),
        ImageEntry::S7z(item) => (
            item.archive_path.as_path(),
            crate::app::queues::source_thumbnail_key(entry),
        ),
        ImageEntry::Rar(item) => (
            item.archive_path.as_path(),
            crate::app::queues::source_thumbnail_key(entry),
        ),
    };

    let metadata = fs::metadata(source_path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let identity = format!(
        "home-thumbnail-v1\0{source_key}\0{}\0{modified_nanos}\0{}",
        metadata.len(),
        requested_size.max(1)
    );
    let hash = stable_hash(identity.as_bytes());
    let directory = settings_path.parent()?.join("cache");
    Some(directory.join(format!(
        "home-thumb-{hash:016x}-{}.rgba",
        requested_size.max(1)
    )))
}

pub(crate) fn load(path: &Path) -> Option<CachedHomeThumbnail> {
    let file_len = fs::metadata(path).ok()?.len();
    if !(HEADER_LEN as u64..=HEADER_LEN as u64 + MAX_THUMBNAIL_BYTES).contains(&file_len) {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return None;
    }

    let width = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    let height = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    let resolution = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    let pixel_bytes = u64::from_le_bytes(bytes[20..28].try_into().ok()?);
    let expected = (width as u64)
        .checked_mul(height as u64)?
        .checked_mul(4)?;
    if width == 0
        || height == 0
        || pixel_bytes != expected
        || pixel_bytes > MAX_THUMBNAIL_BYTES
        || bytes.len() as u64 != HEADER_LEN as u64 + pixel_bytes
    {
        return None;
    }

    Some(CachedHomeThumbnail {
        width,
        height,
        resolution: resolution.max(1),
        pixels: bytes[HEADER_LEN..].to_vec(),
    })
}

pub(crate) fn store(path: &Path, thumbnail: &GeneratedThumbnail) {
    let Some(pixel_bytes) = thumbnail
        .width
        .checked_mul(thumbnail.height)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return;
    };
    if thumbnail.width == 0
        || thumbnail.height == 0
        || pixel_bytes != thumbnail.pixels.len()
        || pixel_bytes as u64 > MAX_THUMBNAIL_BYTES
        || thumbnail.width > u32::MAX as usize
        || thumbnail.height > u32::MAX as usize
    {
        return;
    }

    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        eprintln!("[home thumbnail cache] could not create cache directory: {error}");
        return;
    }

    let temporary = path.with_extension(format!("rgba.tmp-{}", std::process::id()));
    let mut file = match fs::File::create(&temporary) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("[home thumbnail cache] could not create cache file: {error}");
            return;
        }
    };
    let write_result = (|| -> std::io::Result<()> {
        file.write_all(MAGIC)?;
        file.write_all(&(thumbnail.width as u32).to_le_bytes())?;
        file.write_all(&(thumbnail.height as u32).to_le_bytes())?;
        file.write_all(&thumbnail.resolution.max(1).to_le_bytes())?;
        file.write_all(&(pixel_bytes as u64).to_le_bytes())?;
        file.write_all(&thumbnail.pixels)?;
        file.flush()?;
        drop(file);
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(()),
            Err(first_error) => {
                // Windows does not replace an existing destination on rename.
                if path.exists() {
                    fs::remove_file(path)?;
                    fs::rename(&temporary, path)
                } else {
                    Err(first_error)
                }
            }
        }
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        eprintln!("[home thumbnail cache] could not write cache file: {error}");
        return;
    }

    prune_cache_directory(parent);
}

fn prune_cache_directory(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut cached_files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("home-thumb-") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(UNIX_EPOCH),
            ))
        })
        .collect::<Vec<_>>();

    if cached_files.len() <= MAX_CACHE_ENTRIES
        && cached_files.iter().map(|(_, size, _)| *size).sum::<u64>() <= MAX_CACHE_BYTES
    {
        return;
    }
    cached_files.sort_by_key(|(_, _, modified)| *modified);
    let mut total_bytes = cached_files.iter().map(|(_, size, _)| *size).sum::<u64>();
    let mut total_entries = cached_files.len();
    for (path, size, _) in cached_files {
        if total_entries <= MAX_CACHE_ENTRIES && total_bytes <= MAX_CACHE_BYTES {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total_entries = total_entries.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(size);
        }
    }
}

/// Stable FNV-1a keeps cache filenames independent of hash-randomization seeds.
fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trip_and_rejects_corrupt_files() {
        let root = std::env::temp_dir().join(format!(
            "jpeg-viewer-home-thumbnail-cache-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("thumbnail.rgba");
        let thumbnail = GeneratedThumbnail {
            width: 2,
            height: 1,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
            resolution: 2,
            source_aspect_ratio: Some(2.0),
            source_dimensions: Some((2, 1)),
        };
        store(&path, &thumbnail);
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.width, 2);
        assert_eq!(loaded.height, 1);
        assert_eq!(loaded.resolution, 2);
        assert_eq!(loaded.pixels, thumbnail.pixels);
        fs::write(&path, b"not a valid cache").unwrap();
        assert!(load(&path).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
