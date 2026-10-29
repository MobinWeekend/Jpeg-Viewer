// Overlay visibility management
use crate::app::types::ViewerApp;
use std::time::Duration;
use eframe::egui;

pub fn update_overlay_visibility(app: &mut ViewerApp, ctx: &egui::Context) {
    // Fullscreen is an intentional immersive view, but controls must remain reachable.
    // The desktop menu bar hides independently in fullscreen; navigation remains visible.
    if ctx.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
        set_visible(app, ctx, true);
        ctx.set_cursor_icon(egui::CursorIcon::Default);
        return;
    }

    if app.image_entries.is_empty() {
        return;
    }

    let settings = app.settings_manager.get().clone();
    if !crate::ui_theme::navigation_auto_hide_enabled(&settings, ctx) {
        set_visible(app, ctx, true);
        ctx.set_cursor_icon(egui::CursorIcon::Default);
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
    let hide_delay = Duration::from_millis(settings.navigation_ui_auto_hide_ms.clamp(500, 5000));

    let should_hide = !mouse_over_ui && !app.hamburger_menu_open && elapsed >= hide_delay;

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
