//! Shared custom Masonry mark and buttons.
//!
//! The mark is an editable SVG asset rather than a font glyph, so it remains
//! consistent across platforms and font configurations.
use eframe::egui;
use super::svg_icons::{paint_svg_icon, SvgIcon};

pub(crate) fn draw_masonry_icon(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    paint_svg_icon(ui, rect, SvgIcon::Masonry, color);
}

/// Draw the application's Home mark from its editable SVG asset.
pub(crate) fn draw_home_icon(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    paint_svg_icon(ui, rect, SvgIcon::Home, color);
}

/// Render a compact button that carries the shared Masonry icon.
/// `flat` is used for menu rows and title-bar chrome; ordinary buttons retain
/// the current Look profile's standard widget surface.
pub(crate) fn masonry_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    min_width: f32,
    flat: bool,
    hover_highlight: bool,
) -> egui::Response {
    masonry_button_impl(ui, label, enabled, min_width, flat, hover_highlight)
}

fn masonry_button_impl(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    min_width: f32,
    flat: bool,
    hover_highlight: bool,
) -> egui::Response {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let text_color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let galley = if label.is_empty() {
        None
    } else {
        Some(
            ui.painter()
                .layout_no_wrap(label.to_owned(), font_id, text_color),
        )
    };
    let icon_size = 16.0;
    let content_width = galley.as_ref().map_or(0.0, |galley| galley.size().x);
    let desired_width = (content_width + icon_size + 25.0).max(min_width.max(28.0));
    let width = desired_width.min(ui.available_width().max(1.0));
    let height = if flat {
        ui.available_height().min(32.0).max(30.0)
    } else {
        ui.available_height().min(38.0).max(30.0)
    };
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);

    let visuals = if !enabled {
        &ui.visuals().widgets.noninteractive
    } else if response.is_pointer_button_down_on() {
        &ui.visuals().widgets.active
    } else if response.hovered() && hover_highlight {
        &ui.visuals().widgets.hovered
    } else {
        &ui.visuals().widgets.inactive
    };
    let show_surface =
        !flat || (hover_highlight && response.hovered()) || response.is_pointer_button_down_on();
    if show_surface && visuals.bg_fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, visuals.bg_fill);
    }
    if !flat {
        ui.painter()
            .rect_stroke(rect, 6.0, visuals.bg_stroke, egui::StrokeKind::Inside);
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(icon_size, icon_size),
    );
    let icon_color = if enabled {
        visuals.fg_stroke.color
    } else {
        text_color
    };
    draw_masonry_icon(ui, icon_rect.intersect(rect), icon_color);
    if let Some(galley) = galley {
        ui.painter().galley(
            egui::pos2(
                icon_rect.right() + 7.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text_color,
        );
    }
    response
}

/// Render a compact Masonry mode toggle using the same custom glyph as the
/// larger Masonry actions. The selected state remains visible while Masonry is
/// active, without representing Masonry as a separate workspace tab.
pub(crate) fn masonry_toggle_button(
    ui: &mut egui::Ui,
    active: bool,
    enabled: bool,
    size: f32,
) -> egui::Response {
    let size = size.max(24.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), sense);
    let visuals = ui.visuals();
    let fill = if !enabled {
        visuals.widgets.noninteractive.bg_fill
    } else if active {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, fill);
    }
    if active {
        ui.painter().rect_stroke(
            rect,
            6.0,
            visuals.selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    let icon_color = if enabled {
        if active {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        }
    } else {
        visuals.weak_text_color()
    };
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(19.0, 19.0));
    draw_masonry_icon(ui, icon_rect.intersect(rect), icon_color);
    response
}
