// sort.rs
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::image_entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};

/// Sort method enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMethod {
    /// Natural/alphanumeric sorting (default)
    Natural,
    /// Sort by file modification date (newest first)
    DateNewest,
    /// Sort by file modification date (oldest first)
    DateOldest,
    /// Random shuffle
    Shuffle,
}

impl Default for SortMethod {
    fn default() -> Self {
        Self::Natural
    }
}

/// Trait for getting a path reference from different image types
#[allow(dead_code)] //This acctually gets used!
pub trait AsPath {
    fn as_path(&self) -> &Path;
    fn get_file_name(&self) -> Option<&std::ffi::OsStr>;
}

impl AsPath for ArchiveImage {
    fn as_path(&self) -> &Path {
        &self.archive_path
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        self.archive_path.file_name()
    }
}

impl AsPath for RarArchiveImage {
    fn as_path(&self) -> &Path {
        &self.archive_path
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        self.archive_path.file_name()
    }
}

impl AsPath for S7ArchiveImage {
    fn as_path(&self) -> &Path {
        &self.archive_path
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        self.archive_path.file_name()
    }
}

impl AsPath for PathBuf {
    fn as_path(&self) -> &Path {
        self.as_path()
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        self.file_name()
    }
}

impl AsPath for Path {
    fn as_path(&self) -> &Path {
        self
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        self.file_name()
    }
}

/// Sort images of type T using the specified method
pub fn sort_images<T: AsPath>(images: &mut [T], method: SortMethod) {
    match method {
        SortMethod::Natural => sort_natural(images),
        SortMethod::DateNewest => sort_by_date_newest(images),
        SortMethod::DateOldest => sort_by_date_oldest(images),
        SortMethod::Shuffle => sort_shuffle(images),
    }
}

/// Sort images naturally (alphanumerically)
pub fn sort_natural<T: AsPath>(images: &mut [T]) {
    #[cfg(target_os = "windows")]
    fn compare_filenames(a: &Path, b: &Path) -> std::cmp::Ordering {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::StrCmpLogicalW;
        use windows::core::PCWSTR;

        let a_name = a.file_name().unwrap_or_default();
        let b_name = b.file_name().unwrap_or_default();

        let a_wide: Vec<u16> = a_name.encode_wide().chain(std::iter::once(0)).collect();
        let b_wide: Vec<u16> = b_name.encode_wide().chain(std::iter::once(0)).collect();

        let result = unsafe { StrCmpLogicalW(PCWSTR(a_wide.as_ptr()), PCWSTR(b_wide.as_ptr())) };

        result.cmp(&0)
    }

    #[cfg(not(target_os = "windows"))]
    fn compare_filenames(a: &Path, b: &Path) -> std::cmp::Ordering {
        let a_name = a.file_name().unwrap_or_default().to_string_lossy();
        let b_name = b.file_name().unwrap_or_default().to_string_lossy();

        natord::compare(&a_name.to_lowercase(), &b_name.to_lowercase())
    }

    images.sort_by(|a, b| {
        let a_path = a.as_path();
        let b_path = b.as_path();
        compare_filenames(a_path, b_path)
    });
}

/// Sort images by modification date (newest first)
pub fn sort_by_date_newest<T: AsPath>(images: &mut [T]) {
    images.sort_by(|a, b| {
        let a_time = get_modified_time(a.as_path()).unwrap_or(SystemTime::UNIX_EPOCH);
        let b_time = get_modified_time(b.as_path()).unwrap_or(SystemTime::UNIX_EPOCH);
        b_time.cmp(&a_time) // Newest first (descending)
    });
}

/// Sort images by modification date (oldest first)
pub fn sort_by_date_oldest<T: AsPath>(images: &mut [T]) {
    images.sort_by(|a, b| {
        let a_time = get_modified_time(a.as_path()).unwrap_or(SystemTime::UNIX_EPOCH);
        let b_time = get_modified_time(b.as_path()).unwrap_or(SystemTime::UNIX_EPOCH);
        a_time.cmp(&b_time) // Oldest first (ascending)
    });
}

/// Shuffle images randomly
pub fn sort_shuffle<T: AsPath>(images: &mut [T]) {
    let mut rng = thread_rng();
    images.shuffle(&mut rng);
}

/// Get file modification time, falling back to system start time if metadata fails
fn get_modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}
