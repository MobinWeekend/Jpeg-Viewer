//! Portable JPEG Viewer Pop-out preset format and archive operations.

mod archive;
mod bundle;
mod file_ops;
mod load;
mod loaded;
mod preset;
mod save;
#[cfg(test)]
mod tests;

use crate::imaging::entry::{ArchiveImage, ImageEntry, RarArchiveImage, S7ArchiveImage};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub(super) use preset::{PopoutPreset, SaveImageChoice};
pub(super) use load::load_jvp;
pub(super) use save::save_jvp;

const JVP_FORMAT: &str = "jpeg-viewer-popout";
const JVP_VERSION: u32 = 1;
const MANIFEST_PATH: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_BUNDLED_IMAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JvpImage {
    File { path: String },
    Zip { archive_path: String, entry_index: usize, name: String },
    SevenZip { archive_path: String, name: String },
    Rar { archive_path: String, name: String },
    Bundled { member_path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JvpManifest {
    format: String,
    version: u32,
    preset: PopoutPreset,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<JvpImage>,
}

#[derive(Clone, Debug)]
pub(super) struct LoadedJvp {
    manifest: JvpManifest,
    archive_path: PathBuf,
}
