//! Platform-native thumbnail lookup and generation.
//!
//! This module is deliberately best-effort. If the desktop shell has no cached
//! thumbnail or cannot generate one, callers fall back to JPEG Viewer's own
//! embedded-preview and decoder pipeline.

use std::path::Path;

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
        return freedesktop::load_or_request(path, max_side);
    }

    #[cfg(target_os = "macos")]
    {
        return quick_look::load_or_request(path, max_side);
    }

    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "linux")]
mod freedesktop {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, UNIX_EPOCH};

    fn cache_root() -> Option<std::path::PathBuf> {
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".cache")))
            .map(|root| root.join("thumbnails"))
    }

    fn file_uri(path: &Path) -> Option<String> {
        let absolute = path.canonicalize().ok()?;
        let raw = absolute.to_string_lossy();
        let mut uri = String::from("file://");
        for byte in raw.as_bytes() {
            match *byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                    uri.push(*byte as char);
                }
                byte => uri.push_str(&format!("%{byte:02X}")),
            }
        }
        Some(uri)
    }

    fn expected_mtime(path: &Path) -> Option<u64> {
        path.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|time| time.as_secs())
    }

    /// Read the PNG tEXt chunks and verify the URI/mtime fields required by the
    /// Freedesktop thumbnail specification. A stale OS cache entry is ignored.
    fn valid_thumbnail_metadata(bytes: &[u8], uri: &str, mtime: u64) -> bool {
        const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
        if !bytes.starts_with(PNG_SIGNATURE) {
            return false;
        }

        let mut offset = 8usize;
        let mut found_uri: Option<&str> = None;
        let mut found_mtime: Option<&str> = None;
        while offset.saturating_add(12) <= bytes.len() {
            let length = u32::from_be_bytes([
                bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
            ]) as usize;
            let Some(chunk_end) = offset.checked_add(12).and_then(|value| value.checked_add(length)) else {
                return false;
            };
            if chunk_end > bytes.len() {
                return false;
            }
            let kind = &bytes[offset + 4..offset + 8];
            let data = &bytes[offset + 8..offset + 8 + length];
            if kind == b"tEXt" {
                if let Some(separator) = data.iter().position(|byte| *byte == 0) {
                    let key = std::str::from_utf8(&data[..separator]).ok();
                    let value = std::str::from_utf8(&data[separator + 1..]).ok();
                    match (key, value) {
                        (Some("Thumb::URI"), Some(value)) => found_uri = Some(value),
                        (Some("Thumb::MTime"), Some(value)) => found_mtime = Some(value),
                        _ => {}
                    }
                }
            }
            offset = chunk_end;
            if kind == b"IEND" {
                break;
            }
        }
        found_uri == Some(uri) && found_mtime.and_then(|value| value.parse::<u64>().ok()) == Some(mtime)
    }

    fn cached_paths(path: &Path, uri: &str) -> Vec<std::path::PathBuf> {
        let Some(root) = cache_root() else { return Vec::new(); };
        let Some(mtime) = expected_mtime(path) else { return Vec::new(); };
        let digest = format!("{:x}", md5::compute(uri.as_bytes()));
        let mut candidates = Vec::new();
        // Check the largest available OS thumbnail first. It can be downscaled
        // for smaller Masonry/sidebar sizes without asking the shell again.
        // A larger OS cache entry can always be downscaled, so inspect every
        // standard flavor from largest to smallest regardless of the requested
        // output size.
        let sizes: &[&str] = &["xx-large", "x-large", "large", "normal"];
        for size in sizes {
            let candidate = root.join(size).join(format!("{digest}.png"));
            if let Ok(bytes) = std::fs::read(&candidate) {
                if valid_thumbnail_metadata(&bytes, uri, mtime) {
                    candidates.push(candidate);
                }
            }
        }
        candidates
    }

    fn read_cached(path: &Path, uri: &str, requested_side: u32) -> Option<image::RgbaImage> {
        for candidate in cached_paths(path, uri) {
            if let Ok(image) = image::open(candidate) {
                let image = image.to_rgba8();
                // A smaller cached flavor is not sufficient for a larger Masonry
                // request. Ask the desktop service for the requested flavor, then
                // let the caller use the application's decoder if it is unavailable.
                if image.width().max(image.height()) >= requested_side {
                    return Some(image);
                }
            }
        }
        None
    }

    fn request_from_desktop(path: &Path, uri: &str, max_side: u32) -> bool {
        // Ask the session's Freedesktop Thumbnailer service. If the service is
        // absent (common on minimal desktops), the normal JPEG Viewer decoder is
        // still used by the caller. The 512px Masonry tier maps to x-large in the
        // Freedesktop flavor names; large is limited to 256px.
        let Some(mime) = infer::get_from_path(path)
            .ok()
            .flatten()
            .map(|kind| kind.mime_type().to_owned())
        else {
            return false;
        };
        let uri_arg = format!("['{uri}']");
        let mime_arg = format!("['{mime}']");
        let flavor = if max_side >= 512 {
            "x-large"
        } else if max_side >= 256 {
            "large"
        } else {
            "normal"
        };
        Command::new("gdbus")
            .args([
                "call", "--session",
                "--dest", "org.freedesktop.thumbnails.Thumbnailer1",
                "--object-path", "/org/freedesktop/thumbnails/Thumbnailer1",
                "--method", "org.freedesktop.thumbnails.Thumbnailer1.Queue",
                uri_arg.as_str(), mime_arg.as_str(), flavor, "default", "0",
            ])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub(super) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        let uri = file_uri(path)?;
        if let Some(image) = read_cached(path, &uri, max_side) {
            return Some(image);
        }
        if !request_from_desktop(path, &uri, max_side) {
            return None;
        }

        // Thumbnailer1 is asynchronous. Poll its standard cache briefly while
        // remaining entirely on a background worker thread.
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(120));
            if let Some(image) = read_cached(path, &uri, max_side) {
                return Some(image);
            }
        }
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
        let thumbnail_size = max_side.clamp(64, 1024).to_string();
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
                .filter(|image| image.width().max(image.height()) >= max_side)
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
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_INCACHEONLY,
        SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
    };
    use windows::core::PCWSTR;

    fn bitmap_to_rgba(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<image::RgbaImage> {
        unsafe {
            let mut description = BITMAP::default();
            let read = GetObjectW(
                bitmap.into(),
                size_of::<BITMAP>() as i32,
                Some((&mut description as *mut BITMAP).cast()),
            );
            if read == 0 || description.bmWidth <= 0 || description.bmHeight == 0 {
                return None;
            }
            let width = description.bmWidth as u32;
            let height = description.bmHeight.unsigned_abs();
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
            if lines == 0 {
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

    fn request_bitmap(path: &Path, max_side: u32, cache_only: bool) -> Option<image::RgbaImage> {
        let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let factory: IShellItemImageFactory = unsafe {
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
        }.ok()?;
        let mut flags = SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT;
        if cache_only {
            flags |= SIIGBF_INCACHEONLY;
        }
        let bitmap = unsafe {
            factory.GetImage(
                SIZE { cx: max_side.clamp(64, 1024) as i32, cy: max_side.clamp(64, 1024) as i32 },
                flags,
            )
        }.ok()?;
        let result = bitmap_to_rgba(bitmap);
        unsafe { let _ = DeleteObject(bitmap.into()); }
        result
    }

    pub(super) fn load_or_request(path: &Path, max_side: u32) -> Option<image::RgbaImage> {
        // Shell thumbnail extraction uses COM. These workers are dedicated to
        // thumbnail tasks, so an STA is appropriate for Shell extension calls.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let meets_requested_size = |image: &image::RgbaImage| {
            image.width().max(image.height()) >= max_side
        };
        let cached = request_bitmap(path, max_side, true).filter(|image| meets_requested_size(image));
        let result = cached.or_else(|| request_bitmap(path, max_side, false).filter(|image| meets_requested_size(image)));
        if initialized {
            unsafe { CoUninitialize(); }
        }
        result
    }
}
