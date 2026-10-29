//! Custom title bar used when native window decorations are disabled.
//!
//! The drag region excludes the caption buttons, so a press on Minimize,
//! Maximize/Restore, or Close can never be mistaken for a window drag.
use crate::app::types::ViewerApp;
use eframe::egui;

const TITLE_BAR_HEIGHT: f32 = 36.0;


/// Reserve actual layout height for the custom title bar. This is called before any
/// other top panel, so the title bar is above the desktop menu and main content does
/// not paint or receive input behind it, even when navigation auto-hide is active.
pub fn reserve_layout_space(app: &ViewerApp, ctx: &egui::Context) {
    if app.settings_manager.get().show_titlebar
        || ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
    {
        return;
    }
    egui::TopBottomPanel::top("jpeg_viewer_custom_titlebar_spacer")
        .exact_height(TITLE_BAR_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(0))
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.set_min_height(TITLE_BAR_HEIGHT);
            ui.set_max_height(TITLE_BAR_HEIGHT);
        });
}

#[derive(Clone, Copy)]
enum CaptionControl {
    Minimize,
    Maximize,
    Close,
}

pub fn render(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.settings_manager.get().show_titlebar
        || ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
    {
        return;
    }

    // Some window managers briefly report a zero-sized client rectangle while a
    // minimize command is being processed. Never build a row from that transient size:
    // it can make egui compute a negative layout width for caption buttons.
    if ctx.input(|input| input.viewport().minimized.unwrap_or(false)) {
        return;
    }
    let viewport_width = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.width()));
    let reported_width = viewport_width.unwrap_or_else(|| ctx.input(|input| input.viewport_rect().width()));
    let profile = crate::ui_theme::current_look_profile(ctx);
    let win98 = profile == "win98";
    let (margin_x, margin_y) = if win98 { (3, 3) } else { (8, 4) };
    let button_size = if win98 {
        egui::vec2(22.0, 22.0)
    } else {
        egui::vec2(36.0, 28.0)
    };
    // Never force an Area wider than a small/transient viewport. In particular,
    // minimizing can briefly produce a zero-sized client rect on some backends.
    // Return early if the viewport cannot physically fit the three caption buttons.
    let right_resize_guard = super::resize_handles::CORNER_RESIZE_HIT_SIZE;
    let min_width = button_size.x * 3.0
        + right_resize_guard
        + (margin_x as f32 * 2.0)
        + 2.0;
    if !reported_width.is_finite() || reported_width < min_width {
        return;
    }
    let window_width = reported_width;
    super::overlay_area("title_bar", egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
        .constrain(true)
        .show(ctx, |area_ui| {
            area_ui.set_min_size(egui::vec2(window_width, TITLE_BAR_HEIGHT));
            area_ui.set_max_size(egui::vec2(window_width, TITLE_BAR_HEIGHT));

            let glass = crate::ui_theme::os_background_blur_enabled(ctx) && !win98;
            let profile_alpha = match profile {
                "gnome" => 222,
                "cosmic" => 218,
                _ => 198,
            };
            let alpha = if glass { profile_alpha } else { 255 };
            let background = crate::ui_theme::title_bar_surface_color(ctx, alpha);
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
                .inner_margin(egui::Margin::symmetric(margin_x, margin_y))
                .show(area_ui, |ui| {
                    let inner_width = (window_width - (margin_x as f32 * 2.0) - 2.0)
                        .max(button_size.x * 3.0 + right_resize_guard);
                    ui.set_min_width(inner_width);
                    ui.set_max_width(inner_width);
                    ui.set_min_height(TITLE_BAR_HEIGHT - (margin_y as f32 * 2.0));
                    ui.set_max_height(TITLE_BAR_HEIGHT - (margin_y as f32 * 2.0));

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let row_height = TITLE_BAR_HEIGHT - (margin_y as f32 * 2.0);
                        // Keep caption buttons out of the enlarged top-right resize
                        // corner target, so both resizing and close/maximize/minimize
                        // remain easy to hit without competing rectangles.
                        let controls_width = button_size.x * 3.0 + right_resize_guard;
                        let inner_available = ui.available_width().max(0.0);
                        let drag_width = (inner_available - controls_width).max(0.0);
                        let (drag_rect, drag_response) = ui.allocate_exact_size(
                            egui::vec2(drag_width, row_height),
                            egui::Sense::click_and_drag(),
                        );

                        // The title text is painted inside the draggable region. Clipping
                        // guarantees long filenames cannot run beneath the caption buttons.
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
                        // Match the useful part of a traditional title-bar system menu:
                        // Restore/Maximize, Minimize, and Close are available by
                        // right-clicking the draggable caption region, as well as by
                        // clicking the caption buttons.
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
                        ui.add_space(right_resize_guard);
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
