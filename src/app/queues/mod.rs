//! Queue management split by responsibility.
mod actions;
pub mod sidebar;
mod storage;
mod thumbnails;
mod ui;
pub use sidebar::{render_left_sidebar, render_rename_dialog};

use crate::imaging::entry::ImageEntry;
use std::path::PathBuf;

fn entry_path(entry: &ImageEntry) -> Option<&PathBuf> {
    if let ImageEntry::File(path) = entry {
        Some(path)
    } else {
        None
    }
}

pub(crate) use actions::collect_folder_images;

pub(crate) use thumbnails::{
    SIDEBAR_CURRENT_PRIORITY, SIDEBAR_HOVER_PRIORITY, SIDEBAR_PLACEHOLDER_SIZE,
    SIDEBAR_VISIBLE_PRIORITY, source_thumbnail_key, thumbnail_fit_rect, thumbnail_request_size,
    update_masonry_thumbnail_visibility,
};

pub(crate) use storage::read_queue;
