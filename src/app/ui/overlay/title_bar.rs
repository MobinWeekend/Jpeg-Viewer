//! Custom title bar used when native window decorations are disabled.
//!
//! The title bar is rendered inside its own top panel, so the reserved layout
//! height and the visible/hit-testable chrome are always identical.
use crate::app::types::ViewerApp;
use eframe::egui;

const TITLE_BAR_HEIGHT: f32 = 38.0;

#[derive(Clone, Copy)]
enum CaptionControl {
    Minimize,
    Maximize,
    Close,
}

/// Render and reserve the custom title bar in one top-panel pass. Keeping the
/// chrome inside the reservation prevents its frame, controls, or hover hitboxes
/// from extending into the desktop menu bar or main view.
pub fn reserve_layout_space(app: &ViewerApp, ctx: &egui::Context) {
    if app.settings_manager.get().show_titlebar
        || ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
    {
        return;
    }

    egui::TopBottomPanel::top("jpeg_viewer_custom_titlebar")
        .exact_height(TITLE_BAR_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(0))
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |panel_ui| {
            panel_ui.set_min_height(TITLE_BAR_HEIGHT);
            panel_ui.set_max_height(TITLE_BAR_HEIGHT);
            let window_width = panel_ui.available_width().max(0.0);
            panel_ui.set_min_width(window_width);
            panel_ui.set_max_width(window_width);

            if ctx.input(|input| input.viewport().minimized.unwrap_or(false)) {
                return;
            }

            let profile = crate::ui_theme::current_look_profile(ctx);
            let win98 = profile == "win98";
            let (margin_y, button_size) = if win98 {
                (3, egui::vec2(22.0, 22.0))
            } else {
                (4, egui::vec2(36.0, 28.0))
            };
            // Keep a one-pixel inner right margin. The top-right resize target
            // occupies only the two outermost pixels, not a full button gap.
            const RIGHT_MARGIN: i8 = 1;
            let left_drag_guard = super::resize_handles::CORNER_RESIZE_HIT_SIZE.min(26.0);
            let left_content_space = left_drag_guard + 6.0;
            let minimum_width = button_size.x * 3.0 + left_content_space + 4.0;
            if !window_width.is_finite() || window_width < minimum_width {
                return;
            }

            let settings = app.settings_manager.get();
            let glass = crate::ui_theme::os_background_blur_enabled(ctx) && !win98;
            let profile_alpha = match profile {
                "gnome" => 222,
                "cosmic" => 218,
                _ => 198,
            };
            let alpha = if !glass {
                255
            } else if settings.title_bar_color_mode == "main_view" {
                (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0)
                    .round() as u8
            } else {
                profile_alpha
            };
            let background = crate::ui_theme::title_bar_surface_color(
                ctx,
                alpha,
                &settings.title_bar_color_mode,
                &settings.main_view_background,
            );
            let radius = if win98 { 0 } else { 6 };
            let visuals = ctx.style().visuals.clone();
            let foreground = if crate::ui_theme::contrast_prefers_light_text(background) {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            };

            egui::Frame::new()
                .fill(background)
                .stroke(if win98 {
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0, 0, 64))
                } else {
                    egui::Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color)
                })
                .corner_radius(egui::CornerRadius::same(radius))
                .inner_margin(egui::Margin {
                    left: 1,
                    right: RIGHT_MARGIN,
                    top: margin_y,
                    bottom: margin_y,
                })
                .show(panel_ui, |ui| {
                    let inner_width = (window_width - 4.0).max(button_size.x * 3.0);
                    let row_height = TITLE_BAR_HEIGHT - (margin_y as f32 * 2.0) - 2.0;
                    ui.set_min_width(inner_width);
                    ui.set_max_width(inner_width);
                    ui.set_min_height(row_height);
                    ui.set_max_height(row_height);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let controls_width = button_size.x * 3.0;
                        let inner_available = ui.available_width().max(0.0);
                        let drag_width =
                            (inner_available - left_content_space - controls_width).max(0.0);
                        ui.add_space(left_content_space);
                        let (drag_rect, drag_response) = ui.allocate_exact_size(
                            egui::vec2(drag_width, row_height),
                            egui::Sense::click_and_drag(),
                        );

                        ui.painter()
                            .with_clip_rect(drag_rect)
                            .text(
                                egui::pos2(drag_rect.left() + 7.0, drag_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                app.window_title(),
                                egui::FontId::proportional(if win98 { 13.0 } else { 14.0 }),
                                foreground,
                            );

                        if drag_response.double_clicked() {
                            let maximized = ctx.input(|input| {
                                input.viewport().maximized.unwrap_or(false)
                            });
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        } else if drag_response.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        drag_response.context_menu(|menu| {
                            let maximized = ctx.input(|input| {
                                input.viewport().maximized.unwrap_or(false)
                            });
                            if menu.add_enabled(maximized, egui::Button::new("Restore")).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                                menu.close();
                            }
                            if menu.add_enabled(!maximized, egui::Button::new("Maximize")).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                                menu.close();
                            }
                            if menu.button("Minimize").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                                menu.close();
                            }
                            menu.separator();
                            if menu.button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                menu.close();
                            }
                        });

                        let response = render_caption_button(
                            ui,
                            CaptionControl::Minimize,
                            "Minimize",
                            foreground,
                            profile,
                            &visuals,
                        );
                        if response.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }

                        let response = render_caption_button(
                            ui,
                            CaptionControl::Maximize,
                            if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
                                "Restore"
                            } else {
                                "Maximize"
                            },
                            foreground,
                            profile,
                            &visuals,
                        );
                        if response.clicked() {
                            let maximized = ctx.input(|input| {
                                input.viewport().maximized.unwrap_or(false)
                            });
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }

                        let response = render_caption_button(
                            ui,
                            CaptionControl::Close,
                            "Close",
                            foreground,
                            profile,
                            &visuals,
                        );
                        if response.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        });
}

fn render_caption_button(
    ui: &mut egui::Ui,
    control: CaptionControl,
    tooltip: &str,
    foreground: egui::Color32,
    profile: &str,
    visuals: &egui::style::Visuals,
) -> egui::Response {
    let win98 = profile == "win98";
    let size = if win98 {
        egui::vec2(22.0, 22.0)
    } else {
        egui::vec2(36.0, 28.0)
    };
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let is_close = matches!(control, CaptionControl::Close);
    let fill = if win98 {
        if is_close && response.hovered() {
            egui::Color32::from_rgb(196, 43, 28)
        } else {
            // Classic caption controls remain gray on hover; only Close turns red.
            visuals.widgets.inactive.bg_fill
        }
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let radius = if win98 { egui::CornerRadius::ZERO } else { egui::CornerRadius::same(5) };
    ui.painter().rect_filled(rect, radius, fill);
    if win98 {
        crate::ui_theme::paint_win98_bevel(ui, rect, pressed);
    } else if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0_f32, visuals.widgets.hovered.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }

    let icon_color = if is_close && response.hovered() {
        egui::Color32::WHITE
    } else if win98 {
        crate::ui_theme::contrast_foreground(fill)
    } else {
        foreground
    };
    let icon_stroke = egui::Stroke::new(if win98 { 1.5_f32 } else { 1.7_f32 }, icon_color);
    let center = rect.center();
    let painter = ui.painter();
    match control {
        CaptionControl::Minimize => {
            painter.line_segment(
                [egui::pos2(center.x - 4.0, center.y + 4.0), egui::pos2(center.x + 4.0, center.y + 4.0)],
                icon_stroke,
            );
        }
        CaptionControl::Maximize => {
            let box_rect = egui::Rect::from_center_size(center, egui::vec2(9.0, 8.0));
            if ui.ctx().input(|input| input.viewport().maximized.unwrap_or(false)) {
                painter.rect_stroke(
                    box_rect.translate(egui::vec2(-1.5, 1.5)),
                    egui::CornerRadius::ZERO,
                    icon_stroke,
                    egui::StrokeKind::Inside,
                );
                painter.rect_stroke(
                    box_rect.translate(egui::vec2(1.5, -1.5)),
                    egui::CornerRadius::ZERO,
                    icon_stroke,
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_stroke(
                    box_rect,
                    egui::CornerRadius::ZERO,
                    icon_stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        CaptionControl::Close => {
            painter.line_segment(
                [egui::pos2(center.x - 4.0, center.y - 4.0), egui::pos2(center.x + 4.0, center.y + 4.0)],
                icon_stroke,
            );
            painter.line_segment(
                [egui::pos2(center.x + 4.0, center.y - 4.0), egui::pos2(center.x - 4.0, center.y + 4.0)],
                icon_stroke,
            );
        }
    }
    response.on_hover_text(tooltip)
}
