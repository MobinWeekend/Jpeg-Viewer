//! Cross-platform appearance profiles and optional desktop-native integration.
//!
//! The application UI is rendered by egui. The system window manager still owns
//! native window decorations, and `rfd` uses the XDG Desktop Portal on Linux when
//! available. Linux desktop fonts are loaded at runtime through fontconfig when it
//! is installed; neither GNOME nor COSMIC UI libraries are required to launch.

use eframe::egui;

const OS_BACKGROUND_BLUR_ID: &str = "jpeg_viewer_os_background_blur_preference";
const OS_BACKGROUND_BLUR_FULLSCREEN_DISABLE_ID: &str =
    "jpeg_viewer_os_background_blur_fullscreen_disable";
const OS_BACKGROUND_BLUR_APPLIED_ID: &str = "jpeg_viewer_os_background_blur_applied";
const OS_BACKGROUND_BLUR_LAST_EFFECTIVE_ID: &str = "jpeg_viewer_os_background_blur_last_effective";
const MAIN_ACCENT_COLOR_ID: &str = "jpeg_viewer_main_accent_color";

/// The resolved accent selected by Settings > Main color. Cached with the style so
/// custom-drawn elements (such as the title bar) use the same color as egui widgets.
pub fn main_accent_color(ctx: &egui::Context) -> egui::Color32 {
    ctx.data(|data| data.get_temp::<egui::Color32>(egui::Id::new(MAIN_ACCENT_COLOR_ID)))
        .unwrap_or_else(|| ctx.style().visuals.selection.stroke.color)
}

/// Mix the configured main accent into the neutral panel surface for the custom
/// title bar. The tint is stronger in dark mode but intentionally keeps enough
/// luminance separation for the theme's foreground text.
pub fn title_bar_surface_color(ctx: &egui::Context, opacity: u8) -> egui::Color32 {
    let visuals = &ctx.style().visuals;
    let accent = main_accent_color(ctx);
    if current_look_profile(ctx) == "win98" {
        // The automatic accent resolves to classic Windows 98 navy; an explicitly
        // selected main color is used directly instead of being mixed back toward navy.
        return with_alpha(accent, opacity);
    }
    let base = visuals.panel_fill;
    let amount = if visuals.dark_mode { 0.52 } else { 0.22 };
    let [br, bg, bb, _] = base.to_srgba_unmultiplied();
    let [ar, ag, ab, _] = accent.to_srgba_unmultiplied();
    let mix = |b: u8, a: u8| {
        (b as f32 * (1.0 - amount) + a as f32 * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let surface =
        egui::Color32::from_rgba_unmultiplied(mix(br, ar), mix(bg, ag), mix(bb, ab), opacity);
    if visuals.dark_mode {
        darken_surface_for_text(surface, egui::Color32::from_rgb(237, 239, 242))
    } else {
        surface
    }
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

pub(crate) fn with_alpha(color: egui::Color32, alpha: u8) -> egui::Color32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
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

fn relative_luminance(color: egui::Color32) -> f32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    let linear = |channel: u8| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Select whichever neutral foreground yields the strongest contrast against a
/// widget's surface. The selected color is then used for labels and controls,
/// rather than assuming every state uses the same foreground.
pub(crate) fn contrast_foreground(background: egui::Color32) -> egui::Color32 {
    let luminance = relative_luminance(background);
    let black_contrast = (luminance + 0.05) / 0.05;
    let white_contrast = 1.05 / (luminance + 0.05);
    if white_contrast >= black_contrast {
        egui::Color32::from_rgb(248, 249, 250)
    } else {
        egui::Color32::from_rgb(24, 26, 28)
    }
}

/// Whether white foreground text yields the stronger neutral contrast on a surface.
pub(crate) fn contrast_prefers_light_text(background: egui::Color32) -> bool {
    contrast_foreground(background).r() > 200
}

fn contrast_ratio(foreground: egui::Color32, background: egui::Color32) -> f32 {
    let fg = relative_luminance(foreground);
    let bg = relative_luminance(background);
    (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05)
}

/// Composite an alpha-bearing surface against the worst-case backdrop for a
/// particular foreground. If foreground text is brighter than the surface, a white
/// backdrop is the hard case; if it is darker, a black backdrop is the hard case.
/// This is conservative for glass surfaces where the actual desktop backdrop varies.
fn contrast_surface_for_text(
    background: egui::Color32,
    foreground: egui::Color32,
) -> egui::Color32 {
    let [r, g, b, a] = background.to_srgba_unmultiplied();
    if a == 255 {
        return background;
    }
    let underlay = if relative_luminance(foreground) > relative_luminance(background) {
        255.0
    } else {
        0.0
    };
    let alpha = a as f32 / 255.0;
    let composite = |channel: u8| {
        (channel as f32 * alpha + underlay * (1.0 - alpha))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgb(composite(r), composite(g), composite(b))
}

/// Mix an accent into white for light-theme surfaces. A pale tint keeps dark
/// foreground text readable, unlike multiplying the accent (which darkens it).
fn tint_with_white(color: egui::Color32, color_strength: f32) -> egui::Color32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    let strength = color_strength.clamp(0.0, 1.0);
    let mix = |channel: u8| {
        (channel as f32 * strength + 255.0 * (1.0 - strength))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgb(mix(r), mix(g), mix(b))
}

/// Keep colored inline text such as links distinguishable while meeting the WCAG
/// AA 4.5:1 contrast target against the current surface whenever possible.
fn accessible_accent(foreground: egui::Color32, background: egui::Color32) -> egui::Color32 {
    let effective_background = contrast_surface_for_text(background, foreground);
    if contrast_ratio(foreground, effective_background) >= 4.5 {
        return foreground;
    }
    let [r, g, b, _] = foreground.to_srgba_unmultiplied();
    let target = if relative_luminance(effective_background) > 0.45 {
        0u8
    } else {
        255u8
    };
    for step in 1..=20 {
        let amount = step as f32 / 20.0;
        let mix = |channel: u8| {
            (channel as f32 * (1.0 - amount) + target as f32 * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        let candidate = egui::Color32::from_rgb(mix(r), mix(g), mix(b));
        let candidate_background = contrast_surface_for_text(background, candidate);
        if contrast_ratio(candidate, candidate_background) >= 4.5 {
            return candidate;
        }
    }
    contrast_foreground(effective_background)
}

fn set_widget_foreground(widget: &mut egui::style::WidgetVisuals, foreground: egui::Color32) {
    // Keep all standard widget states aligned with the selected theme. Rather than
    // changing text to black on a bright accent hover (which looks broken in dark
    // mode), accent surfaces are tuned to remain dark enough for light foregrounds.
    widget.fg_stroke = egui::Stroke::new(widget.fg_stroke.width, foreground);
}

/// Keep a surface dark enough for the chosen light foreground. This is especially
/// useful for selected/hovered controls in dark mode when the user's accent is bright.
fn darken_surface_for_text(surface: egui::Color32, foreground: egui::Color32) -> egui::Color32 {
    if contrast_ratio(foreground, contrast_surface_for_text(surface, foreground)) >= 4.5 {
        return surface;
    }
    let [r, g, b, a] = surface.to_srgba_unmultiplied();
    for step in 1..=20 {
        let scale = 1.0 - (step as f32 / 20.0);
        let candidate = egui::Color32::from_rgba_unmultiplied(
            (r as f32 * scale).round() as u8,
            (g as f32 * scale).round() as u8,
            (b as f32 * scale).round() as u8,
            a,
        );
        if contrast_ratio(foreground, contrast_surface_for_text(candidate, foreground)) >= 4.5 {
            return candidate;
        }
    }
    egui::Color32::from_rgba_unmultiplied(24, 26, 29, a)
}

/// Wrap an egui slider and draw a thin outline around its rail. egui's standard
/// slider rail uses `inactive.bg_fill` without `inactive.bg_stroke`, so the track
/// outline is painted explicitly after the widget itself.
pub struct BorderedSlider<'a> {
    slider: egui::Slider<'a>,
}

impl<'a> BorderedSlider<'a> {
    pub fn new(slider: egui::Slider<'a>) -> Self {
        Self { slider }
    }
}

impl egui::Widget for BorderedSlider<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let slider_width = ui.spacing().slider_width.max(24.0);
        let rail_height = ui.spacing().slider_rail_height.max(3.0);
        let visuals = ui.visuals().clone();
        // Scope the opacity override to this slider only. The rail and grab handle
        // remain fully opaque over photos/glass, while surrounding panels stay translucent.
        let response = ui
            .scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.selection.bg_fill = with_alpha(visuals.selection.bg_fill, 255);
                for widget in [
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                    &mut visuals.widgets.open,
                ] {
                    widget.bg_fill = with_alpha(widget.bg_fill, 255);
                    widget.weak_bg_fill = with_alpha(widget.weak_bg_fill, 255);
                }
                egui::Widget::ui(self.slider, ui)
            })
            .inner;

        // `Slider` allocates its rail first; optional numeric and text controls are
        // appended to the right. Its final response retains the rail's left edge and
        // vertically shares the same center line.
        let center = egui::pos2(
            response.rect.left() + slider_width * 0.5,
            response.rect.center().y,
        );
        let rail_rect = egui::Rect::from_center_size(center, egui::vec2(slider_width, rail_height));
        let border = if visuals.dark_mode {
            egui::Color32::from_rgb(142, 148, 156)
        } else {
            egui::Color32::from_rgb(112, 118, 126)
        };
        if ui.is_rect_visible(rail_rect) {
            let radius = if current_look_profile(ui.ctx()) == "win98" {
                egui::CornerRadius::ZERO
            } else {
                egui::CornerRadius::same((rail_height * 0.5).round().clamp(0.0, 255.0) as u8)
            };
            ui.painter().rect(
                rail_rect,
                radius,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(1.0_f32, border),
                egui::epaint::StrokeKind::Inside,
            );
        }
        response
    }
}

/// Resolve the selected visual profile. Automatic detection is runtime-only and
/// does not probe for or install desktop dependencies.
pub fn resolve_look_profile(preference: &str) -> &'static str {
    match preference {
        "native" => "native",
        "cosmic" => "cosmic",
        "gnome" => "gnome",
        "win98" => "win98",
        _ => detect_desktop_look(),
    }
}

/// Returns the desktop profile reported by the current Linux desktop session.
/// COSMIC is checked first because sessions may expose more than one desktop name.
fn detect_desktop_look() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let session = std::env::var("DESKTOP_SESSION").unwrap_or_default();
        let session_desktop = std::env::var("XDG_SESSION_DESKTOP").unwrap_or_default();
        let cosmic_session = std::env::var_os("COSMIC_SESSION").is_some()
            || std::env::var_os("COSMIC_DATA_DIR").is_some();

        let combined = format!("{current}:{session}:{session_desktop}").to_ascii_lowercase();
        let tokens: Vec<&str> = combined
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|part| !part.is_empty())
            .collect();

        if cosmic_session || tokens.iter().any(|part| *part == "cosmic") {
            return "cosmic";
        }
        if tokens.iter().any(|part| *part == "gnome") {
            return "gnome";
        }
    }

    "native"
}

/// Build font definitions with optional system fonts for the selected look.
/// `fc-match` is queried only while the application runs. If fontconfig, the
/// preferred font, or the font file is unavailable, egui's bundled fonts remain
/// available as a fallback. No native UI toolkit is linked by this function.
pub fn font_definitions_for_profile(preference: &str) -> egui::FontDefinitions {
    let _ = preference; // Used by the Linux-only desktop font selection below.
    let mut fonts = egui::FontDefinitions::default();

    // Keep emoji separate from normal text; it remains the last font fallback.
    fonts.font_data.insert(
        "noto_emoji".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/NotoEmoji-Regular.ttf")).into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("noto_emoji".to_owned());
    }

    #[cfg(target_os = "linux")]
    {
        let profile = resolve_look_profile(preference);
        let (profile_sans, profile_mono) = preferred_font_families(profile);
        // Ask fontconfig for the desktop's generic default first. The profile-specific
        // family is only a secondary system-font choice; egui's built-in font remains
        // the final fallback if neither system font can be loaded.
        let sans = load_font_from_fontconfig("sans-serif")
            .or_else(|| load_font_from_fontconfig(&profile_sans));
        let mono = load_font_from_fontconfig("monospace")
            .or_else(|| load_font_from_fontconfig(&profile_mono));

        if let Some(data) = sans {
            fonts.font_data.insert(
                "desktop_proportional".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            let family = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            family.retain(|name| name != "desktop_proportional");
            family.insert(0, "desktop_proportional".to_owned());
        }
        if let Some(data) = mono {
            fonts.font_data.insert(
                "desktop_monospace".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            let family = fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default();
            family.retain(|name| name != "desktop_monospace");
            family.insert(0, "desktop_monospace".to_owned());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let windows = std::env::var_os("WINDIR").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));
        let candidates = [
            ("desktop_proportional", windows.join("Fonts/segoeui.ttf"), egui::FontFamily::Proportional),
            ("desktop_monospace", windows.join("Fonts/consola.ttf"), egui::FontFamily::Monospace),
        ];
        for (name, path, family) in candidates {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(name.to_owned(), egui::FontData::from_owned(data).into());
                let entries = fonts.families.entry(family).or_default();
                entries.retain(|entry| entry != name);
                entries.insert(0, name.to_owned());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            ("desktop_proportional", "/System/Library/Fonts/SFNS.ttf", egui::FontFamily::Proportional),
            ("desktop_monospace", "/System/Library/Fonts/SFNSMono.ttf", egui::FontFamily::Monospace),
            ("desktop_proportional_fallback", "/Library/Fonts/Arial.ttf", egui::FontFamily::Proportional),
        ];
        for (name, path, family) in candidates {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(name.to_owned(), egui::FontData::from_owned(data).into());
                let entries = fonts.families.entry(family).or_default();
                entries.retain(|entry| entry != name);
                entries.insert(0, name.to_owned());
            }
        }
    }

    fonts
}

#[cfg(target_os = "linux")]
fn preferred_font_families(profile: &str) -> (String, String) {
    match profile {
        "gnome" => (
            gnome_font_family("font-name").unwrap_or_else(|| "Adwaita Sans".to_owned()),
            gnome_font_family("monospace-font-name").unwrap_or_else(|| "Adwaita Mono".to_owned()),
        ),
        "cosmic" => (
            cosmic_font_family("interface_font").unwrap_or_else(|| "Open Sans".to_owned()),
            cosmic_font_family("monospace_font").unwrap_or_else(|| "Noto Sans Mono".to_owned()),
        ),
        "win98" => ("Tahoma".to_owned(), "Courier New".to_owned()),
        _ => ("sans-serif".to_owned(), "monospace".to_owned()),
    }
}

#[cfg(target_os = "linux")]
fn gnome_font_family(setting: &str) -> Option<String> {
    if detect_desktop_look() != "gnome" {
        return None;
    }
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", setting])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim().trim_matches(|ch| ch == '\'' || ch == '"');
    let mut parts = value.split_whitespace().collect::<Vec<_>>();
    // GNOME settings normally serialize a family and trailing point size (e.g.
    // "Adwaita Sans 11"). Keep multiword family names intact.
    if parts.last().is_some_and(|last| last.parse::<f32>().is_ok()) {
        parts.pop();
    }
    let family = parts.join(" ");
    (!family.is_empty()).then_some(family)
}

#[cfg(target_os = "linux")]
fn cosmic_font_family(key: &str) -> Option<String> {
    let value = cosmic_tk_config_value(key)?;
    // COSMIC Tk serializes configured font families as RON structs such as
    // (family: "Open Sans", weight: Normal, stretch: Normal, style: Normal).
    let marker = "family:";
    let family_start = value.find(marker)? + marker.len();
    let after_marker = value[family_start..].trim_start();
    let quote = after_marker.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let rest = &after_marker[quote.len_utf8()..];
    let end = rest.find(quote)?;
    let family = rest[..end].trim();
    (!family.is_empty()).then_some(family.to_owned())
}

#[cfg(target_os = "linux")]
fn load_font_from_fontconfig(family: &str) -> Option<Vec<u8>> {
    use std::process::Command;

    let output = Command::new("fc-match")
        .args(["-f", "%{file}", family])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }

    std::fs::read(path).ok()
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
        detect_cosmic_frosted(ctx.style().visuals.dark_mode).unwrap_or(true)
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
    let requested_blur = ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new(OS_BACKGROUND_BLUR_ID))
            .unwrap_or(true)
    });
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

#[cfg(target_os = "linux")]
fn detect_gnome_accent() -> Option<&'static str> {
    // Read only the existing desktop preference. Missing schemas, commands, or
    // unsupported GNOME versions simply fall back to GNOME's standard blue.
    if detect_desktop_look() != "gnome" {
        return None;
    }

    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value
        .trim()
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .to_ascii_lowercase();
    match value.as_str() {
        "blue" => Some("blue"),
        "teal" => Some("teal"),
        "green" => Some("green"),
        "yellow" => Some("yellow"),
        "orange" => Some("orange"),
        "red" => Some("red"),
        "pink" | "rose" => Some("rose"),
        "purple" => Some("purple"),
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn detect_gnome_accent() -> Option<&'static str> {
    None
}

#[cfg(target_os = "linux")]
fn detect_cosmic_accent(dark: bool) -> Option<egui::Color32> {
    if detect_desktop_look() != "cosmic" {
        return None;
    }

    let value = cosmic_theme_config_value_for_mode(dark, "accent", &["v2", "v1"])?;
    parse_cosmic_color(&value)
}

#[cfg(not(target_os = "linux"))]
fn detect_cosmic_accent(_dark: bool) -> Option<egui::Color32> {
    None
}

#[cfg(target_os = "linux")]
fn cosmic_tk_config_value(key: &str) -> Option<String> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".config/cosmic"));
    }
    roots.push(std::path::PathBuf::from("/usr/share/cosmic"));

    for root in roots {
        let path = root.join("com.system76.CosmicTk").join("v1").join(key);
        if let Ok(value) = std::fs::read_to_string(path) {
            return Some(value);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn cosmic_theme_config_value_for_mode(dark: bool, key: &str, versions: &[&str]) -> Option<String> {
    let mode = if dark { "Dark" } else { "Light" };
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".config/cosmic"));
    }
    roots.push(std::path::PathBuf::from("/usr/share/cosmic"));

    // Prefer user customizations, and within each location prefer the newest config
    // schema. COSMIC currently has both v1 and v2 theme files on some installations.
    for root in roots {
        for version in versions {
            let path = root
                .join(format!("com.system76.CosmicTheme.{mode}"))
                .join(version)
                .join(key);
            if let Ok(value) = std::fs::read_to_string(path) {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_cosmic_frosted(dark: bool) -> Option<bool> {
    if detect_desktop_look() != "cosmic" {
        return None;
    }

    let value = cosmic_theme_config_value_for_mode(dark, "frosted", &["v2"])
        .or_else(|| cosmic_theme_config_value_for_mode(dark, "is_frosted", &["v1"]))?;
    match value
        .trim()
        .trim_matches(|ch| ch == '"' || ch == '\'')
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "some(true)" => Some(true),
        "false" | "some(false)" => Some(false),
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn detect_cosmic_frosted(_dark: bool) -> Option<bool> {
    None
}

/// COSMIC's theme values are serialized configuration files, not an ABI. Parse the
/// common RON/numeric color representation conservatively; unknown formats fall back
/// to the standard blue accent rather than affecting startup.
#[cfg(target_os = "linux")]
fn parse_cosmic_color(value: &str) -> Option<egui::Color32> {
    let value = value.trim();
    if let Some(hex_start) = value.find('#') {
        let hex = value[hex_start + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect::<String>();
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(egui::Color32::from_rgb(r, g, b));
        }
    }

    let mut numbers = Vec::new();
    let mut token = String::new();
    for ch in value.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            token.push(ch);
        } else if !token.is_empty() {
            if let Ok(number) = token.parse::<f32>() {
                if number.is_finite() {
                    numbers.push(number);
                }
            }
            token.clear();
        }
    }

    if numbers.len() < 3 {
        return None;
    }
    let components = &numbers[..3];
    let scale = if components.iter().all(|v| (0.0..=1.0).contains(v)) {
        255.0
    } else {
        1.0
    };
    let to_u8 = |v: f32| (v * scale).round().clamp(0.0, 255.0) as u8;
    Some(egui::Color32::from_rgb(
        to_u8(components[0]),
        to_u8(components[1]),
        to_u8(components[2]),
    ))
}

pub fn apply_polished_style_with_options(
    ctx: &egui::Context,
    accent_name: &str,
    ui_scale: f32,
    text_scale: f32,
) {
    ctx.set_zoom_factor(ui_scale.clamp(0.75, 1.50));
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("ui_text_scale_factor"),
            text_scale.clamp(0.80, 1.40),
        )
    });

    let mut style = (*ctx.style()).clone();
    let look_profile = current_look_profile(ctx);
    // Windows 98 was a light, classic-gray interface regardless of modern system theme.
    let dark = style.visuals.dark_mode;
    style.visuals.dark_mode = dark;
    let visuals = &mut style.visuals;

    // Automatic accent follows the actual desktop where it can be read without
    // linking that desktop's toolkit. GNOME publishes a named accent through
    // gsettings; COSMIC stores its active accent in its ordinary theme config files.
    // Other desktops and older installations fall back to a sensible profile default.
    let automatic_accent = match look_profile {
        "gnome" => detect_gnome_accent().unwrap_or("blue"),
        "cosmic" => "blue",
        "win98" => "win98blue",
        _ => "teal",
    };
    let use_desktop_accent = accent_name == "auto";
    let accent_name = if use_desktop_accent {
        automatic_accent
    } else {
        accent_name
    };
    let cosmic_accent = if use_desktop_accent && look_profile == "cosmic" {
        detect_cosmic_accent(dark)
    } else {
        None
    };

    let custom = accent_name.strip_prefix("custom:#").and_then(|hex| {
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(egui::Color32::from_rgb(r, g, b))
    });

    let (light_accent, dark_accent) = match accent_name {
        "blue" => (
            egui::Color32::from_rgb(53, 132, 228),
            egui::Color32::from_rgb(120, 174, 245),
        ),
        "win98blue" => (
            egui::Color32::from_rgb(0, 0, 128),
            egui::Color32::from_rgb(46, 107, 189),
        ),
        "purple" => (
            egui::Color32::from_rgb(145, 65, 172),
            egui::Color32::from_rgb(194, 137, 220),
        ),
        "orange" => (
            egui::Color32::from_rgb(237, 91, 0),
            egui::Color32::from_rgb(255, 155, 91),
        ),
        "rose" => (
            egui::Color32::from_rgb(213, 97, 153),
            egui::Color32::from_rgb(241, 151, 194),
        ),
        "green" => (
            egui::Color32::from_rgb(58, 148, 74),
            egui::Color32::from_rgb(120, 205, 132),
        ),
        "yellow" => (
            egui::Color32::from_rgb(200, 136, 0),
            egui::Color32::from_rgb(245, 190, 77),
        ),
        "red" => (
            egui::Color32::from_rgb(230, 45, 66),
            egui::Color32::from_rgb(255, 119, 131),
        ),
        "teal" => (
            egui::Color32::from_rgb(0, 133, 123),
            egui::Color32::from_rgb(93, 220, 202),
        ),
        _ => (
            egui::Color32::from_rgb(0, 133, 123),
            egui::Color32::from_rgb(93, 220, 202),
        ),
    };
    let accent = custom
        .or(cosmic_accent)
        .unwrap_or(if dark { dark_accent } else { light_accent });
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(MAIN_ACCENT_COLOR_ID), accent);
    });
    let accent_soft = if dark {
        accent.linear_multiply(0.34)
    } else {
        tint_with_white(accent, 0.16)
    };
    let panel = if dark {
        egui::Color32::from_rgb(27, 29, 32)
    } else {
        egui::Color32::from_rgb(244, 245, 247)
    };
    let surface = if dark {
        egui::Color32::from_rgb(36, 38, 42)
    } else {
        egui::Color32::from_rgb(255, 255, 255)
    };
    let hover = if dark {
        accent.linear_multiply(0.24).gamma_multiply(0.8)
    } else {
        tint_with_white(accent, 0.10)
    };
    let active = if dark {
        accent.linear_multiply(0.42)
    } else {
        tint_with_white(accent, 0.22)
    };
    let border = if dark {
        egui::Color32::from_rgb(67, 70, 74)
    } else {
        egui::Color32::from_rgb(214, 216, 219)
    };

    visuals.panel_fill = panel;
    visuals.window_fill = surface;
    visuals.extreme_bg_color = if dark {
        egui::Color32::from_rgb(15, 17, 19)
    } else {
        egui::Color32::from_rgb(255, 255, 255)
    };
    visuals.faint_bg_color = if dark {
        egui::Color32::from_rgb(32, 34, 36)
    } else {
        egui::Color32::from_rgb(238, 238, 238)
    };
    visuals.code_bg_color = if dark {
        egui::Color32::from_rgb(23, 25, 27)
    } else {
        egui::Color32::from_rgb(238, 239, 240)
    };
    // Clear any global text override; foregrounds are derived from each widget state.
    visuals.override_text_color = None;
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent_soft;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 5],
        blur: 18,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 110 } else { 26 }),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 100 } else { 22 }),
    };

    let text_color = visuals.text_color();
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.inactive.bg_fill = surface;
    visuals.widgets.inactive.weak_bg_fill = panel;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.inactive.expansion = 0.0;
    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, text_color);
    visuals.widgets.hovered.expansion = 1.0;
    visuals.widgets.active.bg_fill = active;
    visuals.widgets.active.weak_bg_fill = active;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.active.expansion = 0.0;
    visuals.widgets.open.bg_fill = accent_soft;
    visuals.widgets.open.weak_bg_fill = accent_soft;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, accent);

    let radius = match look_profile {
        "cosmic" => 10,
        "gnome" => 8,
        "win98" => 0,
        _ => 10,
    };
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(radius);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(radius);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(radius);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(radius);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(radius);
    // Popup/window frames use separate radii from widget controls. Set these too
    // so ComboBox menus, context menus, and egui Windows remain square in Win98 mode.
    let surface_radius = if look_profile == "win98" {
        egui::CornerRadius::ZERO
    } else {
        egui::CornerRadius::same(radius)
    };
    visuals.window_corner_radius = surface_radius;
    visuals.menu_corner_radius = surface_radius;

    // Give COSMIC and GNOME their own surface hierarchy and spacing conventions.
    // These are faithfully styled egui widgets, not embedded native GTK/libcosmic
    // widgets; genuine compositor backdrop blur is not exposed through eframe.
    match look_profile {
        "win98" => {
            // Classic Windows 98 system colors in light mode, plus a genuinely
            // dark variant when the user selects Dark / Follow System.
            let gray = if dark {
                egui::Color32::from_rgb(38, 40, 43)
            } else {
                egui::Color32::from_rgb(192, 192, 192)
            };
            let face = if dark {
                egui::Color32::from_rgb(48, 50, 54)
            } else {
                egui::Color32::from_rgb(212, 208, 200)
            };
            let white = if dark {
                egui::Color32::from_rgb(229, 231, 234)
            } else {
                egui::Color32::WHITE
            };
            let black = if dark {
                egui::Color32::from_rgb(9, 10, 12)
            } else {
                egui::Color32::BLACK
            };
            let body_text = if dark {
                egui::Color32::from_rgb(238, 240, 243)
            } else {
                egui::Color32::BLACK
            };
            let inner = if dark {
                egui::Color32::from_rgb(25, 27, 30)
            } else {
                egui::Color32::WHITE
            };
            let dark_gray = if dark {
                egui::Color32::from_rgb(82, 85, 90)
            } else {
                egui::Color32::from_rgb(128, 128, 128)
            };
            let hover_bg = if dark {
                accent.linear_multiply(0.48)
            } else {
                accent
            };
            let active_bg = if dark {
                accent.linear_multiply(0.34)
            } else {
                face
            };
            visuals.panel_fill = gray;
            visuals.window_fill = gray;
            visuals.extreme_bg_color = inner;
            visuals.faint_bg_color = face;
            visuals.code_bg_color = inner;
            visuals.text_edit_bg_color = Some(inner);
            visuals.window_stroke = egui::Stroke::new(1.0_f32, black);
            visuals.window_shadow = egui::epaint::Shadow {
                offset: [0, 0],
                blur: 0,
                spread: 0,
                color: egui::Color32::TRANSPARENT,
            };
            visuals.popup_shadow = visuals.window_shadow;
            visuals.selection.bg_fill = if dark { hover_bg } else { accent };
            visuals.selection.stroke = egui::Stroke::new(1.0_f32, white);
            visuals.hyperlink_color = if dark {
                tint_with_white(accent, 0.55)
            } else {
                egui::Color32::from_rgb(0, 0, 238)
            };
            visuals.widgets.noninteractive.bg_fill = gray;
            visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, dark_gray);
            visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, body_text);
            visuals.widgets.inactive.bg_fill = gray;
            visuals.widgets.inactive.weak_bg_fill = gray;
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, dark_gray);
            visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, body_text);
            visuals.widgets.hovered.bg_fill = hover_bg;
            visuals.widgets.hovered.weak_bg_fill = hover_bg;
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
            visuals.widgets.hovered.fg_stroke = egui::Stroke::new(
                1.0_f32,
                if dark {
                    egui::Color32::WHITE
                } else {
                    contrast_foreground(hover_bg)
                },
            );
            visuals.widgets.active.bg_fill = active_bg;
            visuals.widgets.active.weak_bg_fill = active_bg;
            visuals.widgets.active.bg_stroke =
                egui::Stroke::new(1.0_f32, if dark { accent } else { black });
            visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, body_text);
            visuals.widgets.open.bg_fill = if dark { hover_bg } else { face };
            visuals.widgets.open.weak_bg_fill = visuals.widgets.open.bg_fill;
            visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, dark_gray);
            visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, body_text);
            style.spacing.item_spacing = egui::vec2(4.0, 4.0);
            style.spacing.button_padding = egui::vec2(5.0, 3.0);
            style.spacing.interact_size = egui::vec2(22.0, 24.0);
            style.spacing.menu_margin = egui::Margin::same(2);
            style.spacing.window_margin = egui::Margin::same(4);
        }
        "cosmic" => {
            visuals.panel_fill = if dark {
                egui::Color32::from_rgb(30, 30, 32)
            } else {
                egui::Color32::from_rgb(246, 246, 248)
            };
            visuals.window_fill = if dark {
                egui::Color32::from_rgb(38, 39, 42)
            } else {
                egui::Color32::from_rgb(252, 252, 253)
            };
            visuals.extreme_bg_color = if dark {
                egui::Color32::from_rgb(23, 24, 26)
            } else {
                egui::Color32::from_rgb(255, 255, 255)
            };
            visuals.faint_bg_color = if dark {
                egui::Color32::from_rgb(45, 46, 49)
            } else {
                egui::Color32::from_rgb(235, 236, 239)
            };
            visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
            visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(
                1.0_f32,
                if dark {
                    egui::Color32::from_rgb(58, 60, 64)
                } else {
                    egui::Color32::from_rgb(220, 221, 225)
                },
            );
            visuals.widgets.inactive.bg_fill = visuals.window_fill;
            visuals.widgets.inactive.weak_bg_fill = visuals.panel_fill;
            style.spacing.item_spacing = egui::vec2(10.0, 9.0);
            style.spacing.button_padding = egui::vec2(13.0, 8.0);
            style.spacing.interact_size = egui::vec2(32.0, 34.0);
            style.spacing.menu_margin = egui::Margin::same(10);
            style.spacing.window_margin = egui::Margin::same(16);
        }
        "gnome" => {
            // Adwaita's view, window, and headerbar layers are neutral and opaque;
            // keep accent color mainly for selected/active controls.
            visuals.panel_fill = if dark {
                egui::Color32::from_rgb(48, 48, 48)
            } else {
                egui::Color32::from_rgb(246, 245, 244)
            };
            visuals.window_fill = if dark {
                egui::Color32::from_rgb(36, 36, 36)
            } else {
                egui::Color32::from_rgb(255, 255, 255)
            };
            visuals.extreme_bg_color = if dark {
                egui::Color32::from_rgb(30, 30, 30)
            } else {
                egui::Color32::from_rgb(255, 255, 255)
            };
            visuals.faint_bg_color = if dark {
                egui::Color32::from_rgb(56, 56, 56)
            } else {
                egui::Color32::from_rgb(241, 240, 239)
            };
            let adwaita_border = if dark {
                egui::Color32::from_rgb(75, 75, 75)
            } else {
                egui::Color32::from_rgb(211, 209, 207)
            };
            visuals.window_stroke = egui::Stroke::new(1.0_f32, adwaita_border);
            visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
            visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, adwaita_border);
            visuals.widgets.inactive.bg_fill = if dark {
                egui::Color32::from_rgb(61, 61, 61)
            } else {
                egui::Color32::from_rgb(250, 249, 248)
            };
            visuals.widgets.inactive.weak_bg_fill = visuals.panel_fill;
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, adwaita_border);
            visuals.widgets.hovered.bg_fill = if dark {
                egui::Color32::from_rgb(73, 73, 73)
            } else {
                egui::Color32::from_rgb(235, 234, 233)
            };
            visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, adwaita_border);
            visuals.widgets.active.bg_fill = accent_soft;
            visuals.widgets.active.weak_bg_fill = accent_soft;
            visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
            visuals.widgets.open.bg_fill = accent_soft;
            visuals.widgets.open.weak_bg_fill = accent_soft;
            visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, accent);
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.interact_size = egui::vec2(30.0, 32.0);
            style.spacing.menu_margin = egui::Margin::same(8);
            style.spacing.window_margin = egui::Margin::same(12);
        }
        _ => {
            style.spacing.item_spacing = egui::vec2(9.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.interact_size = egui::vec2(30.0, 32.0);
            style.spacing.menu_margin = egui::Margin::same(9);
            style.spacing.window_margin = egui::Margin::same(14);
        }
    }

    // Keep profile-specific editor/table surfaces in sync with the active palette.
    // These are assigned after profile styling so disabling glass always restores
    // the expected opaque surface instead of leaving a stale dark/default fill.
    visuals.text_edit_bg_color = Some(visuals.extreme_bg_color);

    // Give every standard egui surface an opacity appropriate to its role when
    // glass is requested. Text-entry and code surfaces stay denser for legibility;
    // hover/active states are slightly denser than passive surfaces. When glass is
    // disabled, no alpha overrides are applied and the original opaque colors win.
    if os_background_blur_enabled(ctx) && look_profile != "win98" {
        visuals.panel_fill = with_alpha(visuals.panel_fill, 192);
        // Popup/window surfaces deliberately stay much more opaque than the main
        // panels: dropdowns and menus must remain readable over arbitrary images.
        visuals.window_fill = with_alpha(visuals.window_fill, 250);
        visuals.extreme_bg_color = with_alpha(visuals.extreme_bg_color, 250);
        visuals.faint_bg_color = with_alpha(visuals.faint_bg_color, 232);
        visuals.code_bg_color = with_alpha(visuals.code_bg_color, 242);
        if let Some(text_edit_bg) = visuals.text_edit_bg_color {
            visuals.text_edit_bg_color = Some(with_alpha(text_edit_bg, 250));
        }
        visuals.selection.bg_fill = with_alpha(visuals.selection.bg_fill, 240);
        visuals.widgets.noninteractive.bg_fill =
            with_alpha(visuals.widgets.noninteractive.bg_fill, 224);
        visuals.widgets.inactive.bg_fill = with_alpha(visuals.widgets.inactive.bg_fill, 244);
        visuals.widgets.inactive.weak_bg_fill =
            with_alpha(visuals.widgets.inactive.weak_bg_fill, 240);
        visuals.widgets.hovered.bg_fill = with_alpha(visuals.widgets.hovered.bg_fill, 248);
        visuals.widgets.hovered.weak_bg_fill =
            with_alpha(visuals.widgets.hovered.weak_bg_fill, 248);
        visuals.widgets.active.bg_fill = with_alpha(visuals.widgets.active.bg_fill, 250);
        visuals.widgets.active.weak_bg_fill = with_alpha(visuals.widgets.active.weak_bg_fill, 250);
        visuals.widgets.open.bg_fill = with_alpha(visuals.widgets.open.bg_fill, 248);
        visuals.widgets.open.weak_bg_fill = with_alpha(visuals.widgets.open.weak_bg_fill, 248);
    }

    // Evaluate links against the effective backdrop-aware panel surface after
    // opacity has been applied, rather than the uncomposited RGB components.
    visuals.hyperlink_color = accessible_accent(accent, visuals.panel_fill);

    // Selected egui widgets render their text using selection.stroke.color rather
    // than WidgetVisuals::fg_stroke. Recompute it against the actual selection fill
    // so light-theme yellow/orange accents do not produce low-contrast selected text.
    // Use one theme-aware foreground family for all standard widget states. The old
    // per-background heuristic could turn button/header text black when an accent
    // hover was bright, even in dark mode. Instead, dark-mode accents are subdued
    // surfaces and all normal widget text stays light; light-mode controls stay dark.
    let body_text = if dark {
        egui::Color32::from_rgb(237, 239, 242)
    } else {
        egui::Color32::from_rgb(32, 34, 37)
    };
    let weak_text = if dark {
        egui::Color32::from_rgb(190, 196, 204)
    } else {
        egui::Color32::from_rgb(82, 86, 92)
    };
    if dark {
        visuals.selection.bg_fill = darken_surface_for_text(visuals.selection.bg_fill, body_text);
        visuals.widgets.hovered.bg_fill =
            darken_surface_for_text(visuals.widgets.hovered.bg_fill, body_text);
        visuals.widgets.hovered.weak_bg_fill =
            darken_surface_for_text(visuals.widgets.hovered.weak_bg_fill, body_text);
        visuals.widgets.active.bg_fill =
            darken_surface_for_text(visuals.widgets.active.bg_fill, body_text);
        visuals.widgets.active.weak_bg_fill =
            darken_surface_for_text(visuals.widgets.active.weak_bg_fill, body_text);
        visuals.widgets.open.bg_fill =
            darken_surface_for_text(visuals.widgets.open.bg_fill, body_text);
        visuals.widgets.open.weak_bg_fill =
            darken_surface_for_text(visuals.widgets.open.weak_bg_fill, body_text);
    }
    visuals.selection.stroke = egui::Stroke::new(
        visuals.selection.stroke.width,
        if dark {
            body_text
        } else {
            accessible_accent(accent, visuals.selection.bg_fill)
        },
    );
    set_widget_foreground(&mut visuals.widgets.noninteractive, body_text);
    set_widget_foreground(&mut visuals.widgets.inactive, body_text);
    set_widget_foreground(&mut visuals.widgets.hovered, body_text);
    set_widget_foreground(&mut visuals.widgets.active, body_text);
    set_widget_foreground(&mut visuals.widgets.open, body_text);
    if look_profile == "win98" {
        // Preserve the selected Main color instead of hard-coding classic navy
        // here. The accent has already been resolved from settings (Windows 98
        // blue by default), and dark mode uses its darker surface variants above.
        let selected_fill = visuals.selection.bg_fill;
        let hover_fill = visuals.widgets.hovered.bg_fill;
        let active_fill = visuals.widgets.active.bg_fill;
        let open_fill = visuals.widgets.open.bg_fill;
        visuals.selection.stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                body_text
            } else {
                contrast_foreground(selected_fill)
            },
        );
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                body_text
            } else {
                contrast_foreground(hover_fill)
            },
        );
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                tint_with_white(accent, 0.25)
            } else {
                egui::Color32::BLACK
            },
        );
        visuals.widgets.active.fg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                body_text
            } else {
                contrast_foreground(active_fill)
            },
        );
        visuals.widgets.open.fg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark {
                body_text
            } else {
                contrast_foreground(open_fill)
            },
        );
    }
    visuals.weak_text_color = Some(accessible_accent(weak_text, visuals.panel_fill));
    visuals.weak_text_alpha = 1.0;

    // Widget-level stroke is also visible on the slider handle; BorderedSlider paints
    // the rail outline explicitly because egui's built-in rail ignores this stroke.
    let slider_rail = if dark {
        egui::Color32::from_rgb(112, 117, 124)
    } else {
        egui::Color32::from_rgb(126, 131, 138)
    };
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.35_f32, slider_rail);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, body_text);
    // Preserve denser, near-opaque fill for the slider rail and knob while leaving
    // the main app panels transparent enough to show the OS backdrop.
    visuals.widgets.inactive.bg_fill = with_alpha(visuals.widgets.inactive.bg_fill, 255);
    visuals.widgets.inactive.weak_bg_fill = with_alpha(visuals.widgets.inactive.weak_bg_fill, 255);
    visuals.widgets.hovered.bg_fill = with_alpha(visuals.widgets.hovered.bg_fill, 255);
    visuals.widgets.active.bg_fill = with_alpha(visuals.widgets.active.bg_fill, 255);
    visuals.widgets.open.bg_fill = with_alpha(visuals.widgets.open.bg_fill, 255);
    visuals.selection.bg_fill = with_alpha(visuals.selection.bg_fill, 255);
    let hovered_text = if look_profile == "win98" && !dark {
        contrast_foreground(visuals.widgets.hovered.bg_fill)
    } else {
        body_text
    };
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.35_f32, hovered_text);
    visuals.override_text_color = None;

    let text_scale = text_scale.clamp(0.80, 1.40);
    for (text_style, font_id) in style.text_styles.iter_mut() {
        let base_size = if *text_style == egui::TextStyle::Small {
            10.0
        } else if *text_style == egui::TextStyle::Body {
            14.0
        } else if *text_style == egui::TextStyle::Button {
            14.0
        } else if *text_style == egui::TextStyle::Heading {
            18.0
        } else if *text_style == egui::TextStyle::Monospace {
            14.0
        } else {
            font_id.size
        };
        font_id.size = base_size * text_scale;
    }

    ctx.set_style(style);
}

/// Draw a compact switch control and return true when its value changes.
pub fn switch(ui: &mut egui::Ui, value: &mut bool, label: impl Into<egui::WidgetText>) -> bool {
    let label = label.into();
    let mut changed = false;
    ui.horizontal(|ui| {
        let desired = egui::vec2(38.0, 22.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
        if response.clicked() {
            *value = !*value;
            changed = true;
        }
        let visuals = ui.visuals();
        let win98 = current_look_profile(ui.ctx()) == "win98";
        let on = *value;
        let track = if on {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let stroke = if on {
            visuals.selection.stroke
        } else {
            visuals.widgets.inactive.bg_stroke
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(if win98 { 0 } else { 11 }),
            track,
            stroke,
            egui::StrokeKind::Inside,
        );
        let x = if on {
            rect.right() - 11.0
        } else {
            rect.left() + 11.0
        };
        let knob = if on {
            visuals.selection.stroke.color
        } else {
            visuals.widgets.inactive.fg_stroke.color
        };
        if win98 {
            let knob_rect = egui::Rect::from_center_size(
                egui::pos2(x, rect.center().y),
                egui::vec2(10.0, 12.0),
            );
            ui.painter()
                .rect_filled(knob_rect, egui::CornerRadius::ZERO, knob);
            ui.painter().rect_stroke(
                knob_rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0_f32, visuals.widgets.inactive.bg_stroke.color),
                egui::StrokeKind::Inside,
            );
        } else {
            ui.painter()
                .circle_filled(egui::pos2(x, rect.center().y), 7.0, knob);
        }
        if ui
            .add(egui::Label::new(label).sense(egui::Sense::click()))
            .clicked()
        {
            *value = !*value;
            changed = true;
        }
    });
    changed
}

/// Scale explicit RichText/FontId sizes alongside egui's default text styles.
pub fn scaled_text_size(ctx: &egui::Context, base: f32) -> f32 {
    let factor = ctx
        .data(|data| data.get_temp::<f32>(egui::Id::new("ui_text_scale_factor")))
        .unwrap_or(1.0);
    base * factor
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    #[test]
    fn foreground_selection_meets_high_contrast_for_light_and_dark_surfaces() {
        for background in [
            egui::Color32::from_rgb(246, 246, 248),
            egui::Color32::from_rgb(30, 30, 32),
            egui::Color32::from_rgb(58, 120, 180),
        ] {
            let foreground = contrast_foreground(background);
            assert!(contrast_ratio(foreground, background) >= 4.5);
        }
    }

    #[test]
    fn low_contrast_links_are_adjusted() {
        let background = egui::Color32::WHITE;
        let link = accessible_accent(egui::Color32::from_rgb(100, 180, 255), background);
        assert!(contrast_ratio(link, background) >= 4.5);
    }

    #[test]
    fn light_accent_surfaces_keep_readable_foregrounds() {
        for accent in [
            egui::Color32::from_rgb(53, 132, 228),
            egui::Color32::from_rgb(237, 91, 0),
            egui::Color32::from_rgb(200, 136, 0),
        ] {
            let surface = tint_with_white(accent, 0.16);
            assert!(contrast_ratio(contrast_foreground(surface), surface) >= 4.5);
        }
    }

    #[test]
    fn selected_text_accent_keeps_contrast_on_tinted_surfaces() {
        for (accent, surface) in [
            (
                egui::Color32::from_rgb(200, 136, 0),
                tint_with_white(egui::Color32::from_rgb(200, 136, 0), 0.16),
            ),
            (
                egui::Color32::from_rgb(53, 132, 228),
                tint_with_white(egui::Color32::from_rgb(53, 132, 228), 0.16),
            ),
            (
                egui::Color32::from_rgb(120, 174, 245),
                egui::Color32::from_rgb(40, 42, 45),
            ),
        ] {
            let selected_text = accessible_accent(accent, surface);
            assert!(contrast_ratio(selected_text, surface) >= 4.5);
        }
    }

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
