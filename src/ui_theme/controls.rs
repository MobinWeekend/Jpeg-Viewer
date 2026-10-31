//! Reusable egui controls that adapt to the selected appearance profile.

use eframe::egui;
use super::colors::with_alpha;
use super::profiles::current_look_profile;

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
