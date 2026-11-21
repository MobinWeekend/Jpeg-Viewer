//! Cross-platform sharing for regular files and virtual archive entries.
//!
//! Archive members are staged in a private temporary directory because native
//! share targets need a filesystem path that remains available after the picker
//! has been opened. Old staged copies are pruned on a later share operation.

use crate::app::types::ViewerApp;
use crate::imaging::entry::ImageEntry;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STAGED_SHARE_DIR: &str = "jpeg-viewer-share";
const STAGED_SHARE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

static NEXT_STAGED_SHARE_ID: AtomicU64 = AtomicU64::new(1);

impl ViewerApp {
    pub(crate) fn share_entry(&mut self, entry: &ImageEntry) {
        match share_entry(entry) {
            Ok(()) => self.queue_status = Some("Opened the system share menu.".into()),
            Err(error) => self.queue_status = Some(format!("Could not share image: {error}")),
        }
    }

    pub(crate) fn share_current_image(&mut self) {
        if self.is_home_surface_active() {
            self.queue_status = Some("Open an image before sharing it.".into());
            return;
        }

        let Some(entry) = self.image_entries.get(self.current_index).cloned() else {
            self.queue_status = Some("There is no image to share.".into());
            return;
        };

        self.share_entry(&entry);
    }
}

fn share_entry(entry: &ImageEntry) -> Result<(), String> {
    let path = match entry {
        ImageEntry::File(path) => {
            if !path.is_file() {
                return Err(format!("{} is not an available file", path.display()));
            }
            path.clone()
        }
        ImageEntry::Zip(_) | ImageEntry::S7z(_) | ImageEntry::Rar(_) => {
            stage_archive_entry(entry)?
        }
    };

    let title = format!("Share {}", entry_name(entry));
    robius_share::ShareSheet::new()
        .set_title(title)
        .add_file(&path)
        .share()
        .map_err(|error| error.to_string())
}

fn stage_archive_entry(entry: &ImageEntry) -> Result<PathBuf, String> {
    let bytes = crate::app::workspace::read_archive_entry_for_share(entry)?;
    let directory = std::env::temp_dir().join(STAGED_SHARE_DIR);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    prune_old_staged_files(&directory);

    let name = safe_file_name(&entry_name(entry));
    let source_name = Path::new(&name);
    let stem = source_name
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("shared-image");
    let extension = source_name.extension().and_then(|value| value.to_str());
    let unique_id = NEXT_STAGED_SHARE_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let unique_suffix = format!("{}-{timestamp}-{unique_id}", std::process::id());
    let staged_name = match extension {
        Some(extension) => format!("{stem}-{unique_suffix}.{extension}"),
        None => format!("{stem}-{unique_suffix}"),
    };
    let staged_path = directory.join(staged_name);
    fs::write(&staged_path, bytes).map_err(|error| error.to_string())?;
    Ok(staged_path)
}

fn entry_name(entry: &ImageEntry) -> String {
    match entry {
        ImageEntry::File(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Image")
            .to_owned(),
        ImageEntry::Zip(item) => item.name.clone(),
        ImageEntry::S7z(item) => item.name.clone(),
        ImageEntry::Rar(item) => item.name.clone(),
    }
}

fn safe_file_name(name: &str) -> String {
    let base_name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let safe_name = base_name
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    if safe_name.trim().is_empty() {
        "shared-image".to_owned()
    } else {
        safe_name
    }
}

fn prune_old_staged_files(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let is_old = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STAGED_SHARE_MAX_AGE);
        if is_old {
            let _ = fs::remove_file(path);
        }
    }
}
