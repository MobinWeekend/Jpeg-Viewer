//! Cross-platform appearance profiles, theme colors, and reusable UI styling.
//!
//! The module is split by responsibility while re-exporting the previous public
//! surface so existing application code can continue using `crate::ui_theme::…`.

mod backdrop;
mod colors;
mod controls;
mod platform;
mod profiles;
mod style;

pub use backdrop::{
    disable_os_background_blur_in_fullscreen_preference, native_title_bar_color,
    os_background_blur_enabled, os_background_blur_preference,
    set_disable_os_background_blur_in_fullscreen, set_os_background_blur_preference,
    sync_os_background_blur, title_bar_surface_color,
};
pub(crate) use backdrop::paint_win98_bevel;
pub use colors::{main_accent_color, main_view_background_color};
pub(crate) use colors::{contrast_foreground, contrast_prefers_light_text, with_alpha};
pub use platform::font_definitions_for_profile;
pub use profiles::{
    current_look_profile, effective_theme_preference, navigation_auto_hide_enabled,
    resolve_look_profile, resolve_main_ui_placement, set_look_profile,
};
pub use controls::{scaled_text_size, switch, BorderedSlider};
pub use style::apply_polished_style_with_options;
