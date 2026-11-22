use super::*;

impl PopoutWindow {
    pub(super) fn control_bar_rect(viewport_rect: egui::Rect) -> egui::Rect {
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        // Keep the controls entirely inside the central area between the
        // enlarged corner-resize hotspots, so toolbar clicks never overlap a
        // move region on compact pop-out windows.
        let width = (viewport_rect.width() - corner * 2.0 - 8.0).clamp(54.0, 119.0);
        egui::Rect::from_center_size(
            egui::pos2(
                viewport_rect.center().x,
                viewport_rect.top() + corner + 16.0,
            ),
            egui::vec2(width, 30.0),
        )
    }

    pub(super) fn render_hover_controls(
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        tab_id: u64,
        show_buttons: bool,
    ) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter().clone();
        if show_buttons && viewport_rect.height() >= 48.0 {
            let gap = 4.0;
            let bar_rect = Self::control_bar_rect(viewport_rect);
            let bar_width = bar_rect.width();
            let button_width = ((bar_width - 16.0 - gap * 2.0) / 3.0).max(10.0);
            let button_size = egui::vec2(button_width, 24.0);
            painter.rect_filled(
                bar_rect,
                egui::CornerRadius::same(15),
                egui::Color32::from_black_alpha(165),
            );
            let first = bar_rect.left() + (bar_width - (button_size.x * 3.0 + gap * 2.0)) * 0.5;
            let top_rect =
                egui::Rect::from_min_size(egui::pos2(first, bar_rect.top() + 3.0), button_size);
            let bottom_rect = egui::Rect::from_min_size(
                egui::pos2(first + button_size.x + gap, bar_rect.top() + 3.0),
                button_size,
            );
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(first + (button_size.x + gap) * 2.0, bar_rect.top() + 3.0),
                button_size,
            );
            if Self::hover_button(
                ui,
                tab_id,
                top_rect,
                if self.stay_on_top { "●" } else { "↑" },
                "Stay on top",
            ) {
                self.stay_on_top = !self.stay_on_top;
                if self.stay_on_top {
                    self.stay_on_bottom = false;
                }
            }
            if Self::hover_button(
                ui,
                tab_id,
                bottom_rect,
                if self.stay_on_bottom { "●" } else { "↓" },
                "Stay on bottom",
            ) {
                self.stay_on_bottom = !self.stay_on_bottom;
                if self.stay_on_bottom {
                    self.stay_on_top = false;
                }
            }
            if Self::hover_button(ui, tab_id, close_rect, "×", "Close pop-out") {
                // Notify the root manager before requesting native closure. If
                // this flag is not set, the child viewport disappears but the
                // manager still considers the tab's pop-out open, preventing
                // the user from opening it again.
                self.close_requested = true;
            }
        }

        // Full four-edge resize strips and larger diagonal corner hit zones.
        // The top strip remains usable because the floating toolbar is inset
        // well below it; corner handles win over the generic move surface.
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        let edge = RESIZE_EDGE.min(corner);
        let left = viewport_rect.left();
        let right = viewport_rect.right();
        let top = viewport_rect.top();
        let bottom = viewport_rect.bottom();
        let zones = [
            (
                "north",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, top),
                    egui::pos2(right - corner, top + edge),
                ),
                0.0,
                -1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "south",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, bottom - edge),
                    egui::pos2(right - corner, bottom),
                ),
                0.0,
                1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "west",
                egui::Rect::from_min_max(
                    egui::pos2(left, top + corner),
                    egui::pos2(left + edge, bottom - corner),
                ),
                -1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "east",
                egui::Rect::from_min_max(
                    egui::pos2(right - edge, top + corner),
                    egui::pos2(right, bottom - corner),
                ),
                1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "north_west",
                egui::Rect::from_min_size(viewport_rect.min, egui::vec2(corner, corner)),
                -1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthWest,
            ),
            (
                "north_east",
                egui::Rect::from_min_size(
                    egui::pos2(right - corner, top),
                    egui::vec2(corner, corner),
                ),
                1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthEast,
            ),
            (
                "south_west",
                egui::Rect::from_min_size(
                    egui::pos2(left, bottom - corner),
                    egui::vec2(corner, corner),
                ),
                -1.0,
                1.0,
                egui::CursorIcon::ResizeSouthWest,
            ),
            (
                "south_east",
                egui::Rect::from_min_size(
                    viewport_rect.right_bottom() - egui::vec2(corner, corner),
                    egui::vec2(corner, corner),
                ),
                1.0,
                1.0,
                egui::CursorIcon::ResizeSouthEast,
            ),
        ];
        for (name, rect, horizontal, vertical, cursor) in zones {
            let response = ui.interact(
                rect,
                egui::Id::new(("jpeg_viewer_popout_resize", tab_id, name)),
                egui::Sense::drag(),
            );
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                self.resize_gesture = Some(ResizeGesture {
                    start_size: self.size,
                    start_position: self.position,
                    horizontal,
                    vertical,
                    accumulated_delta: egui::Vec2::ZERO,
                });
                self.dvd_mode = false;
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(gesture) = self.resize_gesture {
                    // Resize is based on total movement from the original press.
                    // drag_delta() is only the movement since the previous frame;
                    // adding it to start_size causes a reset/flicker every frame.
                    let mut gesture = gesture;
                    if let Some(total_delta) = response.total_drag_delta() {
                        gesture.accumulated_delta = total_delta;
                    } else {
                        gesture.accumulated_delta += response.drag_delta();
                    }
                    self.resize_gesture = Some(gesture);
                    let delta = gesture.accumulated_delta;
                    let horizontal_scale = if gesture.horizontal != 0.0 {
                        (gesture.start_size.x + delta.x * gesture.horizontal)
                            / gesture.start_size.x.max(1.0)
                    } else {
                        1.0
                    };
                    let vertical_scale = if gesture.vertical != 0.0 {
                        (gesture.start_size.y + delta.y * gesture.vertical)
                            / gesture.start_size.y.max(1.0)
                    } else {
                        1.0
                    };
                    let scale = match (gesture.horizontal != 0.0, gesture.vertical != 0.0) {
                        (true, true) => {
                            if (horizontal_scale - 1.0).abs() >= (vertical_scale - 1.0).abs() {
                                horizontal_scale
                            } else {
                                vertical_scale
                            }
                        }
                        (true, false) => horizontal_scale,
                        (false, true) => vertical_scale,
                        (false, false) => 1.0,
                    };
                    let new_size = self.size_for_width(gesture.start_size.x * scale);
                    let mut new_position = gesture.start_position;
                    if gesture.horizontal < 0.0 {
                        new_position.x += gesture.start_size.x - new_size.x;
                    } else if gesture.horizontal == 0.0 {
                        new_position.x += (gesture.start_size.x - new_size.x) * 0.5;
                    }
                    if gesture.vertical < 0.0 {
                        new_position.y += gesture.start_size.y - new_size.y;
                    } else if gesture.vertical == 0.0 {
                        new_position.y += (gesture.start_size.y - new_size.y) * 0.5;
                    }
                    self.request_geometry(&ctx, new_size, new_position);
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.resize_gesture = None;
            }
        }
        if self.resize_gesture.is_some()
            && !ctx.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
        {
            // A release outside the pop-out may not reach the originating resize
            // response. Global button state prevents a permanently latched resize.
            self.resize_gesture = None;
        }
    }

    pub(super) fn hover_button(
        ui: &mut egui::Ui,
        tab_id: u64,
        rect: egui::Rect,
        label: &str,
        tooltip: &str,
    ) -> bool {
        let response = ui.interact(
            rect,
            egui::Id::new(("jpeg_viewer_popout_button", tab_id, label)),
            egui::Sense::click(),
        );
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(6),
            if response.hovered() && label == "×" {
                egui::Color32::from_rgb(175, 55, 60)
            } else if response.hovered() {
                egui::Color32::from_black_alpha(220)
            } else {
                egui::Color32::from_black_alpha(155)
            },
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        response.on_hover_text(tooltip).clicked()
    }
}
