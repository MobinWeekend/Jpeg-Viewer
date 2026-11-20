//! Optional aggregate counters for application-level I/O.
//!
//! Enable with `JPEG_VIEWER_IO_STATS=1`. Counters intentionally avoid per-file
//! logging so instrumentation does not itself create noisy disk/console work.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();
static SOURCE_READS: AtomicU64 = AtomicU64::new(0);
static SOURCE_BYTES: AtomicU64 = AtomicU64::new(0);
static ARCHIVE_EXTRACTS: AtomicU64 = AtomicU64::new(0);
static ARCHIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static METADATA_QUERIES: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_CACHE_READS: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_CACHE_BYTES: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_CACHE_WRITES: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_TEMP_READS: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_TEMP_BYTES: AtomicU64 = AtomicU64::new(0);
static SETTINGS_SAVES: AtomicU64 = AtomicU64::new(0);
static SETTINGS_BYTES: AtomicU64 = AtomicU64::new(0);
static PRELOAD_CANCELLATIONS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var_os("JPEG_VIEWER_IO_STATS")
            .and_then(|value| value.into_string().ok())
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

#[inline]
pub(crate) fn source_read(bytes: usize) {
    if enabled() {
        SOURCE_READS.fetch_add(1, Ordering::Relaxed);
        SOURCE_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn archive_extraction(bytes: usize) {
    if enabled() {
        ARCHIVE_EXTRACTS.fetch_add(1, Ordering::Relaxed);
        ARCHIVE_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn metadata_query() {
    if enabled() {
        METADATA_QUERIES.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
#[inline]
pub(crate) fn thumbnail_cache_read(bytes: usize) {
    if enabled() {
        THUMBNAIL_CACHE_READS.fetch_add(1, Ordering::Relaxed);
        THUMBNAIL_CACHE_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
#[inline]
pub(crate) fn thumbnail_temp_read(bytes: usize) {
    if enabled() {
        THUMBNAIL_TEMP_READS.fetch_add(1, Ordering::Relaxed);
        THUMBNAIL_TEMP_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
#[inline]
pub(crate) fn thumbnail_cache_write() {
    if enabled() {
        THUMBNAIL_CACHE_WRITES.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn settings_save(bytes: usize) {
    if enabled() {
        SETTINGS_SAVES.fetch_add(1, Ordering::Relaxed);
        SETTINGS_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn preload_cancelled() {
    if enabled() {
        PRELOAD_CANCELLATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn report() {
    if !enabled() {
        return;
    }
    eprintln!(
        "[Jpeg-Viewer I/O] source_reads={} source_bytes={} archive_extractions={} archive_bytes={} metadata_queries={} thumbnail_cache_reads={} thumbnail_cache_bytes={} thumbnail_cache_writes={} thumbnail_temp_reads={} thumbnail_temp_bytes={} settings_saves={} settings_bytes={} preload_cancellations={}",
        SOURCE_READS.load(Ordering::Relaxed),
        SOURCE_BYTES.load(Ordering::Relaxed),
        ARCHIVE_EXTRACTS.load(Ordering::Relaxed),
        ARCHIVE_BYTES.load(Ordering::Relaxed),
        METADATA_QUERIES.load(Ordering::Relaxed),
        THUMBNAIL_CACHE_READS.load(Ordering::Relaxed),
        THUMBNAIL_CACHE_BYTES.load(Ordering::Relaxed),
        THUMBNAIL_CACHE_WRITES.load(Ordering::Relaxed),
        THUMBNAIL_TEMP_READS.load(Ordering::Relaxed),
        THUMBNAIL_TEMP_BYTES.load(Ordering::Relaxed),
        SETTINGS_SAVES.load(Ordering::Relaxed),
        SETTINGS_BYTES.load(Ordering::Relaxed),
        PRELOAD_CANCELLATIONS.load(Ordering::Relaxed),
    );
}
