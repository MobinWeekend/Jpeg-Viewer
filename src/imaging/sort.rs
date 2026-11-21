// sort.rs
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::imaging::entry::{ArchiveImage, RarArchiveImage, S7ArchiveImage};

/// Sort method enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMethod {
    /// Natural/alphanumeric sorting (default)
    #[default]
    Natural,
    /// Sort by file modification date (newest first)
    DateNewest,
    /// Sort by file modification date (oldest first)
    DateOldest,
    /// Random shuffle
    Shuffle,
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

impl AsPath for crate::imaging::entry::ImageEntry {
    fn as_path(&self) -> &Path {
        use crate::imaging::entry::ImageEntry;
        match self {
            ImageEntry::File(path) => path.as_path(),
            ImageEntry::Zip(item) => item.archive_path.as_path(),
            ImageEntry::S7z(item) => item.archive_path.as_path(),
            ImageEntry::Rar(item) => item.archive_path.as_path(),
        }
    }

    fn get_file_name(&self) -> Option<&std::ffi::OsStr> {
        use crate::imaging::entry::ImageEntry;
        match self {
            ImageEntry::File(path) => path.file_name(),
            ImageEntry::Zip(item) => Path::new(&item.name).file_name(),
            ImageEntry::S7z(item) => Path::new(&item.name).file_name(),
            ImageEntry::Rar(item) => Path::new(&item.name).file_name(),
        }
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
    sort_by_cached_modified_time(images, true);
}

/// Sort images by modification date (oldest first)
pub fn sort_by_date_oldest<T: AsPath>(images: &mut [T]) {
    sort_by_cached_modified_time(images, false);
}

/// Fetch each modification timestamp exactly once, then sort using the cached
/// values. `sort_by_cached_key` moves the original entries without cloning them.
fn sort_by_cached_modified_time<T: AsPath>(images: &mut [T], newest_first: bool) {
    if newest_first {
        images.sort_by_cached_key(|image| {
            std::cmp::Reverse(get_modified_time(image.as_path()).unwrap_or(SystemTime::UNIX_EPOCH))
        });
    } else {
        images.sort_by_cached_key(|image| {
            get_modified_time(image.as_path()).unwrap_or(SystemTime::UNIX_EPOCH)
        });
    }
}

/// Shuffle images randomly
pub fn sort_shuffle<T: AsPath>(images: &mut [T]) {
    let mut rng = thread_rng();
    images.shuffle(&mut rng);
}

/// Get file modification time, falling back to system start time if metadata fails
fn get_modified_time(path: &Path) -> Option<SystemTime> {
    crate::io_metrics::metadata_query();
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}
