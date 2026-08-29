use crate::app::types::ViewerApp;
use eframe::egui;

pub fn rename_warning(app: &mut ViewerApp, ctx: &egui::Context) {
    let Some((current, suggested)) = app.get_rename_suggestion() else {
        return;
    };

    super::overlay_area(
        "rename_warning",
        egui::Align2::CENTER_TOP,
        egui::vec2(0.0, 42.0),
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
