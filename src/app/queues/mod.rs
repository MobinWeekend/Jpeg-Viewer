//! Queue management split by responsibility.
mod actions;
mod storage;
mod thumbnails;
mod ui;

use crate::image_entry::ImageEntry;
use std::path::PathBuf;

fn entry_path(entry: &ImageEntry) -> Option<&PathBuf> {
    if let ImageEntry::File(path) = entry {
        Some(path)
    } else {
        None
    }
}
