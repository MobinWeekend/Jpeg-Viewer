use crate::app::types::ViewerApp;
use eframe::egui;

const TOOLBAR_BG_ALPHA: u8 = 217;
const MENU_OFFSET: f32 = 8.0;

pub fn toolbar_frame(ctx: &egui::Context) -> egui::Frame {
    let panel_color = ctx.style().visuals.panel_fill;
    let background = egui::Color32::from_rgba_unmultiplied(
        panel_color.r(),
        panel_color.g(),
        panel_color.b(),
        TOOLBAR_BG_ALPHA,
    );

    egui::Frame::new()
        .fill(background)
        .inner_margin(egui::Margin::symmetric(6, 6))
        .corner_radius(egui::CornerRadius::same(12))
}

fn overlay_area(id: &'static str, anchor: egui::Align2, offset: egui::Vec2) -> egui::Area {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
}

pub fn render_top_toolbar(app: &mut ViewerApp, ctx: &egui::Context, offset_y: f32) {
    if app.image_entries.is_empty() {
        return;
    }

    overlay_area(
        "toolbar_top_center",
        egui::Align2::CENTER_TOP,
        egui::vec2(0.0, offset_y),
    )
    .show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, |this, ui| {
            this.zoom_text(ui);
            this.zoom_ui(ctx, ui);
            this.fullscreen_ui(ctx, ui);
            this.pin_window_ui(ctx, ui);
            separator(ui);
            this.image_info_ui(ui);

            if this.is_gif {
                separator(ui);
                this.gif_controls_ui(ui);
            }
        });
    });
}

pub fn render_bottom_toolbar(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.image_entries.is_empty() {
        return;
    }

    overlay_area(
        "toolbar_bot_center",
        egui::Align2::CENTER_BOTTOM,
        egui::vec2(0.0, -MENU_OFFSET),
    )
    .show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, |this, ui| {
            this.navigation_previous_ui(ctx, ui);
            this.slideshow_ui(ctx, ui);
            this.navigation_next_ui(ctx, ui);
        });
    });
}

pub fn render_bottom_right_toolbar(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.image_entries.is_empty() {
        return;
    }

    overlay_area(
        "toolbar_bot_right",
        egui::Align2::RIGHT_BOTTOM,
        egui::vec2(-MENU_OFFSET, -MENU_OFFSET),
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
    toolbar_frame(ctx).show(ui, |ui| {
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

fn separator(ui: &mut egui::Ui) {
    ui.add_space(MENU_OFFSET);
    ui.separator();
    ui.add_space(MENU_OFFSET);
}
