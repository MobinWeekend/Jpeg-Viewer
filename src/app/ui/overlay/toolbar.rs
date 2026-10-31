use crate::app::types::ViewerApp;
//use crate::app::ui::settings_menu::general::sort_method_horizontal_ui;
use eframe::egui;

const TOOLBAR_BG_ALPHA: u8 = 198;
const MENU_OFFSET: f32 = 8.0;

pub fn toolbar_frame(ctx: &egui::Context) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    let win98 = crate::ui_theme::current_look_profile(ctx) == "win98";
    let glass = crate::ui_theme::os_background_blur_enabled(ctx) && !win98;
    let surface = visuals.window_fill;
    let [r, g, b, _] = surface.to_srgba_unmultiplied();
    let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, if glass { 244 } else { 255 });
    egui::Frame::new()
        .fill(fill)
        .stroke(visuals.window_stroke)
        .corner_radius(egui::CornerRadius::same(if win98 { 0 } else { 12 }))
        .inner_margin(egui::Margin::same(if win98 { 4 } else { 10 }))
        .shadow(if win98 {
            egui::epaint::Shadow {
                offset: [0, 0],
                blur: 0,
                spread: 0,
                color: egui::Color32::TRANSPARENT,
            }
        } else {
            visuals.popup_shadow
        })
}

fn toolbar_frame_mode(ctx: &egui::Context, compact: bool) -> egui::Frame {
    let profile = crate::ui_theme::current_look_profile(ctx);
    let panel_color = ctx.style().visuals.panel_fill;

    // Native glass is used where available; egui surfaces become opaque when it is
    // disabled. Win98 always uses classic opaque, square, beveled surfaces.
    let glass = crate::ui_theme::os_background_blur_enabled(ctx) && profile != "win98";
    let (glass_alpha, radius, padding, border) = match profile {
        "cosmic" => (
            if compact { 208 } else { 198 },
            if compact { 8 } else { 12 },
            (8, 5),
            true,
        ),
        "gnome" => (202, 8, (8, 5), true),
        "win98" => (255, 0, (5, 3), true),
        _ => (
            if compact { 202 } else { TOOLBAR_BG_ALPHA },
            if compact { 8 } else { 24 },
            (if compact { 4 } else { 6 }, if compact { -4 } else { 3 }),
            false,
        ),
    };
    let alpha = if glass { glass_alpha } else { 255 };
    let background = egui::Color32::from_rgba_unmultiplied(
        panel_color.r(),
        panel_color.g(),
        panel_color.b(),
        alpha,
    );
    let inner_margin = if compact {
        egui::Margin::symmetric(padding.0, padding.1)
    } else if matches!(profile, "cosmic" | "gnome") {
        egui::Margin::symmetric(10, 6)
    } else {
        egui::Margin::symmetric(padding.0, padding.1)
    };
    let mut frame = egui::Frame::new()
        .fill(background)
        .inner_margin(inner_margin)
        .corner_radius(egui::CornerRadius::same(radius));
    if border {
        let stroke = if profile == "win98" {
            egui::Color32::from_rgb(128, 128, 128)
        } else {
            ctx.style().visuals.widgets.noninteractive.bg_stroke.color
        };
        frame = frame.stroke(egui::Stroke::new(1.0_f32, stroke));
    }
    frame
}

fn overlay_area(id: &'static str, anchor: egui::Align2, offset: egui::Vec2) -> egui::Area {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
}

pub fn render_top_toolbar(app: &mut ViewerApp, ctx: &egui::Context, offset_x: f32, offset_y: f32) {
    let no_image = !app.image_entries.is_empty();
    let compact = app.compactui;
    // === Position of the top bar
    overlay_area(
        "toolbar_top_center",
        egui::Align2::LEFT_TOP,
        egui::vec2(offset_x, offset_y),
    )
    .show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, |this, ui| {
            if !crate::app::ui::uses_desktop_menu_bar(ctx) {
                this.render_hamburger_ui(ui);
            }
            let space = if compact { 8.0 } else { 1.0 };
            if no_image {
                separator(ui, space);
                this.zoom_out_ui(ctx, ui);
                this.zoom_text(ctx, ui);
                this.zoom_in_ui(ctx, ui);
                //this.zoom_ui(ctx, ui);
                //this.fullscreen_ui(ctx, ui);
                //this.pin_window_ui(ctx, ui);
                if !compact {
                    //separator(ui, space);
                    //sort_method_horizontal_ui(this, ctx, ui);

                    separator(ui, space);
                    //this.image_info_ui(ui);
                    this.file_metadata_button(ui);
                }
            }
            if this.is_gif {
                separator(ui, space);
                this.gif_controls_ui(ui);
            }
        });
    });
}

pub fn render_bottom_toolbar(app: &mut ViewerApp, ctx: &egui::Context, left_offset: f32) {
    if app.image_entries.is_empty() {
        return;
    }

    overlay_area(
        "toolbar_bot_center",
        egui::Align2::LEFT_BOTTOM,
        egui::vec2(left_offset, -MENU_OFFSET),
    )
    .show(ctx, |ui| {
        /*
        let viewport_width = ctx.input(|input| {
            input
                .viewport()
                .inner_rect
                .map(|rect| rect.width())
                .unwrap_or(1280.0)
        });
         */
        let avail_rect = ui.available_rect_before_wrap();
        let right_offset = if app.show_settings_menu || app.show_edit_menu {
            app.settings_manager.get().settings_sidebar_width
        } else {
            0.0
        };
        let max_width = (avail_rect.width() - left_offset - right_offset - 16.0).max(96.0);
        ui.set_max_width(max_width);
        let compact = app.compactui;
        let frame_response = toolbar_frame_mode(ctx, compact).show(ui, |ui| {
            ui.set_max_width(max_width - 12.0);
            ui.vertical_centered(|ui| {
                // At the bottom edge, the options row must be above the primary row.
                if app.slideshow_enabled {
                    render_slideshow_options_row(app, ctx, ui);
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    app.navigation_previous_ui(ctx, ui);
                    app.slideshow_ui(ctx, ui);
                    app.navigation_next_ui(ctx, ui);
                });
            });
        });
        if crate::ui_theme::current_look_profile(ctx) == "win98" {
            crate::ui_theme::paint_win98_bevel(ui, frame_response.response.rect, false);
        }
    });
}

/// Render the primary controls and image navigation in a single centered bar for
/// desktop-specific layouts. COSMIC places it at the bottom; GNOME places it at
/// the top. The original JPEG Viewer layout continues to use the separate bars below.
pub fn render_platform_toolbar(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    placement: &str,
    center_offset_x: f32,
    max_width: f32,
    top_offset_y: f32,
) {
    let (anchor, offset, id) = match placement {
        "cosmic" => (
            egui::Align2::CENTER_BOTTOM,
            egui::vec2(center_offset_x, -MENU_OFFSET),
            "toolbar_cosmic_bottom_center",
        ),
        "gnome" => (
            egui::Align2::CENTER_TOP,
            egui::vec2(center_offset_x, top_offset_y),
            "toolbar_gnome_top_center",
        ),
        "win98" => (
            egui::Align2::LEFT_TOP,
            egui::vec2(center_offset_x, top_offset_y),
            "toolbar_win98_between_sidebars_below_menu_bar",
        ),
        _ => return,
    };

    let has_desktop_menu_bar = crate::app::ui::uses_desktop_menu_bar(ctx);
    // COSMIC/GNOME can rely on the menu bar for actions when no image is loaded.
    // Win98 keeps a classic toolbar below its always-on menu bar even on the welcome UI.
    if app.image_entries.is_empty() && has_desktop_menu_bar && placement != "win98" {
        return;
    }

    let original_compact = app.compactui;
    let auto_compact = max_width < 600.0 || (app.is_gif && max_width < 700.0);
    if auto_compact {
        app.compactui = true;
    }
    let compact = app.compactui;
    let ultra_compact = if placement == "win98" {
        max_width < 440.0
    } else {
        max_width < 300.0
    };

    overlay_area(id, anchor, offset)
        .constrain(true)
        .show(ctx, |ui| {
            let classic_win98 = placement == "win98";
            // Win98 is constrained to the central space between docked sidebars; other
            // profiles keep their established minimum width for centered controls.
            let frame_width = if classic_win98 {
                max_width.max(1.0)
            } else {
                max_width.max(96.0)
            };
            ui.set_min_width(frame_width);
            ui.set_max_width(frame_width);
            let frame_response = toolbar_frame_mode(ctx, compact).show(ui, |ui| {
                ui.set_min_width((frame_width - 2.0).max(72.0));
                ui.set_max_width((frame_width - 2.0).max(72.0));
                if classic_win98 {
                    // Classic Windows 98 toolbar fills only the central viewport between
                    // docked sidebars, with the action group aligned to the left edge.
                    ui.vertical(|ui| {
                        // Keep the main toolbar on a stable one-line baseline while the
                        // slideshow options row appears/disappears below it. Without a
                        // fixed row allocation, GIF controls and slider height can inflate
                        // the whole overlay on the first slideshow frame.
                        const PRIMARY_ROW_HEIGHT: f32 = 32.0;
                        let primary_width = ui.available_width().max(1.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(primary_width, PRIMARY_ROW_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |row| {
                                row.set_min_height(PRIMARY_ROW_HEIGHT);
                                row.set_max_height(PRIMARY_ROW_HEIGHT);
                                row.set_min_width(primary_width);
                                row.set_max_width(primary_width);
                                render_win98_primary_row(app, ctx, row, ultra_compact);
                            },
                        );
                        if app.slideshow_enabled && !app.image_entries.is_empty() {
                            ui.separator();
                            render_slideshow_options_row(app, ctx, ui);
                        }
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        if app.slideshow_enabled
                            && placement == "cosmic"
                            && !app.image_entries.is_empty()
                        {
                            render_slideshow_options_row(app, ctx, ui);
                            ui.separator();
                        }
                        render_platform_primary_row(
                            app,
                            ctx,
                            ui,
                            has_desktop_menu_bar,
                            compact,
                            ultra_compact,
                            placement,
                        );
                        if app.slideshow_enabled
                            && placement != "cosmic"
                            && !app.image_entries.is_empty()
                        {
                            ui.separator();
                            render_slideshow_options_row(app, ctx, ui);
                        }
                    });
                }
            });
            if classic_win98 {
                crate::ui_theme::paint_win98_bevel(ui, frame_response.response.rect, false);
            }
        });

    app.compactui = original_compact;
}

fn render_win98_primary_row(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    ultra_compact: bool,
) {
    // Classic controls are aligned left within the central viewport, never over
    // the docked sidebars. Narrow viewports use only the essential navigation trio.
    if !app.image_entries.is_empty() {
        app.navigation_previous_ui(ctx, ui);
        app.slideshow_ui(ctx, ui);
        app.navigation_next_ui(ctx, ui);
        if !ultra_compact {
            separator(ui, 1.0);
            app.zoom_out_ui(ctx, ui);
            app.zoom_text(ctx, ui);
            app.zoom_in_ui(ctx, ui);
            separator(ui, 1.0);
            app.file_metadata_button(ui);
            if app.is_gif {
                separator(ui, 1.0);
                app.gif_controls_ui(ui);
            }
        }
    } else if ui.button("Open Image…").clicked() {
        app.handle_command(ctx, crate::shortcuts::ViewerCommand::OpenFile);
    }
}

fn render_slideshow_options_row(app: &mut ViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    // This row is deliberately height-constrained. In the Win98 profile, the previous
    // child content could make the overlay's auto-sized Area grow vertically when the
    // slideshow was enabled. Horizontal overflow is handled by a clipped scroll area.
    const OPTIONS_ROW_HEIGHT: f32 = 32.0;
    let available_width = ui.available_width().max(1.0);
    ui.allocate_ui_with_layout(
        egui::vec2(available_width, OPTIONS_ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |host| {
            host.set_min_height(OPTIONS_ROW_HEIGHT);
            host.set_max_height(OPTIONS_ROW_HEIGHT);
            egui::ScrollArea::horizontal()
                .id_salt("slideshow_options_row")
                .max_height(OPTIONS_ROW_HEIGHT)
                .min_scrolled_height(OPTIONS_ROW_HEIGHT)
                .scroll_bar_visibility(
                    egui::containers::scroll_area::ScrollBarVisibility::AlwaysHidden,
                )
                .auto_shrink([false, false])
                .show(host, |row| {
                    row.set_min_height(OPTIONS_ROW_HEIGHT);
                    row.set_max_height(OPTIONS_ROW_HEIGHT);
                    row.horizontal(|row| app.slideshow_speed_ui(ctx, row));
                });
        },
    );
}

fn render_platform_primary_row(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    has_desktop_menu_bar: bool,
    compact: bool,
    ultra_compact: bool,
    placement: &str,
) {
    ui.horizontal(|ui| {
        let has_images = !app.image_entries.is_empty();
        if !has_desktop_menu_bar && (!ultra_compact || !has_images) {
            app.render_hamburger_ui(ui);
        }

        if has_images {
            if !has_desktop_menu_bar && (!ultra_compact || !has_images) {
                separator(ui, if compact { 8.0 } else { 1.0 });
            }

            // Keep the standard control sequence identical while a slideshow is
            // active; slideshow_ui swaps its start icon for pause/resume in-place.
            app.navigation_previous_ui(ctx, ui);
            app.slideshow_ui(ctx, ui);
            app.navigation_next_ui(ctx, ui);

            if !ultra_compact {
                separator(ui, if compact { 8.0 } else { 1.0 });
                app.zoom_out_ui(ctx, ui);
                app.zoom_text(ctx, ui);
                app.zoom_in_ui(ctx, ui);
            }

            if !compact && !ultra_compact {
                separator(ui, 1.0);
                app.file_metadata_button(ui);
            }

            if app.is_gif && !auto_compact_for_toolbar(compact, ultra_compact) {
                separator(ui, if compact { 8.0 } else { 1.0 });
                app.gif_controls_ui(ui);
            }
        } else if placement == "win98" {
            if ui.button("Open Image…").clicked() {
                app.handle_command(ctx, crate::shortcuts::ViewerCommand::OpenFile);
            }
        }
    });
}

fn auto_compact_for_toolbar(compact: bool, ultra_compact: bool) -> bool {
    compact || ultra_compact
}

pub fn render_bottom_right_toolbar(app: &mut ViewerApp, ctx: &egui::Context, right_offset: f32) {
    if app.image_entries.is_empty() {
        return;
    }

    overlay_area(
        "toolbar_bot_right",
        egui::Align2::RIGHT_BOTTOM,
        egui::vec2(-right_offset, -MENU_OFFSET),
    )
    .show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, |this, ui| {
            render_image_counter(this, ui);
        });
    });
}

fn render_toolbar_ui(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut ViewerApp, &mut egui::Ui),
) {
    let compact = app.compactui;
    toolbar_frame_mode(ctx, compact).show(ui, |ui| {
        ui.horizontal(|ui| {
            content(app, ui);
        });
    });
}

fn render_image_counter(app: &ViewerApp, ui: &mut egui::Ui) {
    let total = app.image_entries.len();
    let text = format!("{}/{}", app.current_index + 1, total);

    ui.label(
        egui::RichText::new(text)
            .size(14.0)
            .color(ui.style().visuals.text_color()),
    );
}

fn separator(ui: &mut egui::Ui, space: f32) {
    ui.add_space(MENU_OFFSET / (4.0 * space));
    ui.separator();
    ui.add_space(MENU_OFFSET / (4.0 * space));
}
