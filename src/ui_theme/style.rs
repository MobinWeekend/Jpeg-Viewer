//! egui visual-style construction and palette application.

use eframe::egui;
use super::backdrop::os_background_blur_enabled;
use super::colors::{
    accessible_accent, contrast_foreground, darken_surface_for_text, set_widget_foreground,
    tint_with_white, with_alpha, MAIN_ACCENT_COLOR_ID,
};
use super::profiles::current_look_profile;
use super::platform::{detect_cosmic_accent, detect_gnome_accent};

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
