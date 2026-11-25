//! Reusable egui controls that adapt to the selected appearance profile.

use super::colors::with_alpha;
use super::profiles::current_look_profile;
use eframe::egui;

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
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            *value = !*value;
            changed = true;
        }

        let win98 = current_look_profile(ui.ctx()) == "win98";
        let on = *value;
        let visuals = ui.visuals().clone();

        if win98 {
            // ---- classic flat look (unchanged) ----
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
                egui::CornerRadius::ZERO,
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
            // ================= spring-driven, smear-y iOS knob =================
            //
            // We track a scalar `pos` in 0..1 with a velocity `vel` and integrate
            // a slight under-damped spring. This gives us:
            //   * a snappy arrival (stiffness controls how fast),
            //   * a tiny overshoot / bounce (zeta < 1),
            //   * a readable instantaneous velocity → the "smear".
            let pos_id = response.id.with("switch_spring_pos");
            let vel_id = response.id.with("switch_spring_vel");
            let target = if on { 1.0 } else { 0.0 };

            let mut pos: f32 = ui.data(|d| d.get_temp(pos_id).unwrap_or(target));
            let mut vel: f32 = ui.data(|d| d.get_temp(vel_id).unwrap_or(0.0));

            // Integrate with substeps so low frame rates stay stable.
            let dt = ui.input(|i| i.stable_dt).min(1.0 / 30.0);
            let substeps = 8u32;
            let h = dt / substeps as f32;
            let stiffness = 2800.0_f32; // higher = snappier
            let zeta = 0.62_f32; // <1 = overshoot; 1 = dead-beat
            let damping = 2.0 * zeta * stiffness.sqrt();

            let settled = (pos - target).abs() < 0.001 && vel.abs() < 0.001;
            if settled {
                pos = target;
                vel = 0.0;
            } else {
                for _ in 0..substeps {
                    let accel = stiffness * (target - pos) - damping * vel;
                    vel += accel * h;
                    pos += vel * h;
                }
                ui.ctx().request_repaint();
            }
            ui.data_mut(|d| {
                d.insert_temp(pos_id, pos);
                d.insert_temp(vel_id, vel);
            });

            let t = pos.clamp(0.0, 1.0);

            // Press / hover feedback — made quicker than before so it feels sharp.
            let press = ui.ctx().animate_bool_with_time(
                response.id.with("switch_press"),
                response.is_pointer_button_down_on(),
                0.07,
            );
            let hover = ui.ctx().animate_bool_with_time(
                response.id.with("switch_hover"),
                response.hovered(),
                0.09,
            );

            // A defined track is important in light mode, where the default
            // inactive surface can otherwise disappear against the panel.
            let off_track = if visuals.dark_mode {
                visuals.widgets.inactive.weak_bg_fill
            } else {
                egui::Color32::from_rgb(204, 209, 216)
            };
            let on_track = visuals.selection.bg_fill;
            let track = lerp_color(off_track, on_track, t);
            let corner = egui::CornerRadius::same((rect.height() * 0.5).round() as u8);
            let track_border = if t > 0.5 {
                visuals.selection.stroke.color
            } else if visuals.dark_mode {
                egui::Color32::from_rgb(104, 111, 120)
            } else {
                egui::Color32::from_rgb(111, 119, 129)
            };
            ui.painter().rect(
                rect,
                corner,
                track,
                egui::Stroke::new(1.0, track_border),
                egui::StrokeKind::Inside,
            );

            // ---- knob geometry with asymmetric smear ----
            let half_h = rect.height() * 0.5;
            let knob_r = half_h - 2.0;
            let x0 = rect.left() + half_h;
            let x1 = rect.right() - half_h;
            let x_target = x0 + (x1 - x0) * t;

            // Smear strength is proportional to instantaneous speed.
            // (Peak velocity of the spring is ~20 units/s → *0.06 saturates.)
            let smear = (vel.abs() * 0.06).min(1.0);
            let stretch = smear * 8.0; // px of elongation
            let dir = if vel >= 0.0 { 1.0 } else { -1.0 };

            // Keep the *leading* edge at the true target position; let the
            // *trailing* edge lag behind by `stretch`. That's the smear.
            let center = egui::pos2(x_target - dir * stretch * 0.5, rect.center().y);
            let rx = knob_r + stretch * 0.5 + 1.5 * press;
            let ry = knob_r * (1.0 - 0.18 * smear) + 1.0 * press;
            let radius = egui::vec2(rx, ry);

            let painter = ui.painter();

            // Hover halo in the track colour (so it tints correctly off/on).
            if hover > 0.01 {
                let [r, g, b, _] = track.to_array();
                painter.circle_filled(
                    center,
                    knob_r.max(rx) + 3.5,
                    egui::Color32::from_rgba_unmultiplied(r, g, b, (36.0 * hover).round() as u8),
                );
            }

            // Soft drop shadow, clipped to the track.
            let shadow = painter.with_clip_rect(rect);
            for i in (0..3).rev() {
                let spread = 1.0 + i as f32;
                shadow.add(egui::Shape::ellipse_filled(
                    center + egui::vec2(0.0, 1.0),
                    radius + egui::vec2(spread, spread),
                    egui::Color32::from_black_alpha(16 - 4 * i as u8),
                ));
            }

            // Knob (near-white so it reads on both light and dark tracks).
            painter.add(egui::Shape::ellipse_filled(
                center,
                radius,
                egui::Color32::from_gray(250),
            ));
            painter.add(egui::Shape::ellipse_stroke(
                center,
                radius,
                egui::Stroke::new(0.8, egui::Color32::from_rgb(108, 114, 122)),
            ));
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

/// Straight `u8`-space lerp between two colours.
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let a = a.to_array();
    let b = b.to_array();
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        mix(a[0], b[0]),
        mix(a[1], b[1]),
        mix(a[2], b[2]),
        mix(a[3], b[3]),
    )
}

/// Scale explicit RichText/FontId sizes alongside egui's default text styles.
pub fn scaled_text_size(ctx: &egui::Context, base: f32) -> f32 {
    let factor = ctx
        .data(|data| data.get_temp::<f32>(egui::Id::unique("ui_text_scale_factor")))
        .unwrap_or(1.0);
    base * factor
}
