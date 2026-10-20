use crate::app::types::ViewerApp;
use crate::app::ui::settings_menu::general::sort_method_horizontal_ui;
use eframe::egui;

const TOOLBAR_BG_ALPHA: u8 = 217;
const MENU_OFFSET: f32 = 8.0;

pub fn toolbar_frame(ctx: &egui::Context) -> egui::Frame {
    toolbar_frame_mode(ctx, false)
}

fn toolbar_frame_mode(ctx: &egui::Context, compact: bool) -> egui::Frame {
    let panel_color = ctx.style().visuals.panel_fill;
    let background = egui::Color32::from_rgba_unmultiplied(
        panel_color.r(),
        panel_color.g(),
        panel_color.b(),
        TOOLBAR_BG_ALPHA,
    );

    egui::Frame::new()
        .fill(background)
        .inner_margin(if compact {
            egui::Margin::symmetric(3, 1)
        } else {
            egui::Margin::symmetric(12, 6)
        })
        .corner_radius(egui::CornerRadius::same(if compact { 5 } else { 20 }))
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
            this.render_hamburger_ui(ui);
            if no_image {
                separator(ui);
                this.zoom_out_ui(ctx, ui);
                this.zoom_text(ctx, ui);
                this.zoom_in_ui(ctx, ui);
                //this.zoom_ui(ctx, ui);
                //this.fullscreen_ui(ctx, ui);
                //this.pin_window_ui(ctx, ui);
                if !compact {
                    separator(ui);
                    sort_method_horizontal_ui(this, ctx, ui);

                    separator(ui);
                    //this.image_info_ui(ui);
                    this.file_metadata_button(ui);
                }
            }
            if this.is_gif {
                separator(ui);
                this.gif_controls_ui(ui);
            }
        });
    });
}

pub fn render_bottom_toolbar(app: &mut ViewerApp, ctx: &egui::Context, left_offset: f32) {
    if app.image_entries.is_empty() {
        return;
    }

    // === Position of the navigation bottons
    overlay_area(
        "toolbar_bot_center",
        egui::Align2::LEFT_BOTTOM,
        egui::vec2(left_offset, -MENU_OFFSET),
    )
    .show(ctx, |ui| {
        render_toolbar_ui(app, ctx, ui, |this, ui| {
            this.navigation_previous_ui(ctx, ui);
            this.slideshow_ui(ctx, ui);
            this.navigation_next_ui(ctx, ui);
        });
    });
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

fn separator(ui: &mut egui::Ui) {
    ui.add_space(MENU_OFFSET / 4.0);
    ui.separator();
    ui.add_space(MENU_OFFSET / 4.0);
}
