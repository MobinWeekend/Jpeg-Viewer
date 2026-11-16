//! Overlay UI components for the viewer

mod rename_warning;
mod resize_handles;
mod title_bar;
mod toolbar;
mod visibility;

pub use toolbar::toolbar_frame; // used by hamburger_ui
pub use visibility::update_overlay_visibility; // used by hardcoded_input
// render_overlay_ui is defined below and is already public.

use crate::app::types::ViewerApp;
use eframe::egui;

/// Render the complete overlay UI
pub fn render_overlay_ui(app: &mut ViewerApp, ctx: &egui::Context) {
    const MENU_OFFSET: f32 = 8.0;
    let desktop_menu_bar = crate::app::ui::uses_desktop_menu_bar(ctx);
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    // Native corner rounding is a windowed-mode treatment. Ask the OS to remove
    // it while fullscreen and restore the user's preference when leaving fullscreen.
    crate::ui_theme::platform_backdrop::set_native_window_shape(
        app.settings_manager.get().native_window_rounding && !fullscreen,
    );
    let custom_titlebar = !app.settings_manager.get().show_titlebar
        && !fullscreen
        && app.settings_manager.get().custom_titlebar_navigation;
    // The desktop menu bar replaces the hamburger in windowed mode. Fullscreen
    // hides that bar, so keep the tab-row hamburger and its popup fully functional.
    if desktop_menu_bar && !fullscreen {
        app.hamburger_menu_open = false;
    }
    // Sidebar drawer handle is separate from the navigation toolbar and follows
    // the same visibility state, regardless of desktop menu-bar preference.
    let placement = crate::ui_theme::resolve_main_ui_placement(
        &app.settings_manager.get().main_ui_placement_preference,
        ctx,
    );
    // Rename is a modal action, not an overlay control: keep it available even
    // when the user hides the viewer overlays or the file sidebar.
    crate::app::queues::render_rename_dialog(app, ctx);
    if !app.overlay_visible {
        // Desktop profile menu bars are always visible, even while the floating
        // viewer controls are hidden. Their application menu must remain usable.
        if app.hamburger_menu_open
            && (!desktop_menu_bar || custom_titlebar || app.active_workspace_hides_image_navigation())
        {
            render_hamburger_menu(app, ctx, placement);
        }
        render_custom_frame_resize_handles(app, ctx);
        return;
    }

    // In windowed custom-titlebar mode the sidebar toggle lives inside the titlebar
    // before the hamburger. In native-decoration and fullscreen modes, retain its
    // original edge-drawer position.
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    if app.settings_manager.get().show_titlebar
        || fullscreen
        || !app.settings_manager.get().custom_titlebar_navigation
    {
        render_sidebar_drawer_button(app, ctx);
    }

    let is_fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    let menu_offset_y = if placement == "win98" {
        if is_fullscreen && app.workspace_tab_strip_visible(ctx) {
            // Fullscreen's tab row reserves the first 40px of the central view.
            52.0
        } else if is_fullscreen {
            // With only the primary Viewer tab, the strip (including +) is hidden.
            MENU_OFFSET
        } else {
            let base = if crate::app::ui::uses_desktop_menu_bar(ctx) {
                crate::app::ui::desktop_menu_bar_height(ctx) + 2.0
            } else if !app.settings_manager.settings.show_titlebar {
                crate::app::ui::custom_titlebar_height(ctx) + 2.0
            } else {
                2.0
            };
            base + if app.workspace_tab_strip_visible(ctx) { 44.0 } else { 0.0 }
        }
    } else {
        get_menu_offset_y(app, ctx)
    };

    // Hamburger button
    // render_hamburger_button(app, ctx, menu_offset_y);

    // Z-order contract: the image and sidebars are built in the central layout
    // before this module runs. Toolbar Areas use egui::Order::Foreground, keeping
    // navigation above the main view and both sidebars. Win98's menu-bar height is
    // included in `menu_offset_y`, so its toolbar sits below (never over) that bar.
    // All overlay widths and offsets use the same authoritative rectangle as
    // Masonry and Compare. CentralPanel has already removed docked sidebars.
    let central_rect = app.main_view_rect(ctx);
    let central_left = central_rect.left();
    let central_width = app.main_view_width(ctx);
    let toolbar_left = central_left + MENU_OFFSET;
    let toolbar_width = (central_width - MENU_OFFSET * 2.0).max(0.0);
    let hide_image_navigation = app.active_workspace_hides_image_navigation();

    if hide_image_navigation {
        // Masonry, Compare and New Tab own the central view and do not show
        // image-navigation overlays. The standalone hamburger is used only when
        // the tab strip does not host it (native-titlebar multi-tab and fullscreen
        // layouts place it at the strip's leading edge).
        if !custom_titlebar
            && !is_fullscreen
            && !desktop_menu_bar
            && !app.workspace_tab_strip_hosts_hamburger(ctx)
        {
            render_hamburger_button_at(
                app,
                ctx,
                egui::pos2(central_left + MENU_OFFSET, menu_offset_y),
            );
        }
    } else {
        if custom_titlebar {
            toolbar::render_bottom_toolbar(app, ctx, toolbar_left, toolbar_width);
        } else if placement == "native" {
            toolbar::render_top_toolbar(
                app,
                ctx,
                toolbar_left,
                menu_offset_y,
                toolbar_width,
            );
            toolbar::render_bottom_toolbar(app, ctx, toolbar_left, toolbar_width);
        } else if placement == "win98" {
            if central_width >= 136.0 {
                toolbar::render_platform_toolbar(
                    app,
                    ctx,
                    placement,
                    toolbar_left,
                    toolbar_width,
                    menu_offset_y,
                );
            }
        } else {
            let center_offset_x = app.main_view_center_offset_x(ctx);
            if central_width >= 136.0 {
                toolbar::render_platform_toolbar(
                    app,
                    ctx,
                    placement,
                    center_offset_x,
                    toolbar_width,
                    menu_offset_y,
                );
            }
        }
        let right_gap = app.main_view_right_gap(ctx);
        toolbar::render_bottom_right_toolbar(app, ctx, right_gap + MENU_OFFSET);
    }

    // Inspect controls belong to normal image navigation, not the Masonry browser.
    if app.inspect_mode && !hide_image_navigation {
        egui::Area::new(egui::Id::new("inspect_channel_navigation_row"))
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, menu_offset_y + 48.0),
            )
            .show(ctx, |ui| {
                toolbar_frame(ctx).show(ui, |ui| {
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Inspect").strong());
                        for (index, label) in ["R", "G", "B", "A"].iter().enumerate() {
                            changed |= ui
                                .checkbox(&mut app.inspect_channels[index], *label)
                                .changed();
                        }
                        if app.inspect_channels.iter().filter(|v| **v).count() == 1 {
                            changed |= ui
                                .checkbox(&mut app.inspect_colorized, "Show in color")
                                .changed();
                        }
                        if ui.small_button("Close").clicked() {
                            app.inspect_mode = false;
                            app.inspect_texture = None;
                            app.inspect_source = None;
                            app.inspect_source_path = None;
                        }
                    });
                    if changed {
                        app.refresh_inspect_texture(ctx);
                    }
                });
            });
    }

    // Render the popup after the toolbar so it can use the button's actual screen rect.
    if !desktop_menu_bar || is_fullscreen {
        render_hamburger_menu(app, ctx, placement);
    }

    // Rename warning
    rename_warning::rename_warning(app, ctx);

    // Render resize hit areas last. This matters for the four generous corner targets:
    // the top corners intentionally occupy part of the custom title-bar area and must
    // receive pointer drags before the title-bar drag region or caption UI.
    render_custom_frame_resize_handles(app, ctx);
}

fn render_custom_frame_resize_handles(app: &ViewerApp, ctx: &egui::Context) {
    if app.settings_manager.get().show_titlebar
        || ctx.input(|input| input.viewport().maximized.unwrap_or(false) || input.viewport().fullscreen.unwrap_or(false))
    {
        return;
    }
    let thickness = app.settings_manager.get().custom_window_frame_thickness;
    let win98 = crate::ui_theme::current_look_profile(ctx) == "win98";
    let border = if win98 {
        if ctx.style().visuals.dark_mode {
            egui::Color32::from_rgb(96, 99, 104)
        } else {
            egui::Color32::BLACK
        }
    } else {
        ctx.style().visuals.window_stroke.color
    };
    resize_handles::render_resize_handles(
        ctx,
        thickness,
        border,
        !app.settings_manager.get().show_titlebar,
    );
}

/*
pub fn render_hamburger_button(app: &mut ViewerApp, ctx: &egui::Context) {
    overlay_area(
        "hamburger_button",
        egui::Align2::LEFT_TOP,
        egui::vec2(0.0, 0.0),
    )
    .show(ctx, |ui| app.render_hamburger_ui(ui));
}
 */

fn render_hamburger_button_at(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    position: egui::Pos2,
) {
    egui::Area::new(egui::Id::new("masonry_navigation_hamburger"))
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .show(ctx, |ui| {
            app.render_hamburger_ui(ui);
        });
}

fn render_sidebar_drawer_button(app: &mut ViewerApp, ctx: &egui::Context) {
    const DRAWER_WIDTH: f32 = 26.0;
    const DRAWER_HEIGHT: f32 = 48.0;

    let sidebar_open = app.settings_manager.get().show_file_sidebar;
    let viewport_size = ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .map(|rect| rect.size())
            .unwrap_or(egui::vec2(1280.0, 800.0))
    });
    // Closed: flush with the window edge. Open: flush with the inner/right edge of
    // the left sidebar so the same affordance closes the drawer.
    let x = if sidebar_open {
        (app.sidebar_actual_width - DRAWER_WIDTH).max(0.0)
    } else {
        0.0
    };
    let y = (viewport_size.y * 0.25 - DRAWER_HEIGHT * 0.5).max(0.0);

    egui::Area::new(egui::Id::new("sidebar_drawer_handle"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(x, y))
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(DRAWER_WIDTH, DRAWER_HEIGHT),
                egui::Sense::click(),
            );
            let visuals = ui.visuals();
            let fill = if crate::ui_theme::current_look_profile(ctx) == "win98" {
                if visuals.dark_mode {
                    egui::Color32::from_rgb(38, 40, 43)
                } else {
                    egui::Color32::from_rgb(192, 192, 192)
                }
            } else {
                visuals.panel_fill
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::ZERO, fill);
            if crate::ui_theme::current_look_profile(ctx) == "win98" {
                crate::ui_theme::paint_win98_bevel(ui, rect, sidebar_open);
            } else {
                ui.painter().rect_stroke(
                    rect,
                    egui::CornerRadius::ZERO,
                    egui::Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color),
                    egui::StrokeKind::Inside,
                );
            }
            crate::app::ui::helpers::paint_sidebar_drawer_icon(ui, rect, !sidebar_open);
            let response = response.on_hover_text(if sidebar_open {
                "Hide sidebar (F3)"
            } else {
                "Show sidebar (F3)"
            });
            if response.clicked() {
                let _ = app.settings_manager.update(|settings| {
                    settings.show_file_sidebar = !sidebar_open;
                });
                ctx.request_repaint();
            }
        });
}

fn render_hamburger_menu(app: &mut ViewerApp, ctx: &egui::Context, placement: &str) {
    if !app.hamburger_menu_open {
        return;
    }

    let button_rect = app
        .hamburger_button_rect
        .unwrap_or_else(|| egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(58.0, 28.0)));
    let popup_gap = 6.0;
    // A desktop menu-bar button always opens downward. The COSMIC bottom toolbar
    // opens upward only when the original floating button is still in use.
    let opens_above = !crate::app::ui::uses_desktop_menu_bar(ctx) && placement == "cosmic";
    let position = if opens_above {
        egui::pos2(button_rect.left(), button_rect.top() - popup_gap)
    } else {
        egui::pos2(button_rect.left(), button_rect.bottom() + popup_gap)
    };
    let pivot = if opens_above {
        egui::Align2::LEFT_BOTTOM
    } else {
        egui::Align2::LEFT_TOP
    };

    egui::Area::new(egui::Id::new("hamburger_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .pivot(pivot)
        .constrain(true)
        .show(ctx, |ui| app.render_hamburger_menu_ui(ctx, ui));
}

fn get_menu_offset_y(app: &ViewerApp, ctx: &egui::Context) -> f32 {
    const MENU_OFFSET: f32 = 8.0;

    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    if fullscreen {
        if app.workspace_tab_strip_visible(ctx) {
            // The fullscreen workspace tab row reserves the first 40px of the central view.
            MENU_OFFSET + 44.0
        } else {
            MENU_OFFSET
        }
    } else {
        let base = if crate::app::ui::uses_desktop_menu_bar(ctx) {
            // Use the sum of the actually rendered top rows; inline menu/titlebar
            // mode is counted only once by desktop_menu_bar_height().
            crate::app::ui::desktop_menu_bar_height(ctx) + 2.0
        } else if app.settings_manager.settings.show_titlebar {
            MENU_OFFSET
        } else {
            crate::app::ui::custom_titlebar_height(ctx) + (MENU_OFFSET / 2.0)
        };
        base + if app.workspace_tab_strip_visible(ctx) { 44.0 } else { 0.0 }
    }
}

/// Helper to create an overlay area – used internally by child modules.
pub fn overlay_area(id: &'static str, anchor: egui::Align2, offset: egui::Vec2) -> egui::Area {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
}

/// Reserve the top layout strip for a custom title bar before docked panels are drawn.
pub fn reserve_custom_titlebar_space(app: &mut ViewerApp, ctx: &egui::Context) {
    title_bar::reserve_layout_space(app, ctx);
}

pub fn custom_titlebar_height(ctx: &egui::Context) -> f32 {
    title_bar::custom_titlebar_height(ctx)
}
