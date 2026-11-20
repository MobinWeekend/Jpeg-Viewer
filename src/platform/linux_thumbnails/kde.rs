//! KDE Plasma thumbnail integration.
//!
//! Use KDE's `kioclient` thumbnail protocol for the standard 128px preview.
//! Larger previews are obtained from the shared Freedesktop cache or installed
//! XDG `.thumbnailer` plugins. This keeps the integration Rust-only and avoids
//! a build-time or runtime dependency on Qt/KDE development libraries.

use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{thumbnailer, xdg_cache};

static NEXT_OUTPUT_ID: AtomicU64 = AtomicU64::new(0);
const KIO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OUTPUT_PIXELS: u64 = 67_108_864;

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn create() -> Option<(Self, std::fs::File)> {
        let id = NEXT_OUTPUT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jpeg-viewer-kio-thumbnail-{}-{id}.png",
            std::process::id()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .ok()?;
        Some((Self(path), file))
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn decode_output(path: &Path) -> Option<image::RgbaImage> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_OUTPUT_BYTES {
        return None;
    }
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    let pixel_count = (width as u64).checked_mul(height as u64)?;
    if width == 0 || height == 0 || pixel_count > MAX_OUTPUT_PIXELS {
        return None;
    }
    image::open(path).ok().map(|decoded| decoded.to_rgba8())
}

fn executable_available(executable: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|directory| directory.join(executable).is_file())
        })
        .unwrap_or(false)
}

fn thumbnail_url(path: &Path) -> Option<String> {
    // KIO's worker expects an absolute local path in the URL path component.
    // Build a properly escaped file URI first, then change only its scheme;
    // this keeps spaces, '#', '?', and non-ASCII filenames safe.
    let canonical = path.canonicalize().ok()?;
    let file_uri = url::Url::from_file_path(canonical).ok()?.to_string();
    Some(format!("thumbnail:{}", file_uri.strip_prefix("file:")?))
}

fn request_with_kioclient(
    path: &Path,
    uri: &str,
    max_side: u32,
    overall_deadline: Instant,
) -> Option<image::RgbaImage> {
    // KIO's direct `thumbnail:` mode defaults to 128x128 when no size metadata
    // is supplied. Do not return an undersized image for larger Masonry tiles.
    if max_side > 128 {
        return None;
    }

    let thumbnail_url = thumbnail_url(path)?;
    let executable = ["kioclient6", "kioclient5", "kioclient"]
        .into_iter()
        .find(|candidate| executable_available(candidate))?;
    let (source_mtime, source_size) = xdg_cache::source_stamp(path)?;
    let (temporary, output_file) = TemporaryOutput::create()?;

    // Redirect stdout to a private file rather than a pipe. This avoids a
    // deadlock if the PNG fills the pipe before the parent polls child status.
    let process_timeout = overall_deadline
        .saturating_duration_since(Instant::now())
        .min(KIO_TIMEOUT);
    if process_timeout.is_zero() {
        return None;
    }

    let mut child = Command::new(executable)
        .arg("cat")
        .arg(&thumbnail_url)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + process_timeout;
    let successful = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    if !successful {
        return None;
    }

    let rgba = decode_output(&temporary.0)?;
    if xdg_cache::source_stamp(path) != Some((source_mtime, source_size)) {
        return None;
    }
    // KIO's 128px direct result is useful to other XDG-aware applications too.
    // Cache writes include source URI/mtime metadata and use an atomic rename.
    xdg_cache::store_generated(path, uri, 128, source_mtime, source_size, &rgba);

    Some(rgba)
}

pub(super) fn generate(
    path: &Path,
    uri: &str,
    max_side: u32,
    deadline: Instant,
) -> Option<image::RgbaImage> {
    request_with_kioclient(path, uri, max_side, deadline)
        .or_else(|| thumbnailer::generate(path, uri, max_side, deadline))
}

#[cfg(test)]
mod tests {
    use super::thumbnail_url;
    use std::path::Path;

    #[test]
    fn thumbnail_url_uses_thumbnail_scheme_and_escapes_special_characters() {
        let file = std::env::temp_dir().join(format!(
            "jpeg viewer # thumbnail {}.jpg",
            std::process::id()
        ));
        std::fs::write(&file, b"test").expect("create test file");
        let url = thumbnail_url(Path::new(&file)).expect("make thumbnail URL");
        assert!(url.starts_with("thumbnail:///"));
        assert!(url.contains("%20"));
        assert!(url.contains("%23"));
        let _ = std::fs::remove_file(file);
    }
}
