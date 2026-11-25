#[cfg(target_os = "windows")]
pub(super) fn enable_popout_dwm_transparency(tab_id: u64, generation: u64) -> bool {
    use windows::core::{BOOL, PCWSTR};
    use windows::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DWM_BLURBEHIND, DWM_BB_BLURREGION, DWM_BB_ENABLE,
    };
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    // eframe 0.33's glow backend can clear the transparent attribute while
    // finalizing child viewports on Windows. Register this top-level HWND with
    // DWM's alpha-compositing path after it has been created and shown. An empty
    // blur region enables per-pixel alpha without adding the dark blur effect.
    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };
        let region = CreateRectRgn(0, 0, -1, -1);
        if region.is_invalid() {
            return false;
        }
        let blur_behind = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: BOOL(1),
            hRgnBlur: region,
            fTransitionOnMaximized: BOOL(0),
        };
        let applied = DwmEnableBlurBehindWindow(hwnd, &blur_behind).is_ok();
        let _ = DeleteObject(region.into());
        applied
    }
}


#[cfg(target_os = "windows")]
pub(super) fn apply_popout_native_window_effects(
    tab_id: u64,
    generation: u64,
    remove_effects: bool,
) -> bool {
    use std::ffi::c_void;
    use std::mem::{size_of, size_of_val};
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE,
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMSBT_AUTO, DWMSBT_NONE, DWMWCP_DEFAULT,
        DWMWCP_DONOTROUND,
    };
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::PCWSTR;

    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `FindWindowW` resolves the exact, generation-scoped top-level
    // pop-out. Each attribute pointer refers to an initialized local value that
    // remains alive for the duration of the synchronous DWM call.
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };

        let corner: DWM_WINDOW_CORNER_PREFERENCE = if remove_effects {
            DWMWCP_DONOTROUND
        } else {
            DWMWCP_DEFAULT
        };
        let border = if remove_effects {
            DWMWA_COLOR_NONE
        } else {
            DWMWA_COLOR_DEFAULT
        };
        let backdrop: DWM_SYSTEMBACKDROP_TYPE = if remove_effects {
            DWMSBT_NONE
        } else {
            DWMSBT_AUTO
        };
        let corner_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const DWM_WINDOW_CORNER_PREFERENCE).cast::<c_void>(),
            size_of_val(&corner) as u32,
        );
        let border_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&border as *const u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        );
        let backdrop_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast::<c_void>(),
            size_of_val(&backdrop) as u32,
        );

        // These attributes are not available on every supported Windows build.
        // The viewport's has_shadow hint and borderless style remain the fallback.
        let _ = (corner_result, border_result, backdrop_result);
        true
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn apply_popout_native_window_effects(
    _tab_id: u64,
    _generation: u64,
    _remove_effects: bool,
) -> bool {
    // Other platforms receive egui/winit's has_shadow and undecorated-window hints.
    true
}

#[cfg(target_os = "windows")]
pub(super) fn detach_popout_owner(tab_id: u64, generation: u64) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GW_OWNER, GWLP_HWNDPARENT, GetWindow, SetWindowLongPtrW,
    };
    use windows::core::PCWSTR;

    // Find by our unique tab-scoped title. Deferred viewports are created as
    // top-level owned windows; Win32 hides owned windows when their owner is
    // minimized. Clearing GWLP_HWNDPARENT makes the pop-out an independent
    // top-level window while keeping it in the same process and UI lifecycle.
    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };
        let owner_is_detached = || GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| owner.0.is_null());
        if owner_is_detached() {
            return true;
        }
        let _previous_owner = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
        owner_is_detached()
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn detach_popout_owner(_tab_id: u64, _generation: u64) -> bool {
    // The native parent-owner minimize behavior addressed above is Windows
    // specific. Deferred viewports already repaint independently elsewhere.
    true
}

#[cfg(target_os = "windows")]
pub(super) fn enforce_bottom_z_order(tab_id: u64, generation: u64) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };
    use windows::core::PCWSTR;

    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn enforce_bottom_z_order(_tab_id: u64, _generation: u64) {
    // Other window managers receive egui's WindowLevel::AlwaysOnBottom hint.
}
