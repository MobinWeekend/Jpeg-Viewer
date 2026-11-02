mod central_panel;
mod desktop_menu_bar;
mod editing;
mod error_ui;
mod hamburger_ui;
mod help_window;
pub(crate) mod helpers;
mod metainfo;
mod navigation_ui;
mod overlay;
pub mod settings_menu;
mod tricks;
mod welcome_ui;

pub use desktop_menu_bar::{
    bump_help_window_generation, desktop_menu_bar_generation_for_help, desktop_menu_bar_height,
    desktop_menu_bar_in_custom_titlebar, render_desktop_menu_bar,
    render_desktop_menu_bar_in_titlebar, set_desktop_menu_bar_preference, uses_desktop_menu_bar,
};
pub use overlay::{
    custom_titlebar_height, render_overlay_ui, reserve_custom_titlebar_space, toolbar_frame,
    update_overlay_visibility,
};
pub use settings_menu::{render_settings_menu, toggle_settings_menu};

/// Return a viewport-local rectangle for effects that must stay attached to the
/// window. `ViewportInfo::inner_rect` can carry a desktop-space origin; using it
/// directly makes a backdrop's UV mapping shift when the native window moves.
pub(crate) fn tricks_backdrop_window_rect(
    ctx: &eframe::egui::Context,
    fallback: eframe::egui::Rect,
) -> eframe::egui::Rect {
    let size = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
    size.filter(|size| size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0)
        .map(|size| eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, size))
        .unwrap_or_else(|| {
            eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                fallback.size(),
            )
        })
}
