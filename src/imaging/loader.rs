use crate::imaging::helpers::is_supported_image;
use crate::imaging::sort::{SortMethod, sort_images};
use std::fs;
use std::path::{Path, PathBuf};

// ========== Directory Loading ==========

/// Load all supported images from a directory with optional sorting
pub fn load_directory_images(path: &Path, sort_method: SortMethod) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| {
            entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    // Extension checks are pure path operations. Reject unrelated
                    // files before asking the filesystem whether they are regular files.
                    if is_supported_image(&path) {
                        crate::io_metrics::metadata_query();
                        let file_type = entry.file_type().ok()?;
                        // `Path::is_file()` followed symlinks in the previous
                        // implementation. Keep that behavior, but pay the
                        // extra target lookup only for actual symlink entries.
                        if file_type.is_file() || (file_type.is_symlink() && path.is_file()) {
                            Some(path)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    sort_images(&mut files, sort_method);
    files
}
