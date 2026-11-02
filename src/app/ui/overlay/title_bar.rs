//! Custom title bar used when native window decorations are disabled.
//!
//! The title bar is rendered inside its own top panel, so the reserved layout
//! height and the visible/hit-testable chrome are always identical.
use crate::app::types::ViewerApp;
use eframe::egui;

const CUSTOM_TITLEBAR_LAYOUT_HEIGHT_ID: &str = "jpeg_viewer_custom_titlebar_layout_height";

enum CaptionControl {
    Minimize,
    Maximize,
    Close,
}

/// Render and reserve the custom title bar in one top-panel pass. Keeping the
/// chrome inside the reservation prevents its frame, controls, or hover hitboxes
/// from extending into the desktop menu bar or main view.
pub fn custom_titlebar_height(ctx: &egui::Context) -> f32 {
    ctx.data(|data| {
        data.get_temp::<f32>(egui::Id::new(CUSTOM_TITLEBAR_LAYOUT_HEIGHT_ID))
            .unwrap_or(0.0)
    })
}

pub fn reserve_layout_space(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.settings_manager.get().show_titlebar
        || ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
    {
        // Clear cached geometry when the custom title bar is not being rendered;
        // otherwise callers can mistake an old windowed height for live layout.
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(CUSTOM_TITLEBAR_LAYOUT_HEIGHT_ID), 0.0_f32);
        });
        return;
    }

    let profile = crate::ui_theme::current_look_profile(ctx);
    let win98 = profile == "win98";
    let (margin, button_size) = if win98 {
        (3, egui::vec2(24.0, 24.0))
    } else {
        (4, egui::vec2(36.0, 32.0))
    };
    let panel_response = egui::TopBottomPanel::top("jpeg_viewer_custom_titlebar")
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(0))
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |panel_ui| {
            let window_width = panel_ui.available_width().max(0.0);
            panel_ui.set_min_width(window_width);
            panel_ui.set_max_width(window_width);
            panel_ui.set_clip_rect(panel_ui.max_rect());

            if ctx.input(|input| input.viewport().minimized.unwrap_or(false)) {
                return;
            }

            // Resize hit areas live on the outer edge and must not reserve layout
            // width. The top-left hit target is kept to the thin frame strip (see
            // resize_handles) so the first title-bar control can start at x=0.
            // Keep a one-pixel inner right margin. The top-right resize target
            // occupies only the two outermost pixels, not a full button gap.
            if !window_width.is_finite() || window_width <= 0.0 {
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
                (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0).round() as u8
            } else {
                profile_alpha
            };
            let background = if settings.tricks_backdrop_enabled {
                // The Tricks backdrop is one continuous window surface. Match
                // the main view's base color/opacity so the texture doesn't pick
                // up a different tint when it crosses the title-bar boundary.
                let mut color = crate::ui_theme::main_view_background_color(
                    &settings.main_view_background,
                    ctx.style().visuals.dark_mode,
                );
                if crate::ui_theme::os_background_blur_enabled(ctx) {
                    let [r, g, b, _] = color.to_srgba_unmultiplied();
                    let opacity = (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0)
                        .round() as u8;
                    color = egui::Color32::from_rgba_unmultiplied(r, g, b, opacity);
                }
                color
            } else {
                crate::ui_theme::title_bar_surface_color(
                    ctx,
                    alpha,
                    &settings.title_bar_color_mode,
                    &settings.main_view_background,
                )
            };
            // Round only the window-facing top corners. Square bottom corners
            // meet the main view without transparent gaps that look like a divider.
            let radius = if win98 {
                egui::CornerRadius::ZERO
            } else {
                egui::CornerRadius {
                    nw: 6,
                    ne: 6,
                    sw: 0,
                    se: 0,
                }
            };
            let visuals = ctx.style().visuals.clone();
            let foreground = if crate::ui_theme::contrast_prefers_light_text(background) {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            };

            // Paint the entire reserved panel, not just the inner row frame. This
            // prevents the transparent top-panel remainder from appearing as a
            // second dark/black strip beneath the visible title-bar controls.
            // `panel_ui.max_rect()` is the rectangle actually reserved by this
            // top panel. Use it instead of a window rectangle that is not in
            // scope here; it also follows the panel's current viewport geometry.
            let panel_rect = panel_ui.max_rect();
            panel_ui
                .painter()
                .rect_filled(panel_rect, radius, background);

            // Reuse the same cached, asynchronously prepared Tricks backdrop as
            // the main image viewport. Paint it above the title-bar surface but
            // below controls so the blurred photo reaches into the custom chrome.
            if settings.tricks_backdrop_enabled {
                let backdrop_rect = crate::app::ui::tricks_backdrop_window_rect(ctx, panel_rect);
                app.paint_tricks_backdrop(
                    &panel_ui.painter(),
                    panel_rect,
                    backdrop_rect,
                    crate::ui_theme::main_accent_color(ctx),
                    settings.tricks_backdrop_tint,
                    settings.tricks_backdrop_opacity,
                );
            }

            // The panel background above is the single surface paint. Painting
            // this inner frame again with alpha would make its center darker than
            // the exposed panel margins, producing a two-tone/overlapped title bar.
            egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE)
                .corner_radius(radius)
                .inner_margin(egui::Margin {
                    left: margin,
                    right: margin,
                    top: margin,
                    bottom: margin,
                })
                .show(panel_ui, |ui| {
                    let inner_width = (window_width - 16.0).max(button_size.x * 3.0);
                    let row_height = button_size.y;
                    ui.set_min_width(inner_width);
                    ui.set_max_width(inner_width);
                    ui.set_min_height(row_height);
                    ui.set_max_height(row_height);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let controls_width = button_size.x * 3.0;
                        let integrate_navigation =
                            app.settings_manager.get().custom_titlebar_navigation;
                        let inner_available = ui.available_width().max(0.0);
                        let inline_desktop_menu =
                            !integrate_navigation && crate::app::ui::uses_desktop_menu_bar(ctx);
                        let compact = inner_available < 600.0;
                        let action_budget = (inner_available - controls_width).max(0.0);
                        let ultra_compact = action_budget < 300.0;
                        let previous_compact = app.compactui;
                        app.compactui = compact;

                        // Sidebar access precedes the menu, but the menu is the
                        // highest-priority control and is never removed for width.
                        let sidebar_button_width =
                            if integrate_navigation && inner_available >= 270.0 {
                                if matches!(profile, "cosmic" | "gnome") {
                                    76.0
                                } else {
                                    28.0
                                }
                            } else {
                                0.0
                            };
                        if sidebar_button_width > 0.0 {
                            ui.add_space(10.0);
                            app.sidebar_drawer_ui(ctx, ui);
                        }

                        // If the custom title bar is not hosting navigation controls,
                        // host the desktop menu in this same row, on the left. It is
                        // not registered as a second top panel in this configuration.
                        if inline_desktop_menu {
                            ui.add_space(18.0);
                            crate::app::ui::render_desktop_menu_bar_in_titlebar(
                                app,
                                ctx,
                                ui,
                                row_height,
                                (inner_available - controls_width).max(0.0),
                            );
                        }

                        // When integrated navigation is enabled, the hamburger has
                        // absolute priority and remains visible even at minimal width.
                        // Otherwise the title bar returns to its standard title-only layout.
                        if integrate_navigation {
                            ui.add_space(3.0);
                            app.render_hamburger_ui(ui);
                        }

                        if integrate_navigation && !app.image_entries.is_empty() {
                            ui.add_space(6.0);
                            app.navigation_previous_ui(ctx, ui);
                            app.slideshow_ui(ctx, ui);
                            app.navigation_next_ui(ctx, ui);

                            // Match the floating toolbar's compact rules: retain the
                            // essential navigation trio and drop zoom controls first.
                            if !ultra_compact {
                                ui.add_space(6.0);
                                app.zoom_out_ui(ctx, ui);
                                app.zoom_text(ctx, ui);
                                app.zoom_in_ui(ctx, ui);
                            }
                        }

                        let actions_end = ui.next_widget_position().x;
                        let caption_start = ui.max_rect().right() - controls_width;
                        let drag_width = (caption_start - actions_end).max(0.0);
                        let title_click_rename =
                            app.settings_manager.get().rename_file_from_titlebar;
                        let (drag_rect, _) = ui.allocate_exact_size(
                            egui::vec2(drag_width, row_height),
                            egui::Sense::hover(),
                        );
                        let title_visible = drag_width >= 90.0;
                        let title_response = ui.interact(
                            drag_rect,
                            egui::Id::new("custom_titlebar_title"),
                            egui::Sense::click_and_drag(),
                        );
                        if title_visible {
                            let title = app.window_title();
                            let center_title =
                                app.settings_manager.get().center_custom_window_title;
                            let title_font =
                                egui::FontId::proportional(if win98 { 13.0 } else { 14.0 });
                            let title_width = ui
                                .painter()
                                .layout_no_wrap(title.clone(), title_font.clone(), foreground)
                                .size()
                                .x;
                            // Prefer the exact window midpoint. If the title would
                            // overlap the left-side actions or caption controls, center
                            // it in the remaining title area instead.
                            let window_center_x = panel_rect.center().x;
                            let exact_center_fits = window_center_x - title_width * 0.5
                                >= drag_rect.left()
                                && window_center_x + title_width * 0.5 <= drag_rect.right();
                            let title_pos = if center_title {
                                egui::pos2(
                                    if exact_center_fits {
                                        window_center_x
                                    } else {
                                        drag_rect.center().x
                                    },
                                    drag_rect.center().y,
                                )
                            } else {
                                egui::pos2(drag_rect.left() + 7.0, drag_rect.center().y)
                            };
                            let title_align = if center_title {
                                egui::Align2::CENTER_CENTER
                            } else {
                                egui::Align2::LEFT_CENTER
                            };
                            ui.painter().with_clip_rect(drag_rect).text(
                                title_pos,
                                title_align,
                                title,
                                title_font,
                                foreground,
                            );
                            if title_click_rename {
                                title_response
                                    .clone()
                                    .on_hover_text("Click to rename this file");
                            }
                        }

                        if title_response.double_clicked() {
                            let maximized =
                                ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        } else if title_visible && title_click_rename && title_response.clicked() {
                            app.begin_rename_current();
                        } else if title_response.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        title_response.context_menu(|menu| {
                            let maximized =
                                ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                            if menu
                                .add_enabled(maximized, egui::Button::new("Restore"))
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                                menu.close();
                            }
                            if menu
                                .add_enabled(!maximized, egui::Button::new("Maximize"))
                                .clicked()
                            {
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
                            let maximized =
                                ctx.input(|input| input.viewport().maximized.unwrap_or(false));
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
                        app.compactui = previous_compact;
                    });
                });
        });
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(CUSTOM_TITLEBAR_LAYOUT_HEIGHT_ID),
            panel_response.response.rect.height(),
        );
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
    let fill = if is_close && response.hovered() {
        egui::Color32::from_rgb(196, 43, 28)
    } else if win98 {
        // Classic caption controls remain gray on hover; only Close turns red.
        visuals.widgets.inactive.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let radius = if win98 {
        egui::CornerRadius::ZERO
    } else {
        egui::CornerRadius::same(5)
    };
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
                [
                    egui::pos2(center.x - 4.0, center.y + 4.0),
                    egui::pos2(center.x + 4.0, center.y + 4.0),
                ],
                icon_stroke,
            );
        }
        CaptionControl::Maximize => {
            let box_rect = egui::Rect::from_center_size(center, egui::vec2(12.0, 12.0));
            if ui
                .ctx()
                .input(|input| input.viewport().maximized.unwrap_or(false))
            {
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
                [
                    egui::pos2(center.x - 4.0, center.y - 4.0),
                    egui::pos2(center.x + 4.0, center.y + 4.0),
                ],
                icon_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 4.0, center.y - 4.0),
                    egui::pos2(center.x - 4.0, center.y + 4.0),
                ],
                icon_stroke,
            );
        }
    }
    response.on_hover_text(tooltip)
}
