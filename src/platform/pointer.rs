//! Desktop pointer position helpers for pop-out pass-through effects.
//!
//! Windows and X11 expose a desktop-wide pointer coordinate. Native Wayland
//! intentionally does not, so callers can only use pointer events delivered to
//! the application window in that session.

use eframe::egui;

/// Whether the Linux backend can query the pointer independently of app-window input.
///
/// The UI only displays this capability note on Linux; keeping the helper
/// Linux-only avoids compiling an unused Windows implementation.
#[cfg(target_os = "linux")]
pub fn global_position_available() -> bool {
    !crate::platform::wayland_pointer_recovery::is_running_on_wayland()
}

/// Return the global pointer position in the same logical desktop coordinates
/// used by monitor bounds and egui viewport outer rectangles.
pub fn global_position(_ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(target_os = "windows")]
    {
        return windows_global_position(_ctx);
    }
    #[cfg(target_os = "linux")]
    {
        return x11_global_position();
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "windows")]
fn windows_global_position(ctx: &egui::Context) -> Option<egui::Pos2> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point).ok()? };
    let physical_x = point.x as f32;
    let physical_y = point.y as f32;
    let monitors = crate::platform::monitors::available_monitors();
    let monitor = monitors.iter().find(|monitor| {
        let scale = monitor.scale_factor.max(0.5);
        let left = monitor.x * scale;
        let top = monitor.y * scale;
        let right = (monitor.x + monitor.width) * scale;
        let bottom = (monitor.y + monitor.height) * scale;
        physical_x >= left && physical_x <= right && physical_y >= top && physical_y <= bottom
    });
    let scale = monitor
        .map(|monitor| monitor.scale_factor)
        .unwrap_or_else(|| ctx.pixels_per_point())
        .max(0.5);
    Some(egui::pos2(physical_x / scale, physical_y / scale))
}

#[cfg(target_os = "linux")]
fn x11_global_position() -> Option<egui::Pos2> {
    if crate::platform::wayland_pointer_recovery::is_running_on_wayland() {
        return None;
    }

    use std::sync::OnceLock;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;
    use x11rb::rust_connection::RustConnection;

    static CONNECTION: OnceLock<Option<(RustConnection, usize)>> = OnceLock::new();
    let (connection, screen_index) = CONNECTION
        .get_or_init(|| x11rb::connect(None).ok())
        .as_ref()?;
    let root = connection.setup().roots.get(*screen_index)?.root;
    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
    let physical_x = pointer.root_x as f32;
    let physical_y = pointer.root_y as f32;

    // X11 monitor discovery reports logical coordinates. On normal X11
    // sessions its scale is 1; retain the per-output conversion for scaled setups.
    let monitors = crate::platform::monitors::available_monitors();
    let monitor = monitors.iter().find(|monitor| {
        let scale = monitor.scale_factor.max(0.5);
        let left = monitor.x * scale;
        let top = monitor.y * scale;
        let right = (monitor.x + monitor.width) * scale;
        let bottom = (monitor.y + monitor.height) * scale;
        physical_x >= left && physical_x <= right && physical_y >= top && physical_y <= bottom
    });
    let scale = monitor
        .map(|monitor| monitor.scale_factor)
        .unwrap_or(1.0)
        .max(0.5);
    Some(egui::pos2(physical_x / scale, physical_y / scale))
}
