// Overlay visibility management
use crate::app::types::ViewerApp;
use crate::constants::OVERLAY_HIDE_DELAY;
use eframe::egui;

pub fn update_overlay_visibility(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.image_entries.is_empty() {
        return;
    }

    let (window_focused, mouse_over_window) = ctx.input(|i| {
        (
            i.viewport().focused.unwrap_or(false),
            i.pointer.hover_pos().is_some(),
        )
    });

    if !window_focused || !mouse_over_window {
        set_visible(app, ctx, false);
        ctx.set_cursor_icon(egui::CursorIcon::Default);
        return;
    }

    let mouse_over_ui = ctx.is_pointer_over_area();
    let elapsed = app.last_interaction_time.elapsed();

    let should_hide = !mouse_over_ui && !app.hamburger_menu_open && elapsed >= OVERLAY_HIDE_DELAY;

    if should_hide {
        set_visible(app, ctx, false);
        ctx.set_cursor_icon(egui::CursorIcon::None);
    } else {
        set_visible(app, ctx, true);
        ctx.set_cursor_icon(egui::CursorIcon::Default);
    }
}

pub fn set_visible(app: &mut ViewerApp, ctx: &egui::Context, visible: bool) {
    if app.overlay_visible != visible {
        app.overlay_visible = visible;
        ctx.request_repaint();
    }
}
