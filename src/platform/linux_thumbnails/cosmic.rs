//! COSMIC Files-compatible thumbnail generation.
//!
//! COSMIC Files uses the Freedesktop cache layout and external thumbnailer
//! definitions. Cache reuse is handled before this module is reached; this
//! backend invokes matching standard `.thumbnailer` entries when available.

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
