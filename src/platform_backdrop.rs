//! Best-effort native window backdrop effects.
//!
//! On Windows 11 this requests the DWM transient-window backdrop (the acrylic-like
//! system material). On macOS it wraps the eframe content view in an AppKit
//! `NSVisualEffectView`. Unsupported desktops simply retain the application's
//! translucent egui surfaces; no system utility or shared library is downloaded.

use raw_window_handle::RawWindowHandle;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(target_os = "windows")]
static MAIN_HWND: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static MAIN_NS_VIEW: AtomicUsize = AtomicUsize::new(0);
#[cfg(any(target_os = "windows", target_os = "macos"))]
static NATIVE_BACKDROP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Retain the root OS view handle so the preference can be changed at runtime.
pub fn initialize(handle: Option<RawWindowHandle>) {
    #[cfg(target_os = "windows")]
    if let Some(RawWindowHandle::Win32(handle)) = handle {
        MAIN_HWND.store(handle.hwnd.get() as usize, Ordering::Relaxed);
    }

    #[cfg(target_os = "macos")]
    if let Some(RawWindowHandle::AppKit(handle)) = handle {
        MAIN_NS_VIEW.store(handle.ns_view.as_ptr() as usize, Ordering::Relaxed);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = handle;
}

/// Apply or remove the native effect. Failure is deliberately non-fatal: the UI
/// keeps its translucent fallback when the OS/compositor does not support blur.
pub fn set_enabled(enabled: bool) {
    #[cfg(target_os = "windows")]
    {
        let active = enabled && windows_backdrop::set_enabled(true);
        if !enabled {
            let _ = windows_backdrop::set_enabled(false);
        }
        NATIVE_BACKDROP_ACTIVE.store(active, Ordering::Relaxed);
    }

    #[cfg(target_os = "macos")]
    {
        let active = macos_backdrop::set_enabled(enabled, MAIN_NS_VIEW.load(Ordering::Relaxed));
        NATIVE_BACKDROP_ACTIVE.store(active, Ordering::Relaxed);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = enabled;
}

/// Best-effort native caption-color override. Passing `None` restores the OS default.
/// Windows 11 exposes the caption-color API; other native window managers retain
/// their system-controlled decoration colors.
pub fn set_native_titlebar_color(color: Option<[u8; 3]>) {
    #[cfg(target_os = "windows")]
    windows_backdrop::set_caption_color(color);

    #[cfg(not(target_os = "windows"))]
    let _ = color;
}

/// Whether a native system backdrop was successfully requested and is enabled.
/// Other platforms use egui's translucent-surface fallback and keep an opaque window.
pub fn is_active() -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        return NATIVE_BACKDROP_ACTIVE.load(Ordering::Relaxed);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

#[cfg(target_os = "windows")]
mod windows_backdrop {
    use super::MAIN_HWND;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU32, Ordering};

    const DWM_COLOR_DEFAULT: u32 = u32::MAX;
    static LAST_CAPTION_COLOR: AtomicU32 = AtomicU32::new(u32::MAX - 1);

    // DWMWA_SYSTEMBACKDROP_TYPE (Windows 11 22H2+). Values mirror Microsoft's
    // DWMSYSTEMBACKDROP_TYPE: NONE = 1, TRANSIENTWINDOW (Acrylic-like) = 3.
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
    const DWMSBT_NONE: u32 = 1;
    const DWMSBT_TRANSIENTWINDOW: u32 = 3;

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            dw_attribute: u32,
            pv_attribute: *const c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    pub(super) fn set_enabled(enabled: bool) -> bool {
        let hwnd = MAIN_HWND.load(Ordering::Relaxed);
        if hwnd == 0 {
            return false;
        }
        let backdrop = if enabled { DWMSBT_TRANSIENTWINDOW } else { DWMSBT_NONE };
        // Windows versions without this DWM attribute return an error. That is a
        // supported fallback condition, not an application error.
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd as *mut c_void,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        result >= 0
    }

    pub(super) fn set_caption_color(color: Option<[u8; 3]>) {
        let hwnd = MAIN_HWND.load(Ordering::Relaxed);
        if hwnd == 0 {
            return;
        }

        // DWM expects COLORREF byte order: 0x00BBGGRR.
        let caption_color = match color {
            Some([r, g, b]) => (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
            None => DWM_COLOR_DEFAULT,
        };
        if LAST_CAPTION_COLOR.swap(caption_color, Ordering::Relaxed) == caption_color {
            return;
        }

        let text_color = match color {
            Some([r, g, b]) => {
                let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                if luminance < 145.0 { 0x00FF_FFFF } else { 0x0000_0000 }
            }
            None => DWM_COLOR_DEFAULT,
        };

        // Unsupported Windows versions return a failure HRESULT; system-native
        // decorations remain in control when these attributes are unavailable.
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd as *mut c_void,
                DWMWA_CAPTION_COLOR,
                &caption_color as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd as *mut c_void,
                DWMWA_TEXT_COLOR,
                &text_color as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_backdrop {
    use super::MAIN_NS_VIEW;
    use std::ffi::{c_char, c_void};
    use std::mem;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Obj = *mut c_void;
    type Sel = *mut c_void;

    static CONTENT_VIEW: AtomicUsize = AtomicUsize::new(0);
    static EFFECT_VIEW: AtomicUsize = AtomicUsize::new(0);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsPoint { x: f64, y: f64 }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsSize { width: f64, height: f64 }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsRect { origin: NsPoint, size: NsSize }

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> Obj;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }

    unsafe fn selector(name: &'static [u8]) -> Sel {
        // All selector byte strings below are statically NUL-terminated.
        unsafe { sel_registerName(name.as_ptr().cast()) }
    }

    unsafe fn send_object(receiver: Obj, name: &'static [u8]) -> Obj {
        if receiver.is_null() { return std::ptr::null_mut(); }
        let function: unsafe extern "C" fn(Obj, Sel) -> Obj = unsafe { mem::transmute(objc_msgSend as *const ()) };
        unsafe { function(receiver, selector(name)) }
    }

    unsafe fn send_object_rect(receiver: Obj, name: &'static [u8], rect: NsRect) -> Obj {
        let function: unsafe extern "C" fn(Obj, Sel, NsRect) -> Obj = unsafe { mem::transmute(objc_msgSend as *const ()) };
        unsafe { function(receiver, selector(name), rect) }
    }

    unsafe fn send_void_object(receiver: Obj, name: &'static [u8], value: Obj) {
        let function: unsafe extern "C" fn(Obj, Sel, Obj) = unsafe { mem::transmute(objc_msgSend as *const ()) };
        unsafe { function(receiver, selector(name), value) }
    }

    unsafe fn send_void_bool(receiver: Obj, name: &'static [u8], value: bool) {
        let function: unsafe extern "C" fn(Obj, Sel, i8) = unsafe { mem::transmute(objc_msgSend as *const ()) };
        unsafe { function(receiver, selector(name), i8::from(value)) }
    }

    unsafe fn send_void_usize(receiver: Obj, name: &'static [u8], value: usize) {
        let function: unsafe extern "C" fn(Obj, Sel, usize) = unsafe { mem::transmute(objc_msgSend as *const ()) };
        unsafe { function(receiver, selector(name), value) }
    }

    unsafe fn color(class_name: &'static [u8], method: &'static [u8]) -> Obj {
        let cls = unsafe { objc_getClass(class_name.as_ptr().cast()) };
        unsafe { send_object(cls, method) }
    }

    unsafe fn attach_effect(window: Obj) -> Obj {
        let existing = EFFECT_VIEW.load(Ordering::Relaxed) as Obj;
        if !existing.is_null() { return existing; }

        let content = unsafe { send_object(window, b"contentView\0") };
        if content.is_null() { return std::ptr::null_mut(); }
        let cls = unsafe { objc_getClass(b"NSVisualEffectView\0".as_ptr().cast()) };
        if cls.is_null() { return std::ptr::null_mut(); }
        let allocated = unsafe { send_object(cls, b"alloc\0") };
        let frame = NsRect {
            origin: NsPoint { x: 0.0, y: 0.0 },
            // The window resizes its root content view to the actual content bounds.
            // A large initial view avoids calling an NSRect-returning method through
            // objc_msgSend (whose ABI differs between Intel and Apple Silicon).
            size: NsSize { width: 32768.0, height: 32768.0 },
        };
        let effect = unsafe { send_object_rect(allocated, b"initWithFrame:\0", frame) };
        if effect.is_null() { return std::ptr::null_mut(); }

        // Keep the existing eframe/Winit content view alive while replacing the
        // NSWindow's root content view with a material-bearing wrapper.
        unsafe {
            let _ = send_object(content, b"retain\0");
            send_void_usize(effect, b"setAutoresizingMask:\0", 18); // width + height sizable
            send_void_usize(effect, b"setMaterial:\0", 21); // UnderWindowBackground
            send_void_usize(effect, b"setBlendingMode:\0", 0); // BehindWindow
            send_void_usize(effect, b"setState:\0", 1); // Active
            send_void_object(window, b"setContentView:\0", effect);
            send_void_usize(content, b"setAutoresizingMask:\0", 18);
            send_void_object(effect, b"addSubview:\0", content);
            let _ = send_object(content, b"release\0");
            send_void_bool(content, b"setWantsLayer:\0", true);
            let layer = send_object(content, b"layer\0");
            if !layer.is_null() { send_void_bool(layer, b"setOpaque:\0", false); }
        }

        CONTENT_VIEW.store(content as usize, Ordering::Relaxed);
        EFFECT_VIEW.store(effect as usize, Ordering::Relaxed);
        effect
    }

    pub(super) fn set_enabled(enabled: bool, view_handle: usize) -> bool {
        let view = if view_handle != 0 { view_handle as Obj } else { MAIN_NS_VIEW.load(Ordering::Relaxed) as Obj };
        if view.is_null() { return false; }
        // Do not alter the native view hierarchy merely to disable an effect that
        // has never been created. Attach it lazily when the user enables the option.
        if !enabled && EFFECT_VIEW.load(Ordering::Relaxed) == 0 {
            return false;
        }

        unsafe {
            let window = send_object(view, b"window\0");
            if window.is_null() { return false; }
            let effect = attach_effect(window);
            if effect.is_null() { return false; }

            let content = CONTENT_VIEW.load(Ordering::Relaxed) as Obj;
            let layer = if content.is_null() { std::ptr::null_mut() } else { send_object(content, b"layer\0") };
            if enabled {
                send_void_usize(effect, b"setMaterial:\0", 21);
                send_void_usize(effect, b"setBlendingMode:\0", 0);
                send_void_usize(effect, b"setState:\0", 1);
                send_void_bool(window, b"setOpaque:\0", false);
                let clear = color(b"NSColor\0", b"clearColor\0");
                send_void_object(window, b"setBackgroundColor:\0", clear);
                if !layer.is_null() { send_void_bool(layer, b"setOpaque:\0", false); }
            } else {
                // Switch the effect view to a within-window appearance material; it
                // remains as the wrapper so the child renderer is never hidden.
                send_void_usize(effect, b"setMaterial:\0", 0); // AppearanceBased
                send_void_usize(effect, b"setBlendingMode:\0", 1); // WithinWindow
                send_void_bool(window, b"setOpaque:\0", true);
                let background = color(b"NSColor\0", b"windowBackgroundColor\0");
                send_void_object(window, b"setBackgroundColor:\0", background);
                if !layer.is_null() { send_void_bool(layer, b"setOpaque:\0", true); }
            }
        }
        enabled
    }
}
