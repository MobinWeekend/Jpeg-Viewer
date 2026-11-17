//! GNOME and GNOME-compatible desktop thumbnail generation.
//!
//! GNOME's thumbnail factory selects executables from the standard
//! `$XDG_DATA_DIRS/thumbnailers/*.thumbnailer` registry. Cinnamon, MATE and
//! Budgie use the same ecosystem of thumbnailer definitions, so they share this
//! backend. JPEG Viewer executes those entries without linking to GTK.

use super::thumbnailer;
use std::path::Path;
use std::time::Instant;

pub(super) fn generate(
    path: &Path,
    uri: &str,
    max_side: u32,
    deadline: Instant,
) -> Option<image::RgbaImage> {
    thumbnailer::generate(path, uri, max_side, deadline)
}
