//! Shared visual polish for the native egui interface.
//!
//! This intentionally keeps the user's light/dark/system preference while
//! giving controls, panels, popups, and dialogs a consistent visual language.

use eframe::egui;

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
    let dark = style.visuals.dark_mode;
    let visuals = &mut style.visuals;

    // Glass-inspired neutral surfaces with a teal accent. Keep the surfaces
    // nearly opaque so text and controls remain legible over busy photographs.
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
            egui::Color32::from_rgb(38, 112, 220),
            egui::Color32::from_rgb(111, 170, 255),
        ),
        "purple" => (
            egui::Color32::from_rgb(126, 75, 190),
            egui::Color32::from_rgb(190, 151, 255),
        ),
        "orange" => (
            egui::Color32::from_rgb(190, 94, 24),
            egui::Color32::from_rgb(255, 174, 99),
        ),
        "rose" => (
            egui::Color32::from_rgb(194, 65, 103),
            egui::Color32::from_rgb(255, 137, 171),
        ),
        _ => (
            egui::Color32::from_rgb(0, 133, 123),
            egui::Color32::from_rgb(93, 220, 202),
        ),
    };
    let accent = custom.unwrap_or(if dark { dark_accent } else { light_accent });
    let accent_soft = if dark {
        accent.linear_multiply(0.34)
    } else {
        accent.linear_multiply(0.18)
    };
    let panel = if dark {
        egui::Color32::from_rgba_unmultiplied(27, 29, 32, 246)
    } else {
        egui::Color32::from_rgba_unmultiplied(244, 245, 247, 248)
    };
    let surface = if dark {
        egui::Color32::from_rgba_unmultiplied(36, 38, 42, 250)
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 250)
    };
    // Derive interactive surfaces from the selected accent so no preset leaves
    // teal-colored hover/active states behind.
    let hover = if dark {
        accent.linear_multiply(0.24).gamma_multiply(0.8)
    } else {
        accent.linear_multiply(0.14)
    };
    let active = if dark {
        accent.linear_multiply(0.42)
    } else {
        accent.linear_multiply(0.24)
    };
    let border = if dark {
        accent.linear_multiply(0.20)
    } else {
        accent.linear_multiply(0.14)
    };

    visuals.panel_fill = panel;
    visuals.window_fill = surface;
    visuals.extreme_bg_color = if dark {
        egui::Color32::from_rgb(15, 20, 23)
    } else {
        egui::Color32::from_rgb(250, 253, 252)
    };
    visuals.faint_bg_color = if dark {
        egui::Color32::from_rgb(32, 34, 36)
    } else {
        egui::Color32::from_rgb(235, 236, 238)
    };
    visuals.code_bg_color = if dark {
        egui::Color32::from_rgb(23, 25, 27)
    } else {
        egui::Color32::from_rgb(238, 239, 240)
    };
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent_soft;
    visuals.selection.stroke = egui::Stroke::new(1.0 as f32, accent);
    visuals.window_stroke = egui::Stroke::new(1.0 as f32, border);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 5],
        blur: 18,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 125 } else { 30 }),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 112 } else { 26 }),
    };

    // Rounded, clearly differentiated button states.
    let text_color = visuals.text_color();
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0 as f32, border);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = surface;
    visuals.widgets.inactive.weak_bg_fill = panel;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0 as f32, border);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.inactive.expansion = 0.0;

    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0 as f32, accent);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5 as f32, text_color);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.hovered.expansion = 1.25;

    visuals.widgets.active.bg_fill = active;
    visuals.widgets.active.weak_bg_fill = active;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0 as f32, accent);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.active.expansion = 0.0;

    visuals.widgets.open.bg_fill = accent_soft;
    visuals.widgets.open.weak_bg_fill = accent_soft;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0 as f32, accent);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(10);

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

    style.spacing.item_spacing = egui::vec2(9.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size = egui::vec2(30.0, 32.0);
    style.spacing.menu_margin = egui::Margin::same(9);
    style.spacing.window_margin = egui::Margin::same(14);

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
            egui::CornerRadius::same(11),
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
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), 7.0, knob);
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
