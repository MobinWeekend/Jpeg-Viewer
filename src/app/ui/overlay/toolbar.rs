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

pub fn render_top_toolbar(app: &mut ViewerApp, ctx: &egui::Context, offset_x: f32, offset_y: f32, max_width: f32) {
    let no_image = !app.image_entries.is_empty();
    let compact = app.compactui;
    // === Position of the top bar
    // egui remembers an Area's prior auto-size. Invalidate that measurement when
    // compact mode, profile, toolbar contents, or the available width changes.
    let width_bucket = (max_width.max(0.0) / 24.0).round() as i32;
    let sizing_changed = toolbar_area_sizing_changed(
        ctx,
        "top_center",
        crate::ui_theme::current_look_profile(ctx),
        compact,
        false,
        width_bucket,
        !app.image_entries.is_empty(),
        app.is_gif,
        crate::app::ui::uses_desktop_menu_bar(ctx),
        app.slideshow_enabled,
    );
    egui::Area::new(egui::Id::new("toolbar_top_center"))
        .sizing_pass(sizing_changed)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(offset_x, offset_y))
        .show(ctx, |ui| {
        // Override the Area's previous-frame content width. The external viewport
        // budget is authoritative when compact mode is toggled back to full size.
        ui.set_max_width(max_width.max(0.0));
        let content_width = toolbar_content_width_budget(ctx, compact, max_width);
        render_toolbar_ui(app, ctx, ui, content_width, |this, ui, budget, row_start| {
            if !crate::app::ui::uses_desktop_menu_bar(ctx) {
                this.render_hamburger_ui(ui);
            }
            let space = if compact { 8.0 } else { 1.0 };
            if no_image {
                separator(ui, space);
                this.zoom_out_ui(ctx, ui);
                this.zoom_text(ctx, ui);
                this.zoom_in_ui(ctx, ui);
            }
            if this.is_gif {
                separator(ui, space);
                this.gif_controls_ui(ui);
            }
            if no_image && !compact {
                separator(ui, space);
                let consumed = (ui.next_widget_position().x - row_start).max(0.0);
                this.file_metadata_button_with_width(ui, (budget - consumed).max(0.0));
            } else if compact {
                this.file_metadata_open = false;
            }
        });
    });
}

pub fn render_bottom_toolbar(app: &mut ViewerApp, ctx: &egui::Context, left_offset: f32) {
    if app.image_entries.is_empty() {
        return;
    }

    let viewport_width = ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .map(|rect| rect.width())
            .unwrap_or(1280.0)
    });
    let right_offset = if app.show_settings_menu || app.show_edit_menu {
        app.settings_manager.get().settings_sidebar_width
    } else {
        0.0
    };
    let usable_width = (viewport_width - left_offset - right_offset - 16.0).max(96.0);
    let sizing_changed = toolbar_area_sizing_changed(
        ctx,
        "jpeg_view_bottom",
        crate::ui_theme::current_look_profile(ctx),
        app.compactui,
        false,
        (usable_width / 24.0).round() as i32,
        true,
        app.is_gif,
        crate::app::ui::uses_desktop_menu_bar(ctx),
        app.slideshow_enabled,
    );
    overlay_area(
        "toolbar_bot_center",
        egui::Align2::LEFT_BOTTOM,
        egui::vec2(left_offset, -MENU_OFFSET),
    )
    .sizing_pass(sizing_changed)
    .constrain(true)
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

    // The prior Area size is stale whenever the layout signature changes; force a
    // one-frame sizing pass so the toolbar can expand and shrink to the new content.
    let width_bucket = (max_width.max(0.0) / 24.0).round() as i32;
    let sizing_changed = toolbar_area_sizing_changed(
        ctx,
        id,
        crate::ui_theme::current_look_profile(ctx),
        compact,
        ultra_compact,
        width_bucket,
        !app.image_entries.is_empty(),
        app.is_gif,
        has_desktop_menu_bar,
        app.slideshow_enabled,
    );
    egui::Area::new(egui::Id::new(id))
        .sizing_pass(sizing_changed)
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
        .constrain(true)
        .show(ctx, |ui| {
            let classic_win98 = placement == "win98";
            // Win98 keeps its full-width classic strip. COSMIC and GNOME only use
            // max-width constraints: their Area must size to the controls' content
            // instead of stretching from one side of the central canvas to the other.
            let frame_width = if classic_win98 {
                max_width.max(1.0)
            } else {
                max_width.max(96.0)
            };
            if classic_win98 {
                ui.set_min_width(frame_width);
            }
            ui.set_max_width(frame_width);
            let content_width = toolbar_content_width_budget(ctx, compact, frame_width);
            let frame_response = toolbar_frame_mode(ctx, compact).show(ui, |ui| {
                if classic_win98 {
                    ui.set_min_width((frame_width - 2.0).max(72.0));
                }
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
                                render_win98_primary_row(app, ctx, row, ultra_compact, primary_width);
                            },
                        );
                        if app.slideshow_enabled && !app.image_entries.is_empty() {
                            ui.separator();
                            render_slideshow_options_row(app, ctx, ui);
                        }
                    });
                } else {
                    // A plain vertical layout lets each row keep its natural width;
                    // the Area's center anchor positions the resulting content block.
                    ui.vertical(|ui| {
                        if app.slideshow_enabled
                            && placement == "cosmic"
                            && !app.image_entries.is_empty()
                        {
                            render_slideshow_options_row_intrinsic(app, ctx, ui);
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
                            content_width,
                        );
                        if app.slideshow_enabled
                            && placement != "cosmic"
                            && !app.image_entries.is_empty()
                        {
                            ui.separator();
                            render_slideshow_options_row_intrinsic(app, ctx, ui);
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
    width_budget: f32,
) {
    let row_start = ui.next_widget_position().x;
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
            if app.is_gif {
                separator(ui, 1.0);
                app.gif_controls_ui(ui);
            }
        }
        if !app.compactui && !ultra_compact {
            separator(ui, 1.0);
            let consumed = (ui.next_widget_position().x - row_start).max(0.0);
            app.file_metadata_button_with_width(ui, (width_budget - consumed).max(0.0));
        } else {
            app.file_metadata_open = false;
        }
    } else if ui.button("Open Image…").clicked() {
        app.handle_command(ctx, crate::shortcuts::ViewerCommand::OpenFile);
    }
}

fn render_slideshow_options_row_intrinsic(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
) {
    // Slideshow settings contain several controls. Let them wrap instead of
    // extending beyond the centered toolbar or being clipped at narrower widths.
    ui.horizontal_wrapped(|row| app.slideshow_speed_ui(ctx, row));
}

fn render_slideshow_options_row(app: &mut ViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    // The original JPEG Viewer layout has a narrower, floating toolbar than the
    // desktop-profile layouts. Wrap controls directly in the available region so
    // the content determines the height instead of reserving a zero-height row.
    ui.horizontal_wrapped(|row| app.slideshow_speed_ui(ctx, row));
}

fn render_platform_primary_row(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    has_desktop_menu_bar: bool,
    compact: bool,
    ultra_compact: bool,
    placement: &str,
    width_budget: f32,
) {
    ui.horizontal(|ui| {
        let row_start = ui.next_widget_position().x;
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

            if app.is_gif && !auto_compact_for_toolbar(compact, ultra_compact) {
                separator(ui, if compact { 8.0 } else { 1.0 });
                app.gif_controls_ui(ui);
            }

            if !compact && !ultra_compact {
                separator(ui, 1.0);
                let consumed = (ui.next_widget_position().x - row_start).max(0.0);
                app.file_metadata_button_with_width(ui, (width_budget - consumed).max(0.0));
            } else {
                app.file_metadata_open = false;
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

    let placement = crate::ui_theme::resolve_main_ui_placement(
        &app.settings_manager.get().main_ui_placement_preference,
        ctx,
    );
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    let (anchor, offset, id) = if placement == "cosmic" {
        let top_offset = if fullscreen {
            MENU_OFFSET
        } else if crate::app::ui::uses_desktop_menu_bar(ctx) {
            crate::app::ui::desktop_menu_bar_height(ctx) + 2.0
        } else if app.settings_manager.get().show_titlebar {
            MENU_OFFSET
        } else {
            40.0
        };
        (
            egui::Align2::RIGHT_TOP,
            egui::vec2(-right_offset - MENU_OFFSET, top_offset),
            "toolbar_cosmic_top_right_counter",
        )
    } else {
        (
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-right_offset, -MENU_OFFSET),
            "toolbar_bot_right",
        )
    };

    overlay_area(id, anchor, offset).show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, 0.0, |this, ui, _budget, _row_start| {
            render_image_counter(this, ui);
        });
    });
}

/// Force egui to remeasure an auto-sized toolbar when its layout constraints change.
/// Area state normally remembers the previous content size; that stale size can survive
/// a compact/full toggle and prevent the metadata button from expanding again.
fn toolbar_area_sizing_changed(
    ctx: &egui::Context,
    area_key: &'static str,
    profile: &str,
    compact: bool,
    ultra_compact: bool,
    width_bucket: i32,
    has_images: bool,
    is_gif: bool,
    has_desktop_menu_bar: bool,
    slideshow_enabled: bool,
) -> bool {
    let state_id = egui::Id::new(("toolbar_layout_signature", area_key));
    let current = (
        profile.to_owned(),
        compact,
        ultra_compact,
        width_bucket,
        has_images,
        is_gif,
        has_desktop_menu_bar,
        slideshow_enabled,
    );
    let previous = ctx.data(|data| {
        data.get_temp::<(String, bool, bool, i32, bool, bool, bool, bool)>(state_id)
    });
    ctx.data_mut(|data| data.insert_temp(state_id, current.clone()));
    previous.is_some_and(|previous| previous != current)
}

fn toolbar_content_width_budget(ctx: &egui::Context, compact: bool, outer_width: f32) -> f32 {
    let profile = crate::ui_theme::current_look_profile(ctx);
    let margin_x = match profile {
        "cosmic" | "gnome" => if compact { 8.0 } else { 10.0 },
        "win98" => 5.0,
        _ => if compact { 4.0 } else { 6.0 },
    };
    let has_stroke = matches!(profile, "cosmic" | "gnome" | "win98");
    (outer_width - margin_x * 2.0 - if has_stroke { 2.0 } else { 0.0 }).max(0.0)
}

fn render_toolbar_ui(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    content_width: f32,
    content: impl FnOnce(&mut ViewerApp, &mut egui::Ui, f32, f32),
) {
    let compact = app.compactui;
    toolbar_frame_mode(ctx, compact).show(ui, |ui| {
        ui.horizontal(|ui| {
            let row_start = ui.next_widget_position().x;
            content(app, ui, content_width, row_start);
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
