//! Portable pop-out presets (`.jvp`). A JVP is a ZIP container with a versioned
//! JSON manifest and, optionally, one bundled image entry.

use crate::imaging::entry::{ArchiveImage, ImageEntry, RarArchiveImage, S7ArchiveImage};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const JVP_FORMAT: &str = "jpeg-viewer-popout";
const JVP_VERSION: u32 = 1;
const MANIFEST_PATH: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_BUNDLED_IMAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SaveImageChoice {
    Bundle,
    Link,
    #[default]
    PresetOnly,
}

/// Options which belong to an Image tab rather than to the native viewport's
/// transient runtime state. Kept separately so a preset can be loaded before
/// its pop-out window is opened.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PopoutPreset {
    pub size_scale: f32,
    pub opacity_percent: f32,
    pub click_through: bool,
    pub pass_near_enabled: bool,
    pub pass_near_distance: f32,
    pub pass_near_opacity: f32,
    pub pass_hover_enabled: bool,
    pub pass_hover_opacity: f32,
    pub show_bounds: bool,
    pub dvd_mode: bool,
    pub dvd_circle_collision: bool,
    pub random_reflect_on_bounce: bool,
    pub random_rotate_on_bounce: bool,
    pub dvd_speed: f32,
    pub dvd_rotation_ease: f32,
    pub dvd_edge_offsets: [f32; 4],
    pub monitor_id: Option<String>,
    pub ball_mode: bool,
    pub ball_gravity: f32,
    pub ball_bounciness: f32,
    pub ball_friction: f32,
    pub ball_rotate: bool,
    pub random_bounce_enabled: bool,
    pub random_bounce_interval: f32,
    pub stay_on_top: bool,
    pub stay_on_bottom: bool,
}

impl Default for PopoutPreset {
    fn default() -> Self {
        Self {
            size_scale: 1.0,
            opacity_percent: 100.0,
            click_through: false,
            pass_near_enabled: false,
            pass_near_distance: 120.0,
            pass_near_opacity: 75.0,
            pass_hover_enabled: false,
            pass_hover_opacity: 100.0,
            show_bounds: false,
            dvd_mode: false,
            dvd_circle_collision: false,
            random_reflect_on_bounce: false,
            random_rotate_on_bounce: false,
            dvd_speed: 1.0,
            dvd_rotation_ease: 0.7,
            dvd_edge_offsets: [0.0; 4],
            monitor_id: None,
            ball_mode: false,
            ball_gravity: 1100.0,
            ball_bounciness: 0.82,
            ball_friction: 0.18,
            ball_rotate: true,
            random_bounce_enabled: false,
            random_bounce_interval: 6.0,
            stay_on_top: false,
            stay_on_bottom: false,
        }
    }
}

impl PopoutPreset {
    pub(super) fn sanitize(&mut self) {
        self.size_scale = finite_clamp(self.size_scale, 0.1, 8.0, 1.0);
        self.opacity_percent = finite_clamp(self.opacity_percent, 0.0, 100.0, 100.0);
        self.pass_near_distance = finite_clamp(self.pass_near_distance, 10.0, 500.0, 120.0);
        self.pass_near_opacity = finite_clamp(self.pass_near_opacity, 0.0, 100.0, 75.0);
        self.pass_hover_opacity = finite_clamp(self.pass_hover_opacity, 0.0, 100.0, 100.0);
        self.dvd_speed = finite_clamp(self.dvd_speed, 0.1, 4.0, 1.0);
        self.dvd_rotation_ease = finite_clamp(self.dvd_rotation_ease, 0.1, 4.0, 0.7);
        for offset in &mut self.dvd_edge_offsets {
            *offset = finite_clamp(*offset, 0.0, 1000.0, 0.0);
        }
        self.ball_gravity = finite_clamp(self.ball_gravity, 0.0, 2400.0, 1100.0);
        self.ball_bounciness = finite_clamp(self.ball_bounciness, 0.0, 1.0, 0.82);
        self.ball_friction = finite_clamp(self.ball_friction, 0.0, 5.0, 0.18);
        self.random_bounce_interval =
            finite_clamp(self.random_bounce_interval, 0.5, 30.0, 6.0);
        if self.dvd_mode {
            self.ball_mode = false;
        }
        if self.ball_mode {
            self.dvd_mode = false;
        }
        if self.stay_on_top {
            self.stay_on_bottom = false;
        }
        if self.stay_on_bottom {
            self.stay_on_top = false;
        }
    }
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum JvpImage {
    File { path: String },
    Zip {
        archive_path: String,
        entry_index: usize,
        name: String,
    },
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

impl LoadedJvp {
    pub(super) fn preset(&self) -> PopoutPreset {
        self.manifest.preset.clone()
    }

    pub(super) fn has_image(&self) -> bool {
        self.manifest.image.is_some()
    }

    pub(super) fn resolve_image(&self) -> Result<Option<ImageEntry>, String> {
        let Some(image) = self.manifest.image.as_ref() else {
            return Ok(None);
        };
        let entry = match image {
            JvpImage::File { path } => {
                let path = PathBuf::from(path);
                if !path.is_file() {
                    return Err(format!("Linked image does not exist: {}", path.display()));
                }
                ImageEntry::File(path)
            }
            JvpImage::Zip {
                archive_path,
                entry_index,
                name,
            } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!("Linked ZIP archive does not exist: {}", archive_path.display()));
                }
                let mut archive = open_zip(&archive_path)?;
                let member = archive.by_index(*entry_index).map_err(|error| {
                    format!("The linked ZIP entry at index {entry_index} is unavailable: {error}")
                })?;
                if member.name() != name {
                    return Err(format!(
                        "The linked ZIP entry changed: expected '{name}', found '{}'",
                        member.name()
                    ));
                }
                ImageEntry::Zip(ArchiveImage {
                    archive_path,
                    entry_index: *entry_index,
                    name: name.clone(),
                })
            }
            JvpImage::SevenZip { archive_path, name } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!("Linked 7z archive does not exist: {}", archive_path.display()));
                }
                ImageEntry::S7z(S7ArchiveImage {
                    archive_path,
                    name: name.clone(),
                })
            }
            JvpImage::Rar { archive_path, name } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!("Linked RAR archive does not exist: {}", archive_path.display()));
                }
                ImageEntry::Rar(RarArchiveImage {
                    archive_path,
                    name: name.clone(),
                })
            }
            JvpImage::Bundled { member_path } => {
                validate_bundle_member_path(member_path)?;
                let mut archive = open_zip(&self.archive_path)?;
                let member = archive
                    .by_name(member_path)
                    .map_err(|error| format!("The bundled image is missing from this JVP: {error}"))?;
                if member.size() > MAX_BUNDLED_IMAGE_BYTES {
                    return Err("The bundled image exceeds the 2 GiB safety limit.".into());
                }
                let mut bytes = Vec::new();
                member
                    .take(MAX_BUNDLED_IMAGE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("Could not extract the bundled image: {error}"))?;
                if bytes.len() as u64 > MAX_BUNDLED_IMAGE_BYTES {
                    return Err("The bundled image exceeds the 2 GiB safety limit.".into());
                }
                if crate::decoder::format_detection::detect_format(&bytes).is_none() {
                    return Err("The bundled data is not a supported image format.".into());
                }
                ImageEntry::File(write_bundled_image(member_path, &bytes)?)
            }
        };

        // Validate archive members from their directory metadata without
        // decompressing the image into memory or changing the active tab.
        let _ = validate_linked_archive_entry(&entry)?;
        Ok(Some(entry))
    }
}

pub(super) fn save_jvp(
    path: &Path,
    preset: &PopoutPreset,
    entry: Option<&ImageEntry>,
    choice: SaveImageChoice,
) -> Result<(), String> {
    let mut preset = preset.clone();
    preset.sanitize();
    let mut image = None;
    let mut bundled_entry = None;

    match choice {
        SaveImageChoice::PresetOnly => {}
        SaveImageChoice::Link => {
            let entry = entry.ok_or_else(|| "There is no current image to link.".to_owned())?;
            image = Some(linked_image(entry)?);
        }
        SaveImageChoice::Bundle => {
            let entry = entry.ok_or_else(|| "There is no current image to bundle.".to_owned())?;
            let uncompressed_size = validate_linked_archive_entry(entry)?;
            if uncompressed_size > MAX_BUNDLED_IMAGE_BYTES {
                return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
            }
            let filename = entry_file_name(entry);
            let member_path = format!("image/{filename}");
            image = Some(JvpImage::Bundled {
                member_path: member_path.clone(),
            });
            bundled_entry = Some((member_path, entry.clone()));
        }
    }

    let manifest = JvpManifest {
        format: JVP_FORMAT.to_owned(),
        version: JVP_VERSION,
        preset,
        image,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Could not encode the JVP manifest: {error}"))?;
    let (temporary_path, file) = create_temporary_file(path)?;
    let mut writer = ZipWriter::new(file);
    let write_result = (|| -> Result<(), String> {
        writer
            .start_file(
                MANIFEST_PATH,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|error| format!("Could not write the JVP manifest: {error}"))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(|error| format!("Could not write the JVP manifest: {error}"))?;
        if let Some((member_path, entry)) = bundled_entry {
            writer
                .start_file(
                    member_path,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .map_err(|error| format!("Could not add the bundled image to the JVP: {error}"))?;
            write_bundled_entry(&mut writer, &entry)?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(writer);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    let output = match writer.finish() {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!("Could not finish the JVP archive: {error}"));
        }
    };
    if let Err(error) = output.sync_all() {
        drop(output);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Could not flush the JVP archive to disk: {error}"));
    }
    drop(output);
    replace_file_atomically(&temporary_path, path)?;
    Ok(())
}

pub(super) fn load_jvp(path: &Path) -> Result<LoadedJvp, String> {
    if !path.is_file() {
        return Err(format!("JVP file does not exist: {}", path.display()));
    }
    let mut archive = open_zip(path)?;
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|error| format!("This is not a valid JPEG Viewer preset (manifest missing): {error}"))?;
    if manifest_file.size() > MAX_MANIFEST_BYTES {
        return Err("The JVP manifest is unexpectedly large.".into());
    }
    let mut manifest_bytes = Vec::new();
    (&mut manifest_file)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| format!("Could not read the JVP manifest: {error}"))?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("The JVP manifest is unexpectedly large.".into());
    }
    drop(manifest_file);
    drop(archive);

    let mut manifest: JvpManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("The JVP manifest is invalid: {error}"))?;
    if manifest.format != JVP_FORMAT {
        return Err("This archive is not a JPEG Viewer Pop-out preset.".into());
    }
    if manifest.version != JVP_VERSION {
        return Err(format!(
            "Unsupported JVP version {} (this version supports {}).",
            manifest.version, JVP_VERSION
        ));
    }
    manifest.preset.sanitize();
    if let Some(JvpImage::Bundled { member_path }) = manifest.image.as_ref() {
        validate_bundle_member_path(member_path)?;
        let mut archive = open_zip(path)?;
        let member = archive
            .by_name(member_path)
            .map_err(|error| format!("The JVP references a missing bundled image: {error}"))?;
        if member.size() > MAX_BUNDLED_IMAGE_BYTES {
            return Err("The bundled image exceeds the 2 GiB safety limit.".into());
        }
    }

    Ok(LoadedJvp {
        manifest,
        archive_path: path.to_path_buf(),
    })
}

fn open_zip(path: &Path) -> Result<ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    ZipArchive::new(file).map_err(|error| format!("Could not read JVP archive: {error}"))
}

fn linked_image(entry: &ImageEntry) -> Result<JvpImage, String> {
    let _ = validate_linked_archive_entry(entry)?;
    let canonical_file = |path: &Path, description: &str| -> Result<String, String> {
        if !path.is_file() {
            return Err(format!("The {description} is unavailable: {}", path.display()));
        }
        path.canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("Could not resolve the {description}: {error}"))
    };
    Ok(match entry {
        ImageEntry::File(path) => JvpImage::File {
            path: canonical_file(path, "image file")?,
        },
        ImageEntry::Zip(item) => JvpImage::Zip {
            archive_path: canonical_file(&item.archive_path, "ZIP archive")?,
            entry_index: item.entry_index,
            name: item.name.clone(),
        },
        ImageEntry::S7z(item) => JvpImage::SevenZip {
            archive_path: canonical_file(&item.archive_path, "7z archive")?,
            name: item.name.clone(),
        },
        ImageEntry::Rar(item) => JvpImage::Rar {
            archive_path: canonical_file(&item.archive_path, "RAR archive")?,
            name: item.name.clone(),
        },
    })
}

fn validate_linked_archive_entry(entry: &ImageEntry) -> Result<u64, String> {
    match entry {
        ImageEntry::File(path) => {
            if !path.is_file() {
                return Err(format!("The linked image file is unavailable: {}", path.display()));
            }
            fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|error| format!("Could not inspect the linked image file: {error}"))
        }
        ImageEntry::Zip(item) => {
            let file = std::fs::File::open(&item.archive_path)
                .map_err(|error| format!("Could not open the linked ZIP archive: {error}"))?;
            let mut archive = ZipArchive::new(file)
                .map_err(|error| format!("Could not read the linked ZIP archive: {error}"))?;
            let member = archive
                .by_index(item.entry_index)
                .map_err(|error| format!("The linked ZIP entry is unavailable: {error}"))?;
            if member.name() != item.name.as_str() {
                return Err(format!(
                    "The linked ZIP entry changed: expected '{}', found '{}'",
                    item.name,
                    member.name()
                ));
            }
            Ok(member.size())
        }
        ImageEntry::S7z(item) => {
            let archive = sevenz_rust2::Archive::open(&item.archive_path)
                .map_err(|error| format!("Could not read the linked 7z archive: {error}"))?;
            archive
                .files
                .iter()
                .find(|file| file.name() == item.name.as_str())
                .map(|file| file.size)
                .ok_or_else(|| format!("The linked image '{}' is missing from the 7z archive.", item.name))
        }
        ImageEntry::Rar(item) => {
            let archive = unrar::Archive::new(&item.archive_path)
                .open_for_listing()
                .map_err(|error| format!("Could not read the linked RAR archive: {error}"))?;
            let mut archive = archive;
            loop {
                let header = archive
                    .read_header()
                    .map_err(|error| format!("Could not inspect the linked RAR archive: {error}"))?;
                let Some(header) = header else {
                    return Err(format!("The linked image '{}' is missing from the RAR archive.", item.name));
                };
                let name = header.entry().filename.to_string_lossy().to_string();
                if name.as_str() == item.name.as_str() {
                    return Ok(header.entry().unpacked_size);
                }
                archive = header
                    .skip()
                    .map_err(|error| format!("Could not inspect the linked RAR archive: {error}"))?;
            }
        }
    }
}

fn write_bundled_entry<W: Write>(writer: &mut W, entry: &ImageEntry) -> Result<(), String> {
    match entry {
        ImageEntry::File(path) => {
            let mut source = std::fs::File::open(path)
                .map_err(|error| format!("Could not open the image for bundling: {error}"))?;
            copy_limited_image(&mut source, writer)
        }
        ImageEntry::Zip(item) => {
            let file = std::fs::File::open(&item.archive_path)
                .map_err(|error| format!("Could not open the ZIP image for bundling: {error}"))?;
            let mut archive = ZipArchive::new(file)
                .map_err(|error| format!("Could not read the ZIP image for bundling: {error}"))?;
            let mut source = archive
                .by_index(item.entry_index)
                .map_err(|error| format!("Could not open the ZIP image entry: {error}"))?;
            copy_limited_image(&mut source, writer)
        }
        ImageEntry::S7z(_) | ImageEntry::Rar(_) => {
            // The 7z and RAR readers in the existing archive abstraction expose
            // decoded entries as byte vectors. Keep that path, but enforce the
            // same output limit before writing anything into the JVP.
            let bytes = crate::app::workspace::read_archive_entry_for_share(entry)
                .map_err(|error| format!("Could not extract the archive image: {error}"))?;
            if bytes.is_empty() {
                return Err("The current image is empty.".into());
            }
            if bytes.len() as u64 > MAX_BUNDLED_IMAGE_BYTES {
                return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
            }
            writer
                .write_all(&bytes)
                .map_err(|error| format!("Could not write the bundled image: {error}"))
        }
    }
}

fn copy_limited_image<R: Read, W: Write>(source: &mut R, destination: &mut W) -> Result<(), String> {
    let mut limited = source.take(MAX_BUNDLED_IMAGE_BYTES + 1);
    let copied = std::io::copy(&mut limited, destination)
        .map_err(|error| format!("Could not copy the bundled image: {error}"))?;
    if copied == 0 {
        return Err("The current image is empty.".into());
    }
    if copied > MAX_BUNDLED_IMAGE_BYTES {
        return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
    }
    Ok(())
}

fn create_temporary_file(destination: &Path) -> Result<(PathBuf, std::fs::File), String> {
    let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = destination.file_name().unwrap_or_else(|| std::ffi::OsStr::new("preset.jvp"));
    let filename = filename.to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..100_u32 {
        let path = parent.join(format!(
            ".{filename}.tmp-{}-{timestamp}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("Could not create a temporary JVP file: {error}"));
            }
        }
    }
    Err("Could not allocate a temporary filename for the JVP.".into())
}

fn replace_file_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        if !destination.is_file() {
            let _ = fs::remove_file(temporary);
            return Err(format!("The destination is not a regular file: {}", destination.display()));
        }
        let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let filename = destination.file_name().unwrap_or_else(|| std::ffi::OsStr::new("preset.jvp"));
        let filename = filename.to_string_lossy();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup = (0..100_u32)
            .map(|attempt| parent.join(format!(
                ".{filename}.bak-{}-{timestamp}-{attempt}",
                std::process::id()
            )))
            .find(|path| !path.exists());
        let Some(backup) = backup else {
            let _ = fs::remove_file(temporary);
            return Err("Could not allocate a backup filename for the existing JVP.".into());
        };
        if let Err(error) = fs::rename(destination, &backup) {
            let _ = fs::remove_file(temporary);
            return Err(format!("Could not protect the existing JVP before saving: {error}"));
        }
        if let Err(error) = fs::rename(temporary, destination) {
            let restore_error = fs::rename(&backup, destination).err();
            let _ = fs::remove_file(temporary);
            return Err(match restore_error {
                Some(restore_error) => format!(
                    "Could not replace the JVP ({error}); restoring the previous file also failed ({restore_error}). The previous file remains at {}.",
                    backup.display()
                ),
                None => format!("Could not replace the JVP; the previous file was restored: {error}"),
            });
        }
        // The new preset is safely in place. A stale backup is harmless if the
        // OS refuses cleanup, so do not report a successful save as failed.
        let _ = fs::remove_file(backup);
        Ok(())
    } else {
        fs::rename(temporary, destination)
            .map_err(|error| {
                let _ = fs::remove_file(temporary);
                format!("Could not move the completed JVP into place: {error}")
            })
    }
}

fn entry_file_name(entry: &ImageEntry) -> String {
    let raw = match entry {
        ImageEntry::File(path) => path.file_name().and_then(|name| name.to_str()),
        ImageEntry::Zip(item) => Some(item.name.as_str()),
        ImageEntry::S7z(item) => Some(item.name.as_str()),
        ImageEntry::Rar(item) => Some(item.name.as_str()),
    }
    .unwrap_or("image.bin");
    let filename = raw.rsplit(['/', '\\']).next().unwrap_or("image.bin");
    let cleaned: String = filename
        .chars()
        .filter(|character| !character.is_control() && !matches!(*character, '/' | '\\' | ':'))
        .take(180)
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "image.bin".to_owned()
    } else {
        cleaned
    }
}

fn validate_bundle_member_path(path: &str) -> Result<(), String> {
    let normalized = path.replace('\\', "/");
    if !normalized.starts_with("image/")
        || normalized.contains("../")
        || normalized.starts_with('/')
        || normalized.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err("The JVP contains an unsafe bundled image path.".into());
    }
    Ok(())
}

fn write_bundled_image(member_path: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let filename = Path::new(member_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.bin");
    let extension: String = Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("img")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(16)
        .collect();
    let extension = if extension.is_empty() { "img".to_owned() } else { extension };
    let root = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Jpeg Viewer")
        .join("jvp-images");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create the JVP image cache: {error}"))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..100_u32 {
        let path = root.join(format!(
            "image-{}-{timestamp}-{attempt}.{extension}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = fs::remove_file(&path);
                    return Err(format!("Could not materialize the bundled image: {error}"));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("Could not create the extracted image file: {error}"));
            }
        }
    }
    Err("Could not allocate a unique filename for the bundled image.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_sanitization_keeps_values_in_supported_ranges() {
        let mut preset = PopoutPreset {
            opacity_percent: f32::NAN,
            ball_friction: 99.0,
            dvd_mode: true,
            ball_mode: true,
            stay_on_top: true,
            stay_on_bottom: true,
            ..PopoutPreset::default()
        };
        preset.sanitize();
        assert_eq!(preset.opacity_percent, 100.0);
        assert_eq!(preset.ball_friction, 5.0);
        assert!(preset.dvd_mode);
        assert!(!preset.ball_mode);
        assert!(preset.stay_on_top);
        assert!(!preset.stay_on_bottom);
    }

    #[test]
    fn legacy_spin_setting_is_ignored_when_loading_older_jvp_files() {
        // Earlier JVP versions stored a fixed degrees-per-second spin value.
        // Rotation is now derived from throw velocity and collision impulses.
        let preset: PopoutPreset = serde_json::from_str(
            r#"{"ball_mode":true,"ball_rotation_speed":240.0}"#,
        )
        .expect("older JVP preset should remain loadable");
        assert!(preset.ball_mode);
    }

    #[test]
    fn bundle_member_validation_rejects_path_traversal() {
        assert!(validate_bundle_member_path("image/photo.png").is_ok());
        assert!(validate_bundle_member_path("image/../secret.png").is_err());
        assert!(validate_bundle_member_path("../secret.png").is_err());
    }
}
