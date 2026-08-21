//! Slideshow settings

use crate::app::types::ViewerApp;
use eframe::egui;
use std::time::Duration;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("🎬 Slideshow").size(15.0), |ui| {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut enabled = app.slideshow_enabled;
            if ui.checkbox(&mut enabled, "Enable Slideshow").changed() {
                app.slideshow_enabled = enabled;
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_enabled = enabled;
                });
                app.update_window_title(ctx);
            }
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Interval (seconds):");
            ui.add_space(8.0);
            let mut interval_secs = app.slideshow_interval.as_secs_f32();
            if ui
                .add(
                    egui::DragValue::new(&mut interval_secs)
                        .range(0.5..=60.0)
                        .speed(0.5)
                        .clamp_existing_to_range(true),
                )
                .changed()
            {
                let interval_ms = (interval_secs * 1000.0) as u64;
                app.slideshow_interval = Duration::from_millis(interval_ms);
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_interval_ms = interval_ms;
                });
            }
            ui.add_space(4.0);
            ui.label(format!("{:.1}s", interval_secs));
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut loop_enabled = app.slideshow_loop;
            if ui.checkbox(&mut loop_enabled, "Loop").changed() {
                app.slideshow_loop = loop_enabled;
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_loop = loop_enabled;
                });
            }
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut random_enabled = app.slideshow_random;
            if ui.checkbox(&mut random_enabled, "Random Order").changed() {
                app.slideshow_random = random_enabled;
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_random = random_enabled;
                });
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Shortcuts:");
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::LIGHT_GREEN, "L");
            ui.label("= Toggle, ");
            ui.colored_label(egui::Color32::LIGHT_GREEN, ",");
            ui.label("= Slower, ");
            ui.colored_label(egui::Color32::LIGHT_GREEN, ".");
            ui.label("= Faster");
        });
        ui.add_space(4.0);
    });
}