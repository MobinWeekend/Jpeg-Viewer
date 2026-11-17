//! Xfce Tumbler backend using its standard Thumbnailer1 D-Bus interface.
//!
//! Tumbler queues asynchronous thumbnail jobs and writes the results to the
//! shared Freedesktop cache. We poll that cache for a short, bounded interval on
//! the caller's worker thread. Missing gdbus/service/plugin is a normal fallback.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::xdg_cache;

fn gvariant_string_array(value: &str) -> String {
    // gdbus parses its arguments as GVariant expressions. Escape characters
    // that could otherwise terminate the quoted string inside an unusual file
    // URI; this is argument encoding, not shell escaping.
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("['{escaped}']")
}

pub(super) fn request(
    path: &Path,
    uri: &str,
    max_side: u32,
    overall_deadline: Instant,
) -> Option<image::RgbaImage> {
    let mime_type = infer::get_from_path(path).ok().flatten()?.mime_type().to_owned();
    let uri_argument = gvariant_string_array(uri);
    let mime_argument = gvariant_string_array(&mime_type);
    let flavor = match max_side {
        0..=128 => "normal",
        129..=256 => "large",
        257..=512 => "x-large",
        _ => "xx-large",
    };

    let queue_timeout = overall_deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(2));
    if queue_timeout.is_zero() {
        return None;
    }

    let mut child = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.thumbnails.Thumbnailer1",
            "--object-path",
            "/org/freedesktop/thumbnails/Thumbnailer1",
            "--method",
            "org.freedesktop.thumbnails.Thumbnailer1.Queue",
            uri_argument.as_str(),
            mime_argument.as_str(),
            flavor,
            "default",
            "0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + queue_timeout;
    let queued = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };

    if !queued {
        return None;
    }

    let cache_wait = overall_deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(2500));
    if cache_wait.is_zero() {
        return None;
    }
    xdg_cache::wait_for_cached(path, uri, max_side, cache_wait)
}

#[cfg(test)]
mod tests {
    use super::gvariant_string_array;

    #[test]
    fn escapes_quotes_and_backslashes_in_file_uris() {
        assert_eq!(gvariant_string_array("file:///tmp/a'b\\c.jpg"), "['file:///tmp/a\\'b\\\\c.jpg']");
    }
}
