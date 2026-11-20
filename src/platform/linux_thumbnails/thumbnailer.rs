//! Runner for the Freedesktop/GNOME `.thumbnailer` entry format.
//!
//! The same simple format is consumed by GNOME Desktop's thumbnail factory and
//! by COSMIC Files' external-thumbnailer support. Reading the installed entries
//! lets JPEG Viewer reuse the desktop's installed generators without linking
//! against GTK or GNOME libraries.

use std::collections::HashSet;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::xdg_cache;
use ini::Ini;

#[derive(Debug, Clone)]
struct ThumbnailerEntry {
    exec: String,
    mime_types: Vec<String>,
    try_exec: Option<String>,
}

static ENTRIES: OnceLock<Vec<ThumbnailerEntry>> = OnceLock::new();
static NEXT_OUTPUT_ID: AtomicU64 = AtomicU64::new(0);

fn data_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();

    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("share")));
    if let Some(data_home) = data_home {
        directories.push(data_home.join("thumbnailers"));
    }

    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|path| path.is_absolute())
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]);

    directories.extend(data_dirs.into_iter().map(|path| path.join("thumbnailers")));
    directories
}

fn load_entries() -> Vec<ThumbnailerEntry> {
    let mut entries = Vec::new();
    let mut seen_names = HashSet::new();

    // User entries are checked before system entries, matching XDG data-home
    // precedence. A same-named user file therefore overrides the system one.
    for directory in data_directories() {
        let Ok(read_dir) = std::fs::read_dir(directory) else {
            continue;
        };
        let mut files = read_dir
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("thumbnailer"))
            .collect::<Vec<_>>();
        files.sort();

        for path in files {
            let Some(name) = path.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
                continue;
            };
            if !seen_names.insert(name.clone()) {
                continue;
            }
            let Ok(config) = Ini::load_from_file(&path) else {
                continue;
            };
            let Some(section) = config.section(Some("Thumbnailer Entry")) else {
                continue;
            };
            // A same-named user entry with Hidden=true masks the system entry
            // according to desktop-entry precedence.
            if section
                .get("Hidden")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
            {
                continue;
            }
            let Some(exec) = section.get("Exec").map(str::trim).filter(|value| !value.is_empty()) else {
                continue;
            };
            let Some(mime_types) = section.get("MimeType") else {
                continue;
            };

            let mime_types = mime_types
                .split(';')
                .map(str::trim)
                .filter(|mime| !mime.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if mime_types.is_empty() {
                continue;
            }

            entries.push(ThumbnailerEntry {
                exec: exec.to_owned(),
                mime_types,
                try_exec: section.get("TryExec").map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()),
            });
        }
    }

    entries
}

fn entries() -> &'static [ThumbnailerEntry] {
    ENTRIES.get_or_init(load_entries).as_slice()
}

fn mime_matches(entry: &ThumbnailerEntry, mime_type: &str) -> bool {
    entry.mime_types.iter().any(|supported| {
        if let Some(prefix) = supported.strip_suffix("*") {
            mime_type.starts_with(prefix)
        } else {
            supported == mime_type
        }
    })
}

fn executable_available(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|directory| directory.join(executable).is_file()))
        .unwrap_or(false)
}

fn requested_flavor_size(max_side: u32) -> u32 {
    // The Freedesktop cache has four standardized flavors. For requests beyond
    // xx-large, pass the exact requested bound to compatible .thumbnailer
    // programs instead of silently capping the request at 1024px.
    match max_side {
        0..=128 => 128,
        129..=256 => 256,
        257..=512 => 512,
        513..=1024 => 1024,
        _ => max_side.max(1),
    }
}

fn expand_token(
    token: &str,
    input_path: &str,
    input_uri: &str,
    output_path: &str,
    size: u32,
) -> Option<(String, bool)> {
    let mut result = String::with_capacity(token.len() + 32);
    let mut chars = token.chars();
    let mut has_output = false;

    while let Some(character) = chars.next() {
        if character != '%' {
            result.push(character);
            continue;
        }

        match chars.next()? {
            '%' => result.push('%'),
            'u' => result.push_str(input_uri),
            'i' => result.push_str(input_path),
            'o' => {
                result.push_str(output_path);
                has_output = true;
            }
            's' => result.push_str(&size.to_string()),
            // Unknown field codes are invalid for this thumbnailer format.
            _ => return None,
        }
    }

    Some((result, has_output))
}

struct TemporaryOutput {
    directory: PathBuf,
    file: PathBuf,
}

impl TemporaryOutput {
    fn new() -> Option<Self> {
        let id = NEXT_OUTPUT_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "jpeg-viewer-thumbnail-{}-{id}",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700).create(&directory).ok()?;
        Some(Self {
            file: directory.join("thumbnail.png"),
            directory,
        })
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn run_with_timeout(command: &mut Command, timeout: Duration) -> bool {
    let Ok(mut child) = command.stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Run an installed thumbnailer whose declared MIME types match this file.
/// The process writes to a private temporary PNG. On success, JPEG Viewer
/// publishes a spec-compliant, atomically-renamed entry in the shared XDG cache.
pub(super) fn generate(
    path: &Path,
    uri: &str,
    max_side: u32,
    overall_deadline: Instant,
) -> Option<image::RgbaImage> {
    let mime_type = infer::get_from_path(path).ok().flatten()?.mime_type().to_owned();
    let input_path = path.to_str()?;
    let (source_mtime, source_size) = xdg_cache::source_stamp(path)?;
    let size = requested_flavor_size(max_side);

    for entry in entries().iter().filter(|entry| mime_matches(entry, &mime_type)) {
        if Instant::now() >= overall_deadline {
            break;
        }
        if entry.try_exec.as_deref().is_some_and(|program| !executable_available(program)) {
            continue;
        }

        let Some(tokens) = shlex::split(&entry.exec) else {
            continue;
        };
        let Some(program) = tokens.first() else {
            continue;
        };
        if !executable_available(program) {
            continue;
        }

        let Some(output) = TemporaryOutput::new() else {
            return None;
        };
        let Some(output_path) = output.file.to_str() else {
            continue;
        };

        let mut arguments = Vec::with_capacity(tokens.len().saturating_sub(1));
        let mut has_output = false;
        let mut valid = true;
        for token in tokens.iter().skip(1) {
            let Some((expanded, token_has_output)) = expand_token(token, input_path, uri, output_path, size) else {
                valid = false;
                break;
            };
            has_output |= token_has_output;
            arguments.push(expanded);
        }
        // The thumbnailer format requires an output destination. Avoid running
        // malformed entries or interpreting them through a shell.
        if !valid || !has_output {
            continue;
        }

        let remaining = overall_deadline.saturating_duration_since(Instant::now());
        let timeout = remaining.min(Duration::from_secs(3));
        if timeout.is_zero() {
            break;
        }

        let mut command = Command::new(program);
        command.args(arguments);
        if !run_with_timeout(&mut command, timeout) {
            continue;
        }

        crate::io_metrics::metadata_query();
        let Ok(metadata) = std::fs::metadata(&output.file) else {
            continue;
        };
        // Thumbnailers should return small previews. Refuse unexpectedly large
        // outputs rather than allowing a plugin to turn a thumbnail request into
        // an unbounded image decode.
        if metadata.len() == 0 || metadata.len() > 256 * 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&output.file) else {
            continue;
        };
        crate::io_metrics::thumbnail_temp_read(bytes.len());
        let Ok(decoded) = image::load_from_memory(&bytes) else {
            continue;
        };
        let (width, height) = (decoded.width(), decoded.height());
        let Some(pixel_count) = (width as u64).checked_mul(height as u64) else {
            continue;
        };
        // Third-party thumbnailers are not trusted to return bounded data.
        // Keep a generous allocation guard while allowing large Masonry tiles
        // and requests above the old 512/1024px application ceilings.
        if width == 0 || height == 0 || pixel_count > 67_108_864 {
            continue;
        }
        let rgba = decoded.to_rgba8();
        if rgba.width().max(rgba.height()) >= max_side {
            // Do not return a preview generated from a file that changed while
            // the external thumbnailer was running.
            if xdg_cache::source_stamp(path) != Some((source_mtime, source_size)) {
                continue;
            }
            xdg_cache::store_generated(path, uri, max_side, source_mtime, source_size, &rgba);
            return Some(rgba);
        }

        // Try another matching entry if this one only supports a smaller tier.
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{ThumbnailerEntry, expand_token, mime_matches, requested_flavor_size};

    #[test]
    fn expands_standard_thumbnailer_tokens() {
        let (expanded, has_output) = expand_token(
            "--size=%s:%u:%i:%o:%%",
            "/tmp/a file.jpg",
            "file:///tmp/a%20file.jpg",
            "/tmp/out.png",
            512,
        ).expect("valid thumbnailer entry");
        assert_eq!(expanded, "--size=512:file:///tmp/a%20file.jpg:/tmp/a file.jpg:/tmp/out.png:%");
        assert!(has_output);
    }

    #[test]
    fn mime_matching_supports_exact_and_wildcard_types() {
        let entry = ThumbnailerEntry {
            exec: "test %u %o".to_owned(),
            mime_types: vec!["image/*".to_owned(), "application/pdf".to_owned()],
            try_exec: None,
        };
        assert!(mime_matches(&entry, "image/jpeg"));
        assert!(mime_matches(&entry, "application/pdf"));
        assert!(!mime_matches(&entry, "video/mp4"));
    }

    #[test]
    fn maps_requested_dimensions_to_xdg_flavors() {
        assert_eq!(requested_flavor_size(64), 128);
        assert_eq!(requested_flavor_size(220), 256);
        assert_eq!(requested_flavor_size(512), 512);
        assert_eq!(requested_flavor_size(900), 1024);
        assert_eq!(requested_flavor_size(1024), 1024);
        assert_eq!(requested_flavor_size(1200), 1200);
        assert_eq!(requested_flavor_size(2048), 2048);
    }
}
