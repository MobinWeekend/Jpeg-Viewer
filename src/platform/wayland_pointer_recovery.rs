//! Temporary workaround for egui's stuck-pointer state after native window operations on Wayland.
//!
//! Targeted at this project's current integration: eframe/egui 0.33.3 with winit 0.30.13 (as pinned in Cargo.lock).
//! See https://github.com/emilk/egui/issues/7959.
//!
//! This module is intentionally isolated so it can be removed once the upstream
//! egui/winit integration reliably receives pointer-release events after the
//! compositor takes over a window move or resize.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use raw_window_handle::RawDisplayHandle;

static RUNNING_ON_WAYLAND: AtomicBool = AtomicBool::new(false);

/// Record the actual display backend selected for the native window.
///
/// Using the raw display handle is more reliable than checking environment
/// variables: a Wayland desktop can still run the application through XWayland.
pub fn initialize(display: Option<RawDisplayHandle>) {
    let is_wayland = matches!(display, Some(RawDisplayHandle::Wayland(_)));
    RUNNING_ON_WAYLAND.store(is_wayland, Ordering::Relaxed);
}

/// Return whether the application is using a native Wayland display backend.
///
/// This is based on the actual display handle, not environment variables, so
/// XWayland sessions are correctly treated as X11 for window-position support.
pub fn is_running_on_wayland() -> bool {
    RUNNING_ON_WAYLAND.load(Ordering::Relaxed)
}

/// Release egui's stale pointer/drag state after handing a move or resize to
/// the native window manager.
///
/// Wayland compositors may take over the pointer gesture without delivering the
/// release event back to the client. Resetting the state here prevents egui from
/// treating the next interaction as part of the old drag. This is deliberately
/// a no-op on X11 and is only compiled into Linux builds.
pub fn after_native_window_operation(ctx: &egui::Context) {
    if !RUNNING_ON_WAYLAND.load(Ordering::Relaxed) {
        return;
    }

    ctx.input_mut(|input| input.pointer = egui::PointerState::default());
    ctx.stop_dragging();
}
