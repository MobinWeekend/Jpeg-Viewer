//! Shared path, archive, and file-operation helpers for workspace features.

use super::*;

pub(super) fn operation_has_conflicts(operation: &PendingFileOperation) -> bool {
    let mut seen_targets = HashSet::new();
    operation.entries.iter().any(|entry| {
        let target = operation.destination.join(entry_display_name(entry));
        let duplicate_in_batch = !seen_targets.insert(target.clone());
        let source_is_target =
            matches!(entry, ImageEntry::File(path) if operation.move_files && *path == target);
        (target.exists() && !source_is_target) || duplicate_in_batch
    })
}

pub(super) fn entry_source_available(entry: &ImageEntry) -> bool {
    match entry {
        ImageEntry::File(path) => path.is_file(),
        ImageEntry::Zip(item) => item.archive_path.is_file(),
        ImageEntry::S7z(item) => item.archive_path.is_file(),
        ImageEntry::Rar(item) => item.archive_path.is_file(),
    }
}

pub(super) fn is_welcome_recent_path(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "jvq" | "txt" | "zip" | "7z" | "rar"
    )
}

pub(super) fn welcome_paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

pub(super) fn sort_method_label(method: crate::imaging::sort::SortMethod) -> &'static str {
    match method {
        crate::imaging::sort::SortMethod::Natural => "Name (natural)",
        crate::imaging::sort::SortMethod::DateNewest => "Newest first",
        crate::imaging::sort::SortMethod::DateOldest => "Oldest first",
        crate::imaging::sort::SortMethod::Shuffle => "Random",
    }
}

pub(super) fn entry_display_name(entry: &ImageEntry) -> String {
    match entry {
        ImageEntry::File(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Image")
            .to_owned(),
        ImageEntry::Zip(item) => Path::new(&item.name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(item.name.as_str())
            .to_owned(),
        ImageEntry::S7z(item) => Path::new(&item.name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(item.name.as_str())
            .to_owned(),
        ImageEntry::Rar(item) => Path::new(&item.name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(item.name.as_str())
            .to_owned(),
    }
}

pub(super) fn entry_parent(entry: &ImageEntry) -> Option<PathBuf> {
    match entry {
        ImageEntry::File(path) => path.parent().map(Path::to_path_buf),
        ImageEntry::Zip(item) => item.archive_path.parent().map(Path::to_path_buf),
        ImageEntry::S7z(item) => item.archive_path.parent().map(Path::to_path_buf),
        ImageEntry::Rar(item) => item.archive_path.parent().map(Path::to_path_buf),
    }
}

/// A borrowed, allocation-free identity for an entry's containing folder.
/// It mirrors `entry_breadcrumb` but is intended for fast equality checks while
/// deciding whether a large Masonry collection needs folder headers.
pub(super) fn entry_folder_parts<'a>(
    entry: &'a ImageEntry,
    root: Option<&Path>,
) -> (Option<&'a Path>, Option<&'a Path>) {
    fn relative_container<'a>(container: &'a Path, root: Option<&Path>) -> &'a Path {
        root.and_then(|root| container.strip_prefix(root).ok())
            .unwrap_or(container)
    }

    match entry {
        ImageEntry::File(path) => (
            path.parent()
                .map(|container| relative_container(container, root)),
            None,
        ),
        ImageEntry::Zip(item) => (
            Some(relative_container(&item.archive_path, root)),
            Path::new(&item.name)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty()),
        ),
        ImageEntry::S7z(item) => (
            Some(relative_container(&item.archive_path, root)),
            Path::new(&item.name)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty()),
        ),
        ImageEntry::Rar(item) => (
            Some(relative_container(&item.archive_path, root)),
            Path::new(&item.name)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty()),
        ),
    }
}

pub(super) fn entry_breadcrumb(entry: &ImageEntry, root: Option<&Path>) -> String {
    let (container, inner) = match entry {
        ImageEntry::File(path) => (path.parent(), None),
        ImageEntry::Zip(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
        ImageEntry::S7z(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
        ImageEntry::Rar(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
    };
    let mut parts = Vec::new();
    if let Some(container) = container {
        let display = root
            .and_then(|root| container.strip_prefix(root).ok())
            .unwrap_or(container);
        let value = display.to_string_lossy();
        if !value.is_empty() && value != "." {
            parts.push(value.into_owned());
        }
    }
    if let Some(inner) = inner
        && let Some(parent) = Path::new(inner).parent()
        && !parent.as_os_str().is_empty()
    {
        parts.push(parent.to_string_lossy().into_owned());
    }
    if parts.is_empty() {
        "Current folder".to_owned()
    } else {
        parts.join(" / ")
    }
}

pub(super) fn unique_destination_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = path.extension().and_then(|value| value.to_str());
    for suffix in 2..10_000u32 {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-copy"))
}

pub(super) fn unique_sidecar_path(path: &Path, purpose: &str) -> PathBuf {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    for _ in 0..10_000 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.jpeg-viewer-{purpose}-{}-{id}",
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(".{name}.jpeg-viewer-{purpose}-fallback"))
}

pub(super) fn stage_entry_to_file(entry: &ImageEntry, staged: &Path) -> Result<(), String> {
    match entry {
        ImageEntry::File(source) => fs::copy(source, staged)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        archive_entry => {
            let bytes = read_archive_entry(archive_entry)?;
            fs::write(staged, bytes).map_err(|error| error.to_string())
        }
    }
}

pub(super) fn commit_staged_file(
    staged: &Path,
    destination: &Path,
    allow_replace: bool,
) -> Result<(), String> {
    let backup = if destination.exists() {
        if !allow_replace {
            return Err(
                "the destination appeared during the operation; no file was overwritten".into(),
            );
        }
        if !destination.is_file() {
            return Err("the destination exists and is not a regular file".into());
        }
        let backup = unique_sidecar_path(destination, "backup");
        fs::rename(destination, &backup)
            .map_err(|error| format!("could not preserve the existing destination: {error}"))?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = fs::rename(staged, destination) {
        if let Some(backup) = backup.as_ref()
            && let Err(restore_error) = fs::rename(backup, destination)
        {
            return Err(format!(
                "could not install replacement ({error}) or restore the original ({restore_error}); backup remains at {}",
                backup.display()
            ));
        }
        return Err(format!("could not install staged file: {error}"));
    }

    if let Some(backup) = backup
        && let Err(error) = fs::remove_file(&backup)
    {
        // The new destination is committed. Preserve the backup and surface
        // its path instead of reporting the replacement as a total failure.
        return Err(format!(
            "destination was replaced, but backup cleanup failed: {} ({error})",
            backup.display()
        ));
    }
    Ok(())
}

pub(super) fn read_archive_entry(entry: &ImageEntry) -> Result<Vec<u8>, String> {
    match entry {
        ImageEntry::File(path) => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            crate::io_metrics::source_read(bytes.len());
            Ok(bytes)
        }
        ImageEntry::Zip(item) => {
            let file = fs::File::open(&item.archive_path).map_err(|error| error.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
            let mut source = archive
                .by_index(item.entry_index)
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            source
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            crate::io_metrics::archive_extraction(bytes.len());
            Ok(bytes)
        }
        ImageEntry::S7z(item) => {
            let mut archive = sevenz_rust2::ArchiveReader::open(
                &item.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|error| error.to_string())?;
            let bytes = archive
                .read_file(&item.name)
                .map_err(|error| error.to_string())?;
            crate::io_metrics::archive_extraction(bytes.len());
            Ok(bytes)
        }
        ImageEntry::Rar(item) => {
            let archive = unrar::Archive::new(&item.archive_path)
                .open_for_processing()
                .map_err(|error| error.to_string())?;
            let mut archive = archive;
            loop {
                let header = archive
                    .read_header()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("{} not found in archive", item.name))?;
                let name = header.entry().filename.to_string_lossy().to_string();
                if name == item.name {
                    let (bytes, _) = header.read().map_err(|error| error.to_string())?;
                    crate::io_metrics::archive_extraction(bytes.len());
                    return Ok(bytes);
                }
                archive = header.skip().map_err(|error| error.to_string())?;
            }
        }
    }
}

#[cfg(test)]
mod welcome_recent_tests {
    use super::is_welcome_recent_path;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recently_opened_only_accepts_folders_queues_and_archives() {
        assert!(!is_welcome_recent_path(Path::new("photo.jpg")));
        assert!(!is_welcome_recent_path(Path::new("document.png")));
        assert!(is_welcome_recent_path(Path::new("saved-queue.jvq")));
        assert!(is_welcome_recent_path(Path::new("path-list.txt")));
        assert!(is_welcome_recent_path(Path::new("images.zip")));
        assert!(is_welcome_recent_path(Path::new("images.7z")));
        assert!(is_welcome_recent_path(Path::new("images.rar")));

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let folder = std::env::temp_dir().join(format!("jpeg-viewer-recent-folder-{unique}"));
        std::fs::create_dir_all(&folder).unwrap();
        assert!(is_welcome_recent_path(&folder));
        let _ = std::fs::remove_dir_all(folder);
    }
}
