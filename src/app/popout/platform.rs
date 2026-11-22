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
