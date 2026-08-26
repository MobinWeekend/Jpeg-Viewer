use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ctx: &egui::Context) {
    let Some((current, suggested)) = get_rename_suggestion(app) else {
        return;
    };

    super::overlay_area(
        "rename_warning",
        egui::Align2::CENTER_TOP,
        egui::vec2(0.0, 36.0),
    )
    .show(ctx, |ui| {
        egui::Frame::new()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .corner_radius(egui::CornerRadius::same(6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚠")
                            .color(egui::Color32::RED)
                            .size(24.0),
                    );

                    ui.label(format!("Detected .{} (current: .{})", suggested, current));

                    let rename_btn = ui.button(
                        egui::RichText::new(" Rename ")
                            .color(egui::Color32::LIGHT_GREEN)
                            .size(14.0),
                    );

                    if rename_btn.clicked() {
                        app.apply_rename_suggestion();
                    }
                });
            });
    });
}

fn get_rename_suggestion(app: &ViewerApp) -> Option<(String, &'static str)> {
    app.file_type_detection
        .as_ref()
        .filter(|detection| {
            detection.mismatch
                && detection.index == app.current_index
                && detection.generation == app.preload_generation
        })
        .map(|detection| {
            let current = detection
                .current_extension
                .as_deref()
                .unwrap_or("(none)")
                .to_owned();

            let suggested = detection.detected_format.preferred_extension();

            (current, suggested)
        })
}
