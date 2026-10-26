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
    if !app.overlay_visible {
        return;
    }

    // Title bar
    title_bar::render(ctx, app);

    // Resize handles (if titlebar is hidden)
    if !app.settings_manager.settings.show_titlebar {
        resize_handles::render_resize_handles(ctx);
    }

    let menu_offset_y = get_menu_offset_y(app);

    // Hamburger button
    // render_hamburger_button(app, ctx, menu_offset_y);

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
    let right_offset = if app.show_settings_menu || app.show_edit_menu {
        app.settings_manager.get().settings_sidebar_width
    } else {
        0.0
    };

    render_hamburger_menu(app, ctx, left_offset);

    // Main toolbars
    toolbar::render_top_toolbar(app, ctx, left_offset + MENU_OFFSET, menu_offset_y);
    toolbar::render_bottom_toolbar(app, ctx, left_offset + MENU_OFFSET);
    toolbar::render_bottom_right_toolbar(app, ctx, right_offset + MENU_OFFSET);

    // Rename warning
    rename_warning::rename_warning(app, ctx);
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

fn render_hamburger_menu(app: &mut ViewerApp, ctx: &egui::Context, left_offset: f32) {
    const MENU_OFFSET: f32 = 8.0;
    const HAMBURGER_SIZE: f32 = 58.0;

    overlay_area(
        "hamburger_menu",
        egui::Align2::LEFT_TOP,
        if app.compactui {
            egui::vec2(left_offset + MENU_OFFSET * 2.0, HAMBURGER_SIZE)
        } else {
            egui::vec2(left_offset + MENU_OFFSET * 2.0, HAMBURGER_SIZE)
        },
    )
    .show(ctx, |ui| app.render_hamburger_menu_ui(ctx, ui));
}

fn get_menu_offset_y(app: &ViewerApp) -> f32 {
    const MENU_OFFSET: f32 = 8.0;
    const TITLE_BAR_HEIGHT: f32 = 36.0;

    if app.settings_manager.settings.show_titlebar {
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
