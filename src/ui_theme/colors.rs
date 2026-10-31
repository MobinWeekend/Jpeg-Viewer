//! Shared color resolution and contrast calculations for all UI profiles.

use eframe::egui;

pub(super) const MAIN_ACCENT_COLOR_ID: &str = "jpeg_viewer_main_accent_color";

/// The resolved accent selected by Settings > Main color. Cached with the style so
/// custom-drawn elements (such as the title bar) use the same color as egui widgets.
pub fn main_accent_color(ctx: &egui::Context) -> egui::Color32 {
    ctx.data(|data| data.get_temp::<egui::Color32>(egui::Id::new(MAIN_ACCENT_COLOR_ID)))
        .unwrap_or_else(|| ctx.style().visuals.selection.stroke.color)
}

/// Resolve the main image-view background color using the same palette rules as the canvas.
pub fn main_view_background_color(value: &str, dark: bool) -> egui::Color32 {
    if value == "light" {
        return egui::Color32::from_rgb(245, 245, 245);
    }
    if value == "theme" {
        return if dark {
            egui::Color32::from_rgb(18, 18, 18)
        } else {
            egui::Color32::from_rgb(238, 238, 238)
        };
    }
    if let Some(hex) = value.strip_prefix("custom:#") {
        if hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return egui::Color32::from_rgb(r, g, b);
            }
        }
    }
    if dark {
        egui::Color32::from_rgb(18, 18, 18)
    } else {
        egui::Color32::from_rgb(38, 38, 38)
    }
}

pub(crate) fn with_alpha(color: egui::Color32, alpha: u8) -> egui::Color32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

pub(super) fn relative_luminance(color: egui::Color32) -> f32 {
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

pub(super) fn contrast_ratio(foreground: egui::Color32, background: egui::Color32) -> f32 {
    let fg = relative_luminance(foreground);
    let bg = relative_luminance(background);
    (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05)
}

/// Composite an alpha-bearing surface against the worst-case backdrop for a
/// particular foreground. If foreground text is brighter than the surface, a white
/// backdrop is the hard case; if it is darker, a black backdrop is the hard case.
/// This is conservative for glass surfaces where the actual desktop backdrop varies.
pub(super) fn contrast_surface_for_text(
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
pub(super) fn tint_with_white(color: egui::Color32, color_strength: f32) -> egui::Color32 {
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
pub(super) fn accessible_accent(foreground: egui::Color32, background: egui::Color32) -> egui::Color32 {
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

pub(super) fn set_widget_foreground(widget: &mut egui::style::WidgetVisuals, foreground: egui::Color32) {
    // Keep all standard widget states aligned with the selected theme. Rather than
    // changing text to black on a bright accent hover (which looks broken in dark
    // mode), accent surfaces are tuned to remain dark enough for light foregrounds.
    widget.fg_stroke = egui::Stroke::new(widget.fg_stroke.width, foreground);
}

/// Keep a surface dark enough for the chosen light foreground. This is especially
/// useful for selected/hovered controls in dark mode when the user's accent is bright.
pub(super) fn darken_surface_for_text(surface: egui::Color32, foreground: egui::Color32) -> egui::Color32 {
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
}

