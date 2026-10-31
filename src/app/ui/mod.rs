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
    render_desktop_menu_bar, set_desktop_menu_bar_preference, uses_desktop_menu_bar,
};
pub use overlay::{
    render_overlay_ui, reserve_custom_titlebar_space, toolbar_frame, update_overlay_visibility,
};
pub use settings_menu::{render_settings_menu, toggle_settings_menu};
