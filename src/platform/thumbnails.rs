//! Platform-native thumbnail lookup and generation.
//!
//! This module is deliberately best-effort. If the desktop shell has no cached
//! thumbnail or cannot generate one, callers fall back to JPEG Viewer's own
//! embedded-preview and decoder pipeline.

use std::path::Path;

/// Per-thread platform state for the shared thumbnail worker pool. On Windows,
/// each worker owns a COM apartment for its entire lifetime instead of paying
/// the initialization cost for every file.
pub(crate) struct ThumbnailWorkerContext {
    #[cfg(target_os = "windows")]
    com_initialized: bool,
}

impl ThumbnailWorkerContext {
    pub(crate) fn initialize() -> Self {
        #[cfg(target_os = "windows")]
        let com_initialized = windows_shell::initialize_worker_com();

        Self {
            #[cfg(target_os = "windows")]
            com_initialized,
        }
    }
}

impl Drop for ThumbnailWorkerContext {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        windows_shell::uninitialize_worker_com(self.com_initialized);
    }
}

/// Give a Windows STA worker a chance to dispatch COM/window messages while it
/// is idle. Other platforms do not need a worker message pump here.
pub(crate) fn pump_worker_messages() {
    #[cfg(target_os = "windows")]
    windows_shell::pump_worker_messages();
}

/// Whether verbose platform-thumbnail diagnostics were explicitly enabled.
pub(crate) fn diagnostics_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows_shell::diagnostics_enabled();
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Load a native thumbnail if one is cached, otherwise ask the desktop shell to
/// generate one. This function is called only from the shared thumbnail worker
/// pool, never from egui's UI thread.
pub(crate) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
    if !path.is_file() {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        return windows_shell::load_or_request(path, max_side);
    }

    #[cfg(target_os = "linux")]
    {
        return crate::platform::linux_thumbnails::load_or_request(path, max_side);
    }

    #[cfg(target_os = "macos")]
    {
        return quick_look::load_or_request(path, max_side);
    }

    #[allow(unreachable_code)]
    None
}

/// Fast first-pass native preview request for Masonry placeholders. Windows
/// performs one cache-first / generation pass here, but deliberately skips the
/// second larger-size retry; the later high-resolution stage is responsible for
/// asking Shell again at a closer size when the first preview is too small.
pub(crate) fn load_or_request_placeholder(
    path: &Path,
    max_side: u32,
) -> Option<image::RgbaImage> {
    if !path.is_file() {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        return windows_shell::load_or_request_fast(path, max_side);
    }

    #[cfg(target_os = "linux")]
    {
        return crate::platform::linux_thumbnails::load_or_request(path, max_side);
    }

    #[cfg(target_os = "macos")]
    {
        return quick_look::load_or_request(path, max_side);
    }

    #[allow(unreachable_code)]
    None
}

/// Ask the native platform for source image dimensions without decoding the
/// image. This is a metadata fallback for formats that the generic header
/// reader does not understand. The caller runs it on the thumbnail worker.
pub(crate) fn source_dimensions(path: &Path) -> Option<(u32, u32)> {
    #[cfg(target_os = "windows")]
    {
        return windows_shell::source_dimensions(path);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "macos")]
mod quick_look {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub(super) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "jpeg-viewer-quicklook-{}-{id}", std::process::id()
        ));
        std::fs::create_dir_all(&directory).ok()?;
        let thumbnail_size = max_side.max(1).to_string();
        let status = Command::new("qlmanage")
            .args(["-t", "-s", thumbnail_size.as_str(), "-o"])
            .arg(&directory)
            .arg(path)
            .output()
            .ok()
            .is_some_and(|output| output.status.success());

        let result = if status {
            let generated = path.file_name().map(|name| directory.join(format!("{}.png", name.to_string_lossy())));
            let fallback = path.file_stem().map(|stem| directory.join(format!("{}.png", stem.to_string_lossy())));
            generated
                .and_then(|generated| image::open(&generated).ok().or_else(|| {
                    fallback.as_deref().and_then(|fallback| image::open(fallback).ok())
                }))
                .map(|image| image.to_rgba8())
        } else {
            None
        };
        let _ = std::fs::remove_dir_all(directory);
        result
    }
}

#[cfg(target_os = "windows")]
mod windows_shell {
    use super::*;
    use std::cell::Cell;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
    };
    use windows::Win32::Storage::EnhancedStorage::{
        PKEY_Image_HorizontalSize, PKEY_Image_VerticalSize,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        StructuredStorage::{PropVariantClear, PropVariantToUInt32},
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_NOREMOVE, PM_REMOVE,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
        SIIGBF_INCACHEONLY, SIIGBF_THUMBNAILONLY,
        PropertiesSystem::{IPropertyStore, SHGetPropertyStoreFromParsingName, GPS_DEFAULT},
    };
    use windows::core::PCWSTR;

    // Prevent a Shell extension from causing an unbounded RGBA allocation if
    // it returns a bitmap much larger than requested with BIGGERSIZEOK. If a
    // result exceeds these limits, request the same thumbnail again with the
    // Shell's normal resize-to-fit behavior.
    const MAX_SHELL_BITMAP_SIDE: u32 = 16_384;
    const MAX_SHELL_BITMAP_PIXELS: u64 = 16_777_216; // 64 MiB RGBA

    thread_local! {
        static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn diagnostics_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("JPEG_VIEWER_THUMBNAIL_DIAGNOSTICS")
                .ok()
                .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        })
    }

    fn diagnostic(path: &Path, stage: &str, detail: impl std::fmt::Display) {
        if diagnostics_enabled() {
            eprintln!(
                "[thumbnail:windows] {}: {stage}: {detail}",
                path.display(),
            );
        }
    }

    /// Initialize this worker as an STA and create its Windows message queue.
    /// Shell extensions are not uniformly free-threaded; retaining STA avoids
    /// forcing apartment-threaded Shell components through an MTA proxy. The
    /// scheduler periodically pumps this queue while the worker is idle.
    pub(super) fn initialize_worker_com() -> bool {
        unsafe {
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        }

        // windows 0.62 exposes CoInitializeEx as a raw HRESULT, not
        // Result<()>. S_FALSE is also a successful initialization and must be
        // balanced with CoUninitialize just like S_OK.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_ok() {
            COM_INITIALIZED.with(|initialized| initialized.set(true));
            true
        } else {
            if diagnostics_enabled() {
                eprintln!(
                    "[thumbnail:windows] worker COM initialization failed (HRESULT 0x{:08X})",
                    result.0 as u32,
                );
            }
            false
        }
    }

    pub(super) fn uninitialize_worker_com(initialized: bool) {
        if initialized {
            COM_INITIALIZED.with(|ready| ready.set(false));
            unsafe { CoUninitialize() };
        }
    }

    pub(super) fn pump_worker_messages() {
        if !COM_INITIALIZED.with(Cell::get) {
            return;
        }

        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                let _ = DispatchMessageW(&message);
            }
        }
    }

    fn bitmap_to_rgba(
        path: &Path,
        bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    ) -> Option<image::RgbaImage> {
        unsafe {
            let mut description = BITMAP::default();
            let read = GetObjectW(
                bitmap.into(),
                size_of::<BITMAP>() as i32,
                Some((&mut description as *mut BITMAP).cast()),
            );
            if read == 0 || description.bmWidth <= 0 || description.bmHeight == 0 {
                diagnostic(
                    path,
                    "GetObjectW",
                    "Shell returned an invalid bitmap description",
                );
                return None;
            }
            let width = description.bmWidth as u32;
            let height = description.bmHeight.unsigned_abs();
            let pixels = u64::from(width).checked_mul(u64::from(height))?;
            if width > MAX_SHELL_BITMAP_SIDE
                || height > MAX_SHELL_BITMAP_SIDE
                || pixels > MAX_SHELL_BITMAP_PIXELS
            {
                diagnostic(
                    path,
                    "bitmap dimensions",
                    format!("rejected oversized bitmap {width}x{height}"),
                );
                return None;
            }
            let byte_count = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
            let mut pixels = vec![0u8; byte_count];
            let mut info = BITMAPINFO::default();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: byte_count.min(u32::MAX as usize) as u32,
                ..Default::default()
            };
            let dc = GetDC(None);
            if dc.0.is_null() {
                diagnostic(path, "GetDC", "could not acquire a GDI device context");
                return None;
            }
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            let _ = ReleaseDC(None, dc);
            if lines != height as i32 {
                diagnostic(
                    path,
                    "GetDIBits",
                    format!("incomplete bitmap transfer: received {lines} of {height} scanlines"),
                );
                return None;
            }
            // BI_RGB 32-bit DIBs are BGRA. Preserve alpha supplied by the Shell;
            // some handlers leave the whole fourth byte zero because their DIB
            // does not carry alpha, so only that all-zero case is made opaque.
            // Some providers expose premultiplied channels. Detect that layout
            // conservatively (all partially transparent RGB channels <= alpha)
            // and convert it to straight alpha for egui's unmultiplied textures.
            let has_alpha = pixels.chunks_exact(4).any(|pixel| pixel[3] != 0);
            let has_partial_alpha = pixels.chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255);
            let looks_premultiplied = has_alpha && has_partial_alpha && pixels.chunks_exact(4)
                .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
                .all(|pixel| pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3]);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if !has_alpha {
                    pixel[3] = 255;
                } else if pixel[3] == 0 {
                    pixel[0] = 0;
                    pixel[1] = 0;
                    pixel[2] = 0;
                } else if looks_premultiplied && pixel[3] < 255 {
                    let alpha = pixel[3] as u16;
                    for channel in &mut pixel[..3] {
                        *channel = ((*channel as u16 * 255 + alpha / 2) / alpha)
                            .min(255) as u8;
                    }
                }
            }
            image::RgbaImage::from_raw(width, height, pixels)
        }
    }

    fn request_bitmap(
        path: &Path,
        max_side: u32,
        cache_only: bool,
        allow_larger_cached_bitmap: bool,
    ) -> Option<image::RgbaImage> {
        let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let factory: IShellItemImageFactory = unsafe {
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
        }
        .map_err(|error| {
            diagnostic(
                path,
                "SHCreateItemFromParsingName",
                format!("HRESULT 0x{:08X}: {error}", error.code().0 as u32),
            );
            error
        })
        .ok()?;
        let mut flags = SIIGBF_THUMBNAILONLY;
        if allow_larger_cached_bitmap {
            // BIGGERSIZEOK lets Shell return a larger cached flavor (e.g. 1024
            // px for a 768 px request) so Jpeg-Viewer can do the resampling.
            flags |= SIIGBF_BIGGERSIZEOK;
        }
        if cache_only {
            flags |= SIIGBF_INCACHEONLY;
        }
        let bitmap = unsafe {
            factory.GetImage(
                SIZE {
                    cx: max_side.max(1).min(i32::MAX as u32) as i32,
                    cy: max_side.max(1).min(i32::MAX as u32) as i32,
                },
                flags,
            )
        }
        .map_err(|error| {
            let mode = if cache_only { "GetImage(cache-only)" } else { "GetImage(generate)" };
            diagnostic(
                path,
                mode,
                format!("HRESULT 0x{:08X}: {error}", error.code().0 as u32),
            );
            error
        })
        .ok()?;
        let result = bitmap_to_rgba(path, bitmap);
        unsafe { let _ = DeleteObject(bitmap.into()); }
        if result.is_none() {
            diagnostic(path, "bitmap conversion", "Shell bitmap could not be converted to a complete RGBA image");
        }
        result
    }

    fn get_image_dimension(
        path: &Path,
        store: &IPropertyStore,
        key: &windows::Win32::Foundation::PROPERTYKEY,
        property_name: &str,
    ) -> Option<u32> {
        let mut value = unsafe { store.GetValue(key) }
            .map_err(|error| {
                diagnostic(
                    path,
                    property_name,
                    format!("IPropertyStore::GetValue failed (HRESULT 0x{:08X}): {error}", error.code().0 as u32),
                );
                error
            })
            .ok()?;
        let dimension = unsafe { PropVariantToUInt32(&value) }
            .map_err(|error| {
                diagnostic(
                    path,
                    property_name,
                    format!("PROPVARIANT conversion failed (HRESULT 0x{:08X}): {error}", error.code().0 as u32),
                );
                error
            })
            .ok();
        // IPropertyStore::GetValue returns an owning PROPVARIANT. Clear it
        // even when conversion fails, so future property handlers that return
        // dynamically allocated values do not leak COM memory.
        let _ = unsafe { PropVariantClear(&mut value) };
        let dimension = dimension?;
        if dimension == 0 {
            diagnostic(path, property_name, "Windows returned a zero dimension");
            return None;
        }
        Some(dimension)
    }

    fn source_dimensions_inner(path: &Path) -> Option<(u32, u32)> {
        let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let store: IPropertyStore = unsafe {
            SHGetPropertyStoreFromParsingName::<_, _, IPropertyStore>(
                PCWSTR(wide.as_ptr()),
                None,
                GPS_DEFAULT,
            )
        }
        .map_err(|error| {
            diagnostic(
                path,
                "SHGetPropertyStoreFromParsingName",
                format!("HRESULT 0x{:08X}: {error}", error.code().0 as u32),
            );
            error
        })
        .ok()?;
        let width = get_image_dimension(
            path,
            &store,
            &PKEY_Image_HorizontalSize,
            "Image_HorizontalSize",
        )?;
        let height = get_image_dimension(
            path,
            &store,
            &PKEY_Image_VerticalSize,
            "Image_VerticalSize",
        )?;
        Some((width, height))
    }

    pub(super) fn source_dimensions(path: &Path) -> Option<(u32, u32)> {
        if !COM_INITIALIZED.with(Cell::get) {
            diagnostic(
                path,
                "source dimensions",
                "thumbnail worker has no initialized COM apartment",
            );
            return None;
        }
        source_dimensions_inner(path)
    }

    fn request_at_size(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        // Preserve cache-first / generate-second order. BIGGERSIZEOK lets Shell
        // reuse a larger cached flavor rather than needlessly scaling it down.
        let cached = request_bitmap(path, max_side, true, true)
            .or_else(|| request_bitmap(path, max_side, true, false));
        cached.or_else(|| {
            request_bitmap(path, max_side, false, true)
                .or_else(|| request_bitmap(path, max_side, false, false))
        })
    }

    pub(super) fn load_or_request_fast(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        if !COM_INITIALIZED.with(Cell::get) {
            diagnostic(
                path,
                "placeholder thumbnail request",
                "thumbnail worker has no initialized COM apartment",
            );
            return None;
        }
        request_at_size(path, max_side.max(1))
    }

    pub(super) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        if !COM_INITIALIZED.with(Cell::get) {
            diagnostic(
                path,
                "thumbnail request",
                "thumbnail worker has no initialized COM apartment",
            );
            return None;
        }

        let max_side = max_side.max(1);
        let first = request_at_size(path, max_side);
        let first_resolution = first.as_ref().map(|image| image.width().max(image.height()));
        let native_floor = max_side.saturating_mul(3).div_ceil(4);
        if first_resolution.is_some_and(|resolution| resolution >= native_floor) {
            return first;
        }

        // Some Shell handlers/cache flavors return a much smaller thumbnail
        // than requested. Ask the OS once more at a larger size before forcing
        // JPEG Viewer's own source/decoder fallback. This commonly moves a
        // 256px response toward the 1024px Windows thumbnail flavor. Keep the
        // retry bounded to avoid pathological allocation requests.
        let retry_side = max_side.saturating_mul(2).min(4096);
        if retry_side <= max_side {
            return first;
        }
        let upgraded = request_at_size(path, retry_side);
        match (first, upgraded) {
            (Some(first), Some(upgraded)) => {
                if upgraded.width().max(upgraded.height()) > first.width().max(first.height()) {
                    Some(upgraded)
                } else {
                    Some(first)
                }
            }
            (Some(first), None) => Some(first),
            (None, upgraded) => upgraded,
        }
    }
}
