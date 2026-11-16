//! Shared custom Masonry mark and buttons.
//!
//! The mark is a small staggered tile layout rather than a font glyph so it
//! remains consistent across platforms and font configurations.
use eframe::egui;

pub(crate) fn draw_masonry_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    color: egui::Color32,
) {
    let scale = (rect.width().min(rect.height()) / 20.0).max(0.1);
    let origin = rect.center() - egui::vec2(9.5 * scale, 9.0 * scale);
    let rounding = egui::CornerRadius::same((1.1 * scale).round() as u8);
    // Three uneven columns make the mark read as a masonry image layout.
    let tiles = [
        (0.0, 0.0, 5.0, 8.0),
        (0.0, 10.0, 5.0, 8.0),
        (7.0, 0.0, 5.0, 5.0),
        (7.0, 7.0, 5.0, 11.0),
        (14.0, 0.0, 5.0, 10.0),
        (14.0, 12.0, 5.0, 6.0),
    ];
    for (x, y, width, height) in tiles {
        let tile = egui::Rect::from_min_size(
            origin + egui::vec2(x * scale, y * scale),
            egui::vec2(width * scale, height * scale),
        );
        painter.rect_filled(tile, rounding, color);
    }
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
) -> egui::Response {
    masonry_button_impl(ui, label, enabled, min_width, flat, true)
}

/// Render a flat Masonry button without hover highlighting, for toolbar chrome
/// that should behave like the hamburger control while remaining clickable.
pub(crate) fn masonry_button_quiet(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    min_width: f32,
    flat: bool,
) -> egui::Response {
    masonry_button_impl(ui, label, enabled, min_width, flat, false)
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
        Some(ui.painter().layout_no_wrap(label.to_owned(), font_id, text_color))
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
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
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
    let show_surface = !flat || (hover_highlight && response.hovered())
        || response.is_pointer_button_down_on();
    if show_surface && visuals.bg_fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, visuals.bg_fill);
    }
    if !flat {
        ui.painter().rect_stroke(
            rect,
            6.0,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(icon_size, icon_size),
    );
    let icon_color = if enabled { visuals.fg_stroke.color } else { text_color };
    draw_masonry_icon(&ui.painter().with_clip_rect(rect), icon_rect, icon_color);
    if let Some(galley) = galley {
        ui.painter().galley(
            egui::pos2(icon_rect.right() + 7.0, rect.center().y - galley.size().y * 0.5),
            galley,
            text_color,
        );
    }
    response
}
