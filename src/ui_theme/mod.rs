//! Cross-platform appearance profiles, theme colors, and reusable UI styling.
//!
//! The module is split by responsibility while re-exporting the previous public
//! surface so existing application code can continue using `crate::ui_theme::…`.

use eframe::egui;
mod backdrop;
mod colors;
mod controls;
mod platform;
pub(crate) mod platform_backdrop;
mod profiles;
mod style;

pub(crate) use backdrop::paint_win98_bevel;
pub use backdrop::{
    disable_os_background_blur_in_fullscreen_preference, main_view_surface_color,
    native_title_bar_color, os_background_blur_enabled, os_background_blur_preference,
    set_disable_os_background_blur_in_fullscreen, set_os_background_blur_preference,
    sync_os_background_blur, title_bar_surface_color,
};
pub(crate) use colors::{contrast_foreground, contrast_prefers_light_text, with_alpha};
pub use colors::{main_accent_color, main_view_background_color};
pub use controls::{BorderedSlider, scaled_text_size, switch};
pub use platform::font_definitions_for_profile;
pub use profiles::{
    current_look_profile, effective_theme_preference, navigation_auto_hide_enabled,
    resolve_look_profile, resolve_main_ui_placement, set_look_profile,
};
pub use style::apply_polished_style_with_options;

/// Shared vertical navigation tab used by Help and Settings.
/// Keeps row sizing, icon alignment, text, hover/pressed feedback, and selection
/// indicators identical across utility windows.
pub(crate) fn vertical_tab(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    paint_icon: impl FnOnce(&mut egui::Ui, egui::Rect, egui::Color32),
) -> egui::Response {
    let font_id = egui::FontId::proportional(12.5);
    let text_color = if selected {
        ui.visuals().text_color()
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, text_color);
    let width = ui.available_width().max(galley.size().x + 43.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 36.0), egui::Sense::click());

    if selected || response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else if selected {
            &ui.visuals().widgets.open
        } else {
            &ui.visuals().widgets.hovered
        };
        let fill = if visuals.bg_fill == egui::Color32::TRANSPARENT {
            with_alpha(ui.visuals().selection.bg_fill, if selected { 112 } else { 64 })
        } else {
            visuals.bg_fill
        };
        ui.painter().rect_filled(rect, egui::CornerRadius::same(7), fill);
    }
    if selected {
        let marker = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, rect.top() + 7.0),
            egui::vec2(2.0, (rect.height() - 14.0).max(0.0)),
        );
        ui.painter().rect_filled(
            marker,
            egui::CornerRadius::same(1),
            ui.visuals().selection.stroke.color,
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    let foreground = if selected || response.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    paint_icon(ui, icon_rect, foreground);
    ui.painter().galley(
        egui::pos2(icon_rect.right() + 7.0, rect.center().y - galley.size().y * 0.5),
        galley,
        foreground,
    );

    response
}

/// Render a settings slider using the shared label-above-slider layout.
/// Keeping layout here ensures all Settings pages use the same typography and spacing.
pub(crate) fn labeled_slider(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    add_slider: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label.into()).strong());
        ui.add_space(2.0);
        add_slider(ui)
    })
    .inner
}
