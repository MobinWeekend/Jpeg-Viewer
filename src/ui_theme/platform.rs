//! Platform-specific desktop detection, accent settings, and font loading.

use eframe::egui;

/// Returns the desktop profile reported by the current Linux desktop session.
/// COSMIC is checked first because sessions may expose more than one desktop name.
pub(super) fn detect_desktop_look() -> &'static str {
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
        egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoEmoji-Regular.ttf")).into(),
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

#[cfg(target_os = "linux")]
pub(super) fn detect_gnome_accent() -> Option<&'static str> {
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
pub(super) fn detect_gnome_accent() -> Option<&'static str> {
    None
}

#[cfg(target_os = "linux")]
pub(super) fn detect_cosmic_accent(dark: bool) -> Option<egui::Color32> {
    if detect_desktop_look() != "cosmic" {
        return None;
    }

    let value = cosmic_theme_config_value_for_mode(dark, "accent", &["v2", "v1"])?;
    parse_cosmic_color(&value)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn detect_cosmic_accent(_dark: bool) -> Option<egui::Color32> {
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
pub(super) fn detect_cosmic_frosted(dark: bool) -> Option<bool> {
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
pub(super) fn detect_cosmic_frosted(_dark: bool) -> Option<bool> {
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

