use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    /// Calculate the fit zoom for the image
    pub fn calculate_fit_zoom(&mut self, texture_size: egui::Vec2, available: egui::Vec2) -> bool {
        // Check if we should actually apply fit
        let should_fit = {
            let width = texture_size.x;
            let height = texture_size.y;
            let ratio = if width > height {
                height / width
            } else {
                width / height
            };
            // Only apply fit if ratio is >= 0.1 (not extreme aspect ratio)
            ratio >= 0.1
        };
        let zoom_x = available.x / texture_size.x;
        let zoom_y = available.y / texture_size.y;
        if should_fit {
            let fit_zoom = zoom_x.min(zoom_y).min(1.0);
            self.zoom = fit_zoom;
            self.pan = egui::Vec2::ZERO;
            true // Applied fit all sides
        } else {
            // For extreme ratios
            let max_fit_zoom = zoom_x.max(zoom_y).min(1.0);
            self.zoom = max_fit_zoom;
            let snap_to_top = (texture_size.y / 2.0) - (available.y / 2.0) / max_fit_zoom;
            self.pan = egui::Vec2::new(0.0, snap_to_top);
            false // fit to one side
        }
    }

    /// Get the image rectangle centered in available space
    pub fn get_image_rect(&self, texture_size: egui::Vec2, center: egui::Pos2) -> egui::Rect {
        let display_size = texture_size * self.zoom;
        egui::Rect::from_center_size(center + self.pan * self.zoom, display_size)
    }
}

/*
pub fn render_drag_area(ctx: &egui::Context, ui: &mut egui::Ui) {
    let drag_response = ui.interact(
        ui.max_rect(),
        ui.id().with("window_drag"),
        egui::Sense::click_and_drag(),
    );

    if drag_response.double_clicked() {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}
 */

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes_f = bytes as f64;

    if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.2} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).size(14.0))
            .min_size(egui::vec2(24.0, 24.0))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
    )
    .on_hover_text(tooltip)
}

/// Paint a scale-independent sidebar/drawer glyph using vector primitives so it
/// does not depend on a particular symbol font being installed. `show_sidebar`
/// points the arrow into the window; `false` points it back toward the left edge.
pub fn paint_sidebar_drawer_icon(ui: &egui::Ui, rect: egui::Rect, show_sidebar: bool) {
    let painter = ui.painter();
    let color = ui.visuals().text_color();
    let stroke = egui::Stroke::new(1.4_f32, color);
    let frame = rect.shrink2(egui::vec2(6.0, 8.0));
    let corner_radius = if crate::ui_theme::current_look_profile(ui.ctx()) == "win98" {
        egui::CornerRadius::ZERO
    } else {
        egui::CornerRadius::same(2)
    };
    painter.rect_stroke(frame, corner_radius, stroke, egui::StrokeKind::Inside);

    let divider_x = frame.left() + frame.width() * 0.32;
    painter.line_segment(
        [
            egui::pos2(divider_x, frame.top() + 1.5),
            egui::pos2(divider_x, frame.bottom() - 1.5),
        ],
        stroke,
    );

    let center_y = frame.center().y;
    if show_sidebar {
        let start = egui::pos2(divider_x + 2.0, center_y);
        let tip = egui::pos2(frame.right() - 2.0, center_y);
        painter.line_segment([start, tip], stroke);
        painter.line_segment([egui::pos2(tip.x - 3.0, tip.y - 2.5), tip], stroke);
        painter.line_segment([egui::pos2(tip.x - 3.0, tip.y + 2.5), tip], stroke);
    } else {
        let start = egui::pos2(frame.right() - 2.0, center_y);
        let tip = egui::pos2(divider_x + 2.0, center_y);
        painter.line_segment([start, tip], stroke);
        painter.line_segment([egui::pos2(tip.x + 3.0, tip.y - 2.5), tip], stroke);
        painter.line_segment([egui::pos2(tip.x + 3.0, tip.y + 2.5), tip], stroke);
    }
}
