//! Native backdrop effects and title-bar surface integration.

use eframe::egui;
use super::colors::{main_accent_color, main_view_background_color, with_alpha};
use super::profiles::current_look_profile;

const OS_BACKGROUND_BLUR_ID: &str = "jpeg_viewer_os_background_blur_preference";
const OS_BACKGROUND_BLUR_FULLSCREEN_DISABLE_ID: &str =
    "jpeg_viewer_os_background_blur_fullscreen_disable";
const OS_BACKGROUND_BLUR_APPLIED_ID: &str = "jpeg_viewer_os_background_blur_applied";
const OS_BACKGROUND_BLUR_LAST_EFFECTIVE_ID: &str = "jpeg_viewer_os_background_blur_last_effective";
/// Resolve the custom title-bar surface from the selected color source.
/// `system` follows the active UI profile's neutral surface, `main_view` uses
/// the exact canvas background, and `main_color` uses the selected accent.
pub fn title_bar_surface_color(
    ctx: &egui::Context,
    opacity: u8,
    color_mode: &str,
    main_view_background: &str,
) -> egui::Color32 {
    let visuals = &ctx.style().visuals;
    let color = match color_mode {
        "main_view" => main_view_background_color(main_view_background, visuals.dark_mode),
        "main_color" => main_accent_color(ctx),
        _ if current_look_profile(ctx) == "win98" => egui::Color32::from_rgb(0, 0, 128),
        _ => visuals.panel_fill,
    };
    with_alpha(color, opacity)
}

/// Return a solid RGB caption override for native window decorations. `None`
/// means the operating system should restore its own title-bar color.
pub fn native_title_bar_color(
    ctx: &egui::Context,
    color_mode: &str,
    main_view_background: &str,
) -> Option<[u8; 3]> {
    let visuals = &ctx.style().visuals;
    let color = match color_mode {
        "main_view" => main_view_background_color(main_view_background, visuals.dark_mode),
        "main_color" => main_accent_color(ctx),
        _ => return None,
    };
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    Some([r, g, b])
}

/// Store the user's OS/DE backdrop preference and immediately reconcile the native backdrop.
pub fn set_os_background_blur_preference(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(OS_BACKGROUND_BLUR_ID), enabled);
    });
    sync_os_background_blur(ctx);
}

/// Control whether the backdrop effect is disabled only while fullscreen.
pub fn set_disable_os_background_blur_in_fullscreen(ctx: &egui::Context, disabled: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(OS_BACKGROUND_BLUR_FULLSCREEN_DISABLE_ID),
            disabled,
        );
    });
    sync_os_background_blur(ctx);
}

pub fn os_background_blur_preference(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new(OS_BACKGROUND_BLUR_ID))
            .unwrap_or(true)
    })
}

pub fn disable_os_background_blur_in_fullscreen_preference(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new(OS_BACKGROUND_BLUR_FULLSCREEN_DISABLE_ID))
            .unwrap_or(true)
    })
}

/// Whether glass is currently effective for drawing UI surfaces. This returns false
/// when the preference is off, the Win98 profile is selected, or fullscreen suppression
/// is enabled while the native viewport is fullscreen.
pub fn os_background_blur_enabled(ctx: &egui::Context) -> bool {
    let requested = os_background_blur_preference(ctx);
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    let suppressed_in_fullscreen =
        fullscreen && disable_os_background_blur_in_fullscreen_preference(ctx);
    requested && current_look_profile(ctx) != "win98" && !suppressed_in_fullscreen
}

/// Synchronize the native backdrop with the effective UI preference. Returns true if
/// the effective style changed, so the caller can rebuild opacity-sensitive widget visuals.
pub fn sync_os_background_blur(ctx: &egui::Context) -> bool {
    let effective = os_background_blur_enabled(ctx);
    let previous_effective =
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new(OS_BACKGROUND_BLUR_LAST_EFFECTIVE_ID)));
    let changed = previous_effective != Some(effective);
    if changed {
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(OS_BACKGROUND_BLUR_LAST_EFFECTIVE_ID),
                effective,
            );
        });
    }

    let previous_native_request =
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new(OS_BACKGROUND_BLUR_APPLIED_ID)));
    if previous_native_request != Some(effective) {
        // The native module safely handles unsupported versions and desktops.
        crate::platform_backdrop::set_enabled(effective);
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(OS_BACKGROUND_BLUR_APPLIED_ID), effective);
        });
    }
    changed
}

/// Paint the classic Windows 98 two-pixel raised/sunken bevel around a control.
pub(crate) fn paint_win98_bevel(ui: &egui::Ui, rect: egui::Rect, pressed: bool) {
    let painter = ui.painter();
    let dark_mode = ui.visuals().dark_mode;
    let white = if dark_mode {
        egui::Color32::from_rgb(83, 86, 91)
    } else {
        egui::Color32::WHITE
    };
    let light = if dark_mode {
        egui::Color32::from_rgb(62, 65, 70)
    } else {
        egui::Color32::from_rgb(223, 223, 223)
    };
    let gray = if dark_mode {
        egui::Color32::from_rgb(25, 27, 30)
    } else {
        egui::Color32::from_rgb(128, 128, 128)
    };
    let black = if dark_mode {
        egui::Color32::from_rgb(8, 9, 10)
    } else {
        egui::Color32::BLACK
    };
    let (tl_outer, tl_inner, br_inner, br_outer) = if pressed {
        (black, gray, light, white)
    } else {
        (white, light, gray, black)
    };
    for (offset, color) in [(0.5, tl_outer), (1.5, tl_inner)] {
        let stroke = egui::Stroke::new(1.0_f32, color);
        painter.line_segment(
            [
                egui::pos2(rect.left() + offset, rect.top() + offset),
                egui::pos2(rect.right() - offset, rect.top() + offset),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(rect.left() + offset, rect.top() + offset),
                egui::pos2(rect.left() + offset, rect.bottom() - offset),
            ],
            stroke,
        );
    }
    for (offset, color) in [(0.5, br_outer), (1.5, br_inner)] {
        let stroke = egui::Stroke::new(1.0_f32, color);
        painter.line_segment(
            [
                egui::pos2(rect.left() + offset, rect.bottom() - offset),
                egui::pos2(rect.right() - offset, rect.bottom() - offset),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(rect.right() - offset, rect.top() + offset),
                egui::pos2(rect.right() - offset, rect.bottom() - offset),
            ],
            stroke,
        );
    }
}

