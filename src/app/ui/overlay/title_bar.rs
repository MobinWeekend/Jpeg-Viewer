// Custom title bar implementation
use crate::app::types::ViewerApp;
use crate::app::ui::helpers::render_drag_area;
use eframe::egui;

const TITLE_BAR_HEIGHT: f32 = 36.0;
const TOOLBAR_BG_ALPHA: u8 = 217;

pub fn render(ctx: &egui::Context, app: &mut ViewerApp) {
    if app.settings_manager.settings.show_titlebar {
        return;
    }

    // Use the helper from the parent module
    super::overlay_area("title_bar", egui::Align2::CENTER_TOP, egui::Vec2::ZERO).show(ctx, |ui| {
        render_title_bar_ui(ctx, app, ui);
    });
}

fn render_title_bar_ui(ctx: &egui::Context, app: &mut ViewerApp, ui: &mut egui::Ui) {
    let window_width = ctx.available_rect().width();

    ui.set_min_width(window_width);
    ui.set_max_width(window_width);
    ui.set_min_height(TITLE_BAR_HEIGHT);

    let panel_color = ctx.style().visuals.panel_fill;
    let background = egui::Color32::from_rgba_unmultiplied(
        panel_color.r(),
        panel_color.g(),
        panel_color.b(),
        TOOLBAR_BG_ALPHA,
    );

    egui::Frame::new()
        .fill(background)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_min_width(window_width - 16.0);

            // Drag area
            render_drag_area(ctx, ui);

            ui.horizontal(|ui| {
                render_window_title(app, ui);
                render_window_controls(ctx, ui);
            });
        });
}

fn render_window_title(app: &ViewerApp, ui: &mut egui::Ui) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(egui::RichText::new(app.window_title()).size(14.0).strong())
                .selectable(false),
        );
    });
}

fn render_window_controls(ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        const BUTTON_WIDTH: f32 = 46.0;
        const BUTTON_HEIGHT: f32 = 28.0;

        let visuals = ui.style().visuals.clone();

        render_close_button(ctx, ui, BUTTON_WIDTH, BUTTON_HEIGHT, &visuals);
        render_maximize_button(ctx, ui, BUTTON_WIDTH, BUTTON_HEIGHT, &visuals);
        render_minimize_button(ctx, ui, BUTTON_WIDTH, BUTTON_HEIGHT, &visuals);
    });
}

fn render_close_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    visuals: &egui::style::Visuals,
) {
    let close_button = ui
        .add(
            egui::Button::new(
                egui::RichText::new("X")
                    .size(14.0)
                    .color(visuals.text_color()),
            )
            .frame(false)
            .min_size(egui::vec2(width, height)),
        )
        .on_hover_text("Close");

    if close_button.hovered() {
        ui.painter()
            .rect_filled(close_button.rect, 0.0, egui::Color32::from_rgb(196, 43, 28));
        ui.painter().text(
            close_button.rect.center(),
            egui::Align2::CENTER_CENTER,
            "X",
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );
    }

    if close_button.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn render_maximize_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    visuals: &egui::style::Visuals,
) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let icon = if maximized { "❐" } else { "□" };

    let maximize_button = ui
        .add(
            egui::Button::new(
                egui::RichText::new(icon)
                    .size(14.0)
                    .color(visuals.text_color()),
            )
            .frame(false)
            .min_size(egui::vec2(width, height)),
        )
        .on_hover_text(if maximized { "Restore" } else { "Maximize" });

    if maximize_button.hovered() {
        ui.painter()
            .rect_filled(maximize_button.rect, 0.0, visuals.widgets.hovered.bg_fill);
    }

    if maximize_button.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

fn render_minimize_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    visuals: &egui::style::Visuals,
) {
    let minimize_button = ui
        .add(
            egui::Button::new(
                egui::RichText::new("—")
                    .size(15.0)
                    .color(visuals.text_color()),
            )
            .frame(false)
            .min_size(egui::vec2(width, height)),
        )
        .on_hover_text("Minimize");

    if minimize_button.hovered() {
        ui.painter()
            .rect_filled(minimize_button.rect, 0.0, visuals.widgets.hovered.bg_fill);
    }

    if minimize_button.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}
