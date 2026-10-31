//! Appearance-profile selection and navigation placement policy.

use eframe::egui;

pub fn resolve_look_profile(preference: &str) -> &'static str {
    match preference {
        "native" => "native",
        "cosmic" => "cosmic",
        "gnome" => "gnome",
        "win98" => "win98",
        _ => super::platform::detect_desktop_look(),
    }
}

/// Resolve the saved theme choice. Selecting Windows 98 sets the preference to Light once;
/// a later explicit Follow System selection remains system-driven.
pub fn effective_theme_preference(
    theme_preference: &str,
    _look_preference: &str,
) -> egui::ThemePreference {
    match theme_preference {
        "light" => egui::ThemePreference::Light,
        "dark" => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    }
}

pub fn current_look_profile(ctx: &egui::Context) -> &'static str {
    // Resolve to static labels so overlay toolbars can query this every frame without
    // allocating a new String in the viewer's rendering path.
    ctx.data(|data| {
        match data
            .get_temp::<String>(egui::Id::new("jpeg_viewer_look_profile"))
            .as_deref()
        {
            Some("cosmic") => "cosmic",
            Some("gnome") => "gnome",
            Some("win98") => "win98",
            _ => "native",
        }
    })
}

/// Resolve the main toolbar/navigation placement independently from the visual profile.
/// Automatic placement follows the currently active look profile.
pub fn resolve_main_ui_placement(preference: &str, ctx: &egui::Context) -> &'static str {
    // The Win98 profile includes a fixed menu bar with its toolbar directly below;
    // separate placement preferences apply again when switching to another profile.
    if current_look_profile(ctx) == "win98" {
        return "win98";
    }
    match preference {
        "native" => "native",
        "cosmic" => "cosmic",
        "gnome" => "gnome",
        "win98" => "win98",
        _ => current_look_profile(ctx),
    }
}

/// Resolve the three-state auto-hide preference. In Automatic mode, auto-hide is
/// deliberately disabled for COSMIC/GNOME-style toolbar placements so controls do
/// not disappear from their desktop-style header/footer positions.
pub fn navigation_auto_hide_enabled(
    settings: &crate::settings::AppSettings,
    ctx: &egui::Context,
) -> bool {
    // Fullscreen temporarily overrides the saved preference for every look/placement.
    // Leaving fullscreen restores the user's normal Auto/On/Off choice automatically.
    if ctx.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
        return true;
    }
    match settings.navigation_ui_auto_hide_preference.as_str() {
        "on" => true,
        "off" => false,
        _ => resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx) == "native",
    }
}

/// Store the resolved profile in egui context data so style refreshes retain it.
pub fn set_look_profile(ctx: &egui::Context, preference: &str) {
    let profile = resolve_look_profile(preference);
    let cosmic_frosted = if profile == "cosmic" {
        super::platform::detect_cosmic_frosted(ctx.style().visuals.dark_mode).unwrap_or(true)
    } else {
        false
    };
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("jpeg_viewer_look_profile"),
            profile.to_string(),
        );
        // Cache this desktop preference rather than reading a config file in every frame.
        data.insert_temp(egui::Id::new("jpeg_viewer_cosmic_frosted"), cosmic_frosted);
    });
    let requested_blur = super::backdrop::os_background_blur_preference(ctx);
    crate::platform_backdrop::set_enabled(requested_blur && profile != "win98");
}

/*
/// Whether the detected COSMIC theme requests frosted surfaces. Egui can only
/// reproduce translucency inside the application; compositor-level blur remains
/// controlled by COSMIC and is not enabled by this flag.
pub fn cosmic_frosted_preference(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::new("jpeg_viewer_cosmic_frosted")))
        .unwrap_or(true)
}

 */

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn automatic_navigation_auto_hide_respects_toolbar_placement() {
        let ctx = egui::Context::default();
        set_look_profile(&ctx, "native");
        let mut settings = crate::settings::AppSettings::default();
        settings.navigation_ui_auto_hide_preference = "auto".to_owned();
        settings.main_ui_placement_preference = "native".to_owned();
        assert!(navigation_auto_hide_enabled(&settings, &ctx));

        settings.main_ui_placement_preference = "cosmic".to_owned();
        assert!(!navigation_auto_hide_enabled(&settings, &ctx));
        settings.main_ui_placement_preference = "gnome".to_owned();
        assert!(!navigation_auto_hide_enabled(&settings, &ctx));

        settings.navigation_ui_auto_hide_preference = "on".to_owned();
        assert!(navigation_auto_hide_enabled(&settings, &ctx));
        settings.navigation_ui_auto_hide_preference = "off".to_owned();
        settings.main_ui_placement_preference = "native".to_owned();
        assert!(!navigation_auto_hide_enabled(&settings, &ctx));
    }
}

