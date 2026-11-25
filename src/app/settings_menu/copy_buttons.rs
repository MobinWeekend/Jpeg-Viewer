//! Copy image and path actions shown at the top of Settings.

use crate::app::types::ViewerApp;
use crate::app::ui::{paint_svg_icon, SvgIcon};
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if settings_action_button(ui, SvgIcon::ContextCopy, "Copy Image")
            .on_hover_text("Shortcut: Ctrl+C")
            .clicked()
        {
            app.copy_image_to_clipboard();
        }
        ui.add_space(8.0);
        if settings_action_button(ui, SvgIcon::ContextCopyPath, "Copy Path")
            .on_hover_text("Shortcut: Ctrl+Shift+C")
            .clicked()
        {
            app.copy_path_to_clipboard();
        }
    });
    ui.add_space(4.0);
}

fn settings_action_button(ui: &mut egui::Ui, icon: SvgIcon, label: &str) -> egui::Response {
    let width = ((ui.available_width() - 16.0) * 0.5).clamp(112.0, 160.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    let (background, border, text_color) = {
        let visuals = ui.visuals();
        let widget = if response.is_pointer_button_down_on() {
            &visuals.widgets.active
        } else if response.hovered() {
            &visuals.widgets.hovered
        } else {
            &visuals.widgets.inactive
        };
        (widget.bg_fill, widget.bg_stroke, widget.text_color())
    };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(5), background);
    if border.width > 0.0 {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(5),
            border,
            egui::StrokeKind::Inside,
        );
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 15.0, rect.center().y),
        egui::vec2(15.0, 15.0),
    );
    paint_svg_icon(ui, icon_rect, icon, text_color);
    ui.painter().text(
        egui::pos2(icon_rect.right() + 7.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );
    response
}
