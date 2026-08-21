use crate::app::types::ViewerApp;
use eframe::egui;
use std::time::Duration;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("💾 Cache Settings").size(13.0).strong());
    ui.add_space(4.0);

    // Cache Radius
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Cache Radius:");
        ui.add_space(8.0);
        let mut radius = app.cache_radius;
        if ui
            .add(
                egui::DragValue::new(&mut radius)
                    .range(1..=100)
                    .speed(1)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            if radius != app.cache_radius {
                app.update_cache_radius(radius);
                let _ = app.settings_manager.update(|settings| {
                    settings.cache_radius = radius;
                });
                if !app.image_entries.is_empty() && !app.is_loading() {
                    app.preload_adjacent_images();
                }
            }
        }
        ui.add_space(4.0);
        ui.label(format!("({})", radius));
    });
    ui.add_space(4.0);

    // Delta Factor
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Delta Factor:");
        ui.add_space(8.0);
        let mut factor = app.cache_delta_factor;
        if ui
            .add(
                egui::DragValue::new(&mut factor)
                    .range(0.1..=1.0)
                    .speed(0.05)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            if factor != app.cache_delta_factor {
                app.cache_delta_factor = factor;
                let _ = app.settings_manager.update(|settings| {
                    settings.cache_delta_factor = factor;
                });
            }
        }
        ui.add_space(4.0);
        ui.label(format!("({:.2})", factor));
    });
    ui.add_space(4.0);

    // Max Tasks
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Max Tasks:");
        ui.add_space(8.0);
        let mut tasks = app.max_cache_task;
        if ui
            .add(
                egui::DragValue::new(&mut tasks)
                    .range(1..=10)
                    .speed(1)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            if tasks != app.max_cache_task {
                app.max_cache_task = tasks;
                let _ = app.settings_manager.update(|settings| {
                    settings.max_cache_task = tasks;
                });
            }
        }
        ui.add_space(4.0);
        ui.label(format!("({})", tasks));
    });
    ui.add_space(4.0);

    // Preload Throttle
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Preload Throttle:");
        ui.add_space(8.0);
        let mut throttle = app.settings_manager.get().preload_throttle_ms;
        if ui
            .add(
                egui::DragValue::new(&mut throttle)
                    .range(10..=1000)
                    .speed(10)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            if throttle != app.settings_manager.get().preload_throttle_ms {
                let _ = app.settings_manager.update(|settings| {
                    settings.preload_throttle_ms = throttle;
                });
            }
        }
        ui.add_space(4.0);
        ui.label(format!("({} ms)", throttle));
    });
    ui.add_space(4.0);

    // Navigation Pause
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Navigation Pause:");
        ui.add_space(8.0);
        let mut pause = app.settings_manager.get().navigation_pause_ms;
        if ui
            .add(
                egui::DragValue::new(&mut pause)
                    .range(100..=5000)
                    .speed(100)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            if pause != app.settings_manager.get().navigation_pause_ms {
                let _ = app.settings_manager.update(|settings| {
                    settings.navigation_pause_ms = pause;
                    app.navigation_pause_duration = Duration::from_millis(pause);
                });
            }
        }
        ui.add_space(4.0);
        ui.label(format!("({} ms)", pause));
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Cache Progress
    render_cache_progress(app, ui);
}

fn render_cache_progress(app: &mut ViewerApp, ui: &mut egui::Ui) {
    let total_images = app.image_entries.len();
    let cached_count = app.image_cache.len();
    let cache_range = app.get_cache_range();
    let target_count = (cache_range * 2 + 1).min(total_images);
    let progress = if target_count > 0 {
        cached_count as f32 / target_count as f32
    } else {
        0.0
    };

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(format!(
            "📊 Cache: {}/{} images",
            cached_count, target_count
        ));
    });
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.add(egui::ProgressBar::new(progress).desired_width(200.0));
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(egui::RichText::new("🗑️ Clear Cache").size(13.0))
                    .min_size(egui::vec2(100.0, 28.0)),
            )
            .clicked()
        {
            app.image_cache.clear();
            app.preloading_indices.clear();
            app.preload_tasks.clear();
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}
