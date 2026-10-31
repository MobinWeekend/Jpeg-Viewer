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
    // The menu bar replaces the floating sidebar/hamburger button group. If the
    // user switches modes while the popup is open, close the now-duplicate menu.
    if desktop_menu_bar {
        app.hamburger_menu_open = false;
    }
    // Sidebar drawer handle is separate from the navigation toolbar and follows
    // the same visibility state, regardless of desktop menu-bar preference.
    let placement = crate::ui_theme::resolve_main_ui_placement(
        &app.settings_manager.get().main_ui_placement_preference,
        ctx,
    );
    // The custom title bar is static window chrome. It must remain visible even if
    // navigation overlays auto-hide, and its resize frame remains interactive too.
    title_bar::render(ctx, app);

    if !app.overlay_visible {
        // Desktop profile menu bars are always visible, even while the floating
        // viewer controls are hidden. Their application menu must remain usable.
        if app.hamburger_menu_open && !desktop_menu_bar {
            render_hamburger_menu(app, ctx, placement);
        }
        render_custom_frame_resize_handles(app, ctx);
        return;
    }

    // Keep the drawer control pinned to the left edge, even when the sidebar is
    // already open (then it sits on the sidebar's right edge to close it).
    render_sidebar_drawer_button(app, ctx);

    let is_fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    let menu_offset_y = if placement == "win98" {
        if is_fullscreen {
            // Fullscreen hides both the desktop menu bar and the custom titlebar
            // spacer. Keep the toolbar close to the top edge instead of leaving
            // the reserved windowed-mode gap behind.
            2.0
        } else if crate::app::ui::uses_desktop_menu_bar(ctx) {
            crate::app::ui::desktop_menu_bar_height(ctx) + 2.0
        } else if !app.settings_manager.settings.show_titlebar {
            36.0 + 2.0
        } else {
            2.0
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
    // Floating controls are offset by the docked sidebars so they never sit
    // underneath a panel.
    // Use the actual panel allocation, not the saved preference: egui may
    // clamp the panel on a narrow window, and overlay controls must begin past
    // the rendered sidebar edge to avoid drawing over it.
    let left_offset = if app.settings_manager.get().show_file_sidebar {
        app.sidebar_actual_width
    } else {
        0.0
    };
    let right_offset = app.right_sidebar_actual_width.max(0.0);

    // The placement can be overridden independently of the visual profile.
    // Automatic follows the resolved Look profile (desktop detection happens there).
    if placement == "native" {
        // Preserve the original JPEG Viewer positions exactly.
        toolbar::render_top_toolbar(app, ctx, left_offset + MENU_OFFSET, menu_offset_y);
        toolbar::render_bottom_toolbar(app, ctx, left_offset + MENU_OFFSET);
    } else {
        // Windows 98 uses a classic full-width toolbar strip below the menu bar,
        // with the action group aligned to the upper-right. Other platform layouts
        // remain centered inside the space between the sidebars.
        let viewport_width = ctx.input(|input| {
            input.viewport().inner_rect.map(|rect| rect.width()).unwrap_or(1280.0)
        });
        if placement == "win98" {
            let central_width = (viewport_width - left_offset - right_offset).max(0.0);
            // Keep the classic strip entirely between the docked sidebars. The
            // toolbar itself clips horizontally within this width if controls overflow.
            if central_width >= 80.0 {
                toolbar::render_platform_toolbar(
                    app, ctx, placement, left_offset, central_width, menu_offset_y,
                );
            }
        } else {
            let center_offset_x = (left_offset - right_offset) / 2.0;
            let central_width = viewport_width - left_offset - right_offset;
            if central_width >= 96.0 {
                let max_width = (central_width - 8.0).max(96.0);
                toolbar::render_platform_toolbar(
                    app, ctx, placement, center_offset_x, max_width, menu_offset_y,
                );
            }
        }
    }
    toolbar::render_bottom_right_toolbar(app, ctx, right_offset + MENU_OFFSET);

    // Inspect channel checklist sits as a secondary row in the navigation UI.
    if app.inspect_mode {
        egui::Area::new(egui::Id::new("inspect_channel_navigation_row"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, menu_offset_y + 48.0))
            .show(ctx, |ui| {
                toolbar_frame(ctx).show(ui, |ui| {
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Inspect").strong());
                        for (index, label) in ["R", "G", "B", "A"].iter().enumerate() {
                            changed |= ui.checkbox(&mut app.inspect_channels[index], *label).changed();
                        }
                        if app.inspect_channels.iter().filter(|v| **v).count() == 1 {
                            changed |= ui.checkbox(&mut app.inspect_colorized, "Show in color").changed();
                        }
                        if ui.small_button("Close").clicked() {
                            app.inspect_mode = false;
                            app.inspect_texture = None;
                            app.inspect_source = None;
                            app.inspect_source_path = None;
                        }
                    });
                    if changed { app.refresh_inspect_texture(ctx); }
                });
            });
    }

    // Render the popup after the toolbar so it can use the button's actual screen rect.
    if !desktop_menu_bar {
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
    if app.settings_manager.get().show_titlebar {
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
    resize_handles::render_resize_handles(ctx, thickness, border);
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
            ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, fill);
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

    let button_rect = app.hamburger_button_rect.unwrap_or_else(|| {
        egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(58.0, 28.0))
    });
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
    const TITLE_BAR_HEIGHT: f32 = 36.0;

    if crate::app::ui::uses_desktop_menu_bar(ctx) {
        // This height includes the custom title-bar spacer when native decorations
        // are disabled, so the toolbar always starts below both top rows.
        crate::app::ui::desktop_menu_bar_height(ctx) + 2.0
    } else if app.settings_manager.settings.show_titlebar {
        MENU_OFFSET
    } else {
        (MENU_OFFSET / 2.0) + TITLE_BAR_HEIGHT
    }
}

/// Helper to create an overlay area – used internally by child modules.
pub fn overlay_area(id: &'static str, anchor: egui::Align2, offset: egui::Vec2) -> egui::Area {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
}

/// Reserve the top layout strip for a custom title bar before docked panels are drawn.
pub fn reserve_custom_titlebar_space(app: &ViewerApp, ctx: &egui::Context) {
    title_bar::reserve_layout_space(app, ctx);
}
