//! Linux desktop thumbnail integration.
//!
//! The implementation deliberately has no hard dependency on GTK, Qt, KDE
//! Frameworks, or a particular desktop environment. It first reuses the shared
//! Freedesktop thumbnail cache, then asks an available desktop thumbnail service
//! or installed XDG thumbnailer plugin to generate a preview. The caller keeps
//! JPEG Viewer's own decoder as the final fallback.

mod cosmic;
mod desktop_detection;
mod gnome;
mod kde;
mod thumbnailer;
mod tumbler;
mod xdg_cache;

pub(crate) fn set_publish_enabled(enabled: bool) {
    xdg_cache::set_publish_enabled(enabled);
}

use std::fs::File;
use std::path::Path;
use std::time::{Duration, Instant};

/// Load an existing desktop-generated thumbnail or request one from the active
/// desktop's available thumbnail infrastructure. `max_side` is the exact
/// physical-pixel bound requested by the UI; individual desktop APIs may map it
/// to their own supported cache flavor or accept it as an arbitrary size.
///
/// This function is called from the application's thumbnail worker pool, never
/// from egui's UI thread. Failures are intentionally silent and fall back to the
/// normal JPEG Viewer decoder path.
pub(crate) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
    if !path.is_file() {
        return None;
    }

    // The Freedesktop thumbnail specification requires applications to verify
    // that the source file is readable before exposing a cached preview. This
    // also prevents a stale thumbnail from bypassing source-file permissions.
    let _source = File::open(path).ok()?;
    let uri = xdg_cache::file_uri(path)?;

    if let Some(cached) = xdg_cache::read_cached(path, &uri, max_side) {
        return Some(cached);
    }

    // Keep all optional desktop-service/plugin attempts within one shared
    // budget, so a missing or broken native service cannot occupy a thumbnail
    // worker indefinitely or stack multiple independent timeouts.
    let generation_deadline = Instant::now() + Duration::from_secs(5);

    match desktop_detection::detect() {
        desktop_detection::Desktop::Xfce => {
            // Xfce's Tumbler service implements Thumbnailer1. It queues work in
            // the desktop's thumbnailer daemon and writes into the shared XDG
            // cache, which we poll briefly on this worker thread.
            if let Some(image) = tumbler::request(path, &uri, max_side, generation_deadline) {
                return Some(image);
            }
            thumbnailer::generate(path, &uri, max_side, generation_deadline)
        }
        desktop_detection::Desktop::GnomeCompatible => {
            gnome::generate(path, &uri, max_side, generation_deadline)
        }
        desktop_detection::Desktop::Kde => kde::generate(path, &uri, max_side, generation_deadline),
        desktop_detection::Desktop::Cosmic => {
            cosmic::generate(path, &uri, max_side, generation_deadline)
        }
        desktop_detection::Desktop::Other => {
            // The .thumbnailer format is desktop-independent. This also gives
            // useful behavior on less common desktops and window managers.
            thumbnailer::generate(path, &uri, max_side, generation_deadline)
        }
    }
}
