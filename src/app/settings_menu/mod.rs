//! Settings menu UI

mod advanced;
mod copy_buttons;
pub mod general;
mod look;
mod settings_file;
mod slideshow;
mod updates;
pub(crate) mod viewing;

use crate::app::types::ViewerApp;
use crate::app::ui::tricks;
use eframe::egui;


/// Collapsible settings section with a consistent SVG icon and hover state.
pub(crate) fn section(
    ui: &mut egui::Ui,
    id: &'static str,
    icon: crate::app::ui::SvgIcon,
    title: &str,
    default_open: bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let section_id = egui::Id::unique(("jpeg_viewer_settings_section", id));
    let mut open = ui.ctx().data(|data| data.get_temp::<bool>(section_id)).unwrap_or(default_open);
    let width = ui.available_width().max(1.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    if response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else {
            &ui.visuals().widgets.hovered
        };
        ui.painter().rect_filled(rect, egui::CornerRadius::same(5), visuals.bg_fill);
    }
    let center_y = rect.center().y;
    let chevron = if open { "▾" } else { "▸" };
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, center_y),
        egui::Align2::CENTER_CENTER,
        chevron,
        egui::FontId::proportional(14.0),
        ui.visuals().weak_text_color(),
    );
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 31.0, center_y),
        egui::vec2(16.0, 16.0),
    );
    crate::app::ui::paint_svg_icon(ui, icon_rect, icon, ui.visuals().text_color());
    ui.painter().text(
        egui::pos2(rect.left() + 46.0, center_y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(14.0),
        ui.visuals().text_color(),
    );
    if response.clicked() {
        open = !open;
        ui.ctx().data_mut(|data| data.insert_temp(section_id, open));
    }
    if open {
        ui.add_space(2.0);
        ui.indent(section_id, contents);
        ui.add_space(3.0);
    }
}

/// Compact icon-and-label heading used for grouped options inside a section.
pub(crate) fn subsection_heading(
    ui: &mut egui::Ui,
    icon: crate::app::ui::SvgIcon,
    title: &str,
) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        crate::app::ui::paint_svg_icon(ui, rect, icon, ui.visuals().text_color());
        ui.label(egui::RichText::new(title).strong());
    });
}

/// Render settings in a docked, resizable sidebar on the right.
pub fn render_settings_menu(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    if !app.show_settings_menu {
        return;
    }

    let sidebar_settings = app.settings_manager.get().clone();
    let default_width = sidebar_settings.settings_sidebar_width.clamp(260.0, 640.0);
    let sidebar_background = if crate::ui_theme::current_look_profile(ctx) == "win98" {
        // Use the active Win98 palette for both Light and Dark themes; other looks
        // retain the existing image-view surface behavior.
        ctx.global_style().visuals.panel_fill
    } else {
        crate::ui_theme::main_view_surface_color(
            ctx,
            &sidebar_settings.main_view_background,
            sidebar_settings.main_view_background_opacity,
        )
    };
    let sidebar_frame = egui::Frame::side_top_panel(&ctx.global_style())
        .fill(sidebar_background)
        .stroke(egui::Stroke::NONE);
    let response = egui::Panel::right("settings_sidebar")
        .resizable(true)
        .default_size(default_width)
        .size_range(260.0..=640.0)
        .frame(sidebar_frame)
        .show(ui, |ui| {
            if sidebar_settings.tricks_backdrop_enabled {
                let sidebar_clip = ui.painter().clip_rect();
                let backdrop_rect = crate::app::ui::tricks_backdrop_window_rect(ctx, sidebar_clip);
                app.paint_tricks_backdrop(
                    ui.painter(),
                    sidebar_clip,
                    backdrop_rect,
                    crate::ui_theme::main_accent_color(ctx),
                    sidebar_settings.tricks_backdrop_tint,
                    sidebar_settings.tricks_backdrop_opacity,
                );
            }
            ui.horizontal(|ui| {
                ui.heading("Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Close settings")
                        .clicked()
                    {
                        app.show_settings_menu = false;
                    }
                });
            });
            ui.separator();
            // Keep the version visible at the bottom while the settings sections scroll.
            const FOOTER_HEIGHT: f32 = 48.0;
            let scroll_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(scroll_height)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    copy_buttons::render(app, ui);
                    look::render(app, ui, ctx);
                    general::render(app, ui, ctx);
                    viewing::render(app, ui, ctx);
                    slideshow::render(app, ui, ctx);
                    tricks::render(app, ui, ctx);
                    advanced::render(app, ui, ctx);
                    updates::render(app, ui, ctx);
                    settings_file::render(app, ui, ctx);
                    ui.add_space(8.0);
                });
            ui.separator();
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("JPEG Viewer v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .weak(),
                );
            });
        });

    app.right_sidebar_actual_width = response.response.rect.width().max(0.0);
    let width = response.response.rect.width().clamp(260.0, 640.0);
    let previous = app.settings_manager.get().settings_sidebar_width;
    if !ctx.input(|input| input.pointer.any_down()) && (width - previous).abs() > 2.0 {
        let _ = app
            .settings_manager
            .update(|settings| settings.settings_sidebar_width = width);
    }
}

/// Toggle settings menu visibility.
pub fn toggle_settings_menu(app: &mut ViewerApp) {
    app.show_settings_menu = !app.show_settings_menu;
    if app.show_settings_menu {
        // Both views use the right dock; preserve the edit session but only
        // display one right-side panel at a time.
        app.show_edit_menu = false;
        app.crop_tool_active = false;
        app.crop_drag_start_image = None;
    }
}
