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
            if crate::ui_theme::switch(ui, &mut enabled, "Enable Slideshow") {
                app.slideshow_enabled = enabled;
                app.update_window_title(ctx);
            }
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut prevent_sleep = app.settings_manager.get().prevent_sleep_during_slideshow;
            if crate::ui_theme::switch(ui, &mut prevent_sleep, "Prevent system sleep") {
                let _ = app.settings_manager.update(|settings| {
                    settings.prevent_sleep_during_slideshow = prevent_sleep;
                });
            }
        });
        ui.label(egui::RichText::new("Keeps the computer awake while a slideshow is playing.").small().weak());
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
            if crate::ui_theme::switch(ui, &mut loop_enabled, "Loop") {
                app.slideshow_loop = loop_enabled;
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_loop = loop_enabled;
                });
            }
        });
        ui.add_space(4.0);
        /*
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut random_enabled = app.slideshow_random;
            if crate::ui_theme::switch(ui, &mut random_enabled, "Random Order") {
                app.slideshow_random = random_enabled;
                let _ = app.settings_manager.update(|settings| {
                    settings.slideshow_random = random_enabled;
                });
            }
        });
        ui.add_space(8.0);
         */
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Transition:");
            let mut transition = app.slideshow_transition.clone();
            egui::ComboBox::from_id_salt("slideshow_transition")
                .selected_text(match transition.as_str() { "none" => "Instant", "slide" => "Slide", "random" => "Random", _ => "Fade" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut transition, "none".to_string(), "Instant");
                    ui.selectable_value(&mut transition, "fade".to_string(), "Fade");
                    ui.selectable_value(&mut transition, "slide".to_string(), "Slide");
                    ui.selectable_value(&mut transition, "random".to_string(), "Random");
                });
            if transition != app.slideshow_transition {
                app.slideshow_transition = transition.clone();
                let _ = app.settings_manager.update(|s| s.slideshow_transition = transition);
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Image motion:");
            let mut motion = app.slideshow_motion.clone();
            egui::ComboBox::from_id_salt("slideshow_motion")
                .selected_text(match motion.as_str() { "none" => "Off", "pan-horizontal" => "Pan wide images", "pan-vertical" => "Pan tall images", "zoom-in" => "Zoom in", "zoom-out" => "Zoom out", "random" => "Random", _ => "Smart" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut motion, "none".to_string(), "Off");
                    ui.selectable_value(&mut motion, "smart".to_string(), "Smart (aspect ratio)");
                    ui.selectable_value(&mut motion, "pan-horizontal".to_string(), "Pan wide images");
                    ui.selectable_value(&mut motion, "pan-vertical".to_string(), "Pan tall images");
                    ui.selectable_value(&mut motion, "zoom-in".to_string(), "Zoom in");
                    ui.selectable_value(&mut motion, "zoom-out".to_string(), "Zoom out");
                    ui.selectable_value(&mut motion, "random".to_string(), "Random");
                });
            if motion != app.slideshow_motion {
                app.slideshow_motion = motion.clone();
                let _ = app.settings_manager.update(|s| s.slideshow_motion = motion);
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label("Motion strength:");
            let mut strength = app.slideshow_motion_strength;
            if ui.add(egui::Slider::new(&mut strength, 5.0..=40.0).suffix("%")).changed() {
                app.slideshow_motion_strength = strength;
                let _ = app.settings_manager.update(|s| s.slideshow_motion_strength = strength);
            }
        });
        ui.label(egui::RichText::new("Effects operate on the displayed texture; decoders remain independent.").small().weak());

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
