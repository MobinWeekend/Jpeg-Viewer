use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("🎮 Frame Limiter").size(13.0).strong());
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Controls how many frames per second the app renders.");
    });
    ui.add_space(6.0);

    // Max FPS
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Max FPS:");
        ui.add_space(8.0);
        if ui
            .add(
                egui::DragValue::new(&mut app.max_fps)
                    .range(0.0..=120.0)
                    .speed(1.0)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.max_fps = app.max_fps;
            });
            app.load_frame_limiter_settings();
            ctx.request_repaint();
        }
        ui.add_space(4.0);
        if app.max_fps == 0.0 {
            ui.label("Unlimited");
        } else {
            ui.label(format!(""));
        }
    });
    ui.add_space(4.0);

    // Idle FPS
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Idle FPS:");
        ui.add_space(8.0);
        if ui
            .add(
                egui::DragValue::new(&mut app.idle_fps_limit)
                    .range(0.0..=60.0)
                    .speed(1.0)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.idle_fps_limit = app.idle_fps_limit;
            });
            app.load_frame_limiter_settings();
            ctx.request_repaint();
        }
        ui.add_space(4.0);
        if app.idle_fps_limit == 0.0 {
            ui.label("Stop frame draw");
        } else {
            ui.label(format!(""));
        }
        ui.add_space(4.0);
    });
    ui.add_space(4.0);

    // Idle Timeout
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Idle Timeout (ms):");
        ui.add_space(8.0);
        if ui
            .add(
                egui::DragValue::new(&mut app.idle_timeout_ms)
                    .range(100..=10000)
                    .speed(100)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.idle_timeout_ms = app.idle_timeout_ms;
            });
            app.load_frame_limiter_settings();
            ctx.request_repaint();
        }
    });
    ui.add_space(4.0);

    // Unfocused Timeout
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Unfocused Timeout (ms):");
        ui.add_space(8.0);
        if ui
            .add(
                egui::DragValue::new(&mut app.unfocused_idle_timeout_ms)
                    .range(50..=5000)
                    .speed(50)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.unfocused_idle_timeout_ms = app.unfocused_idle_timeout_ms;
            });
            app.load_frame_limiter_settings();
            ctx.request_repaint();
        }
    });
    ui.add_space(4.0);

    // Unfocused Idle FPS
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Unfocused Idle FPS:");
        ui.add_space(8.0);
        if ui
            .add(
                egui::DragValue::new(&mut app.unfocused_idle_fps_limit)
                    .range(0.0..=60.0)
                    .speed(1.0)
                    .clamp_existing_to_range(true),
            )
            .changed()
        {
            let _ = app.settings_manager.update(|settings| {
                settings.unfocused_idle_fps_limit = app.unfocused_idle_fps_limit;
            });
            app.load_frame_limiter_settings();
            ctx.request_repaint();
        }
        ui.add_space(4.0);
        if app.unfocused_idle_fps_limit == 0.0 {
            ui.label("Stop frame draw");
        } else {
            ui.label(format!(""));
        }
        ui.add_space(4.0);
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // State display
    let state_text = if app.is_animating {
        "Animating"
    } else if app.is_loading() {
        "Loading"
    } else if app.is_idle {
        "Idle"
    } else {
        "Active"
    };
    let focus_text = if ctx.input(|i| i.viewport().focused).unwrap_or(false) {
        "✅ Focused"
    } else {
        "❌ Unfocused"
    };
    let fps_text = if app.current_fps > 0.0 {
        format!("{:.1} FPS", app.current_fps)
    } else {
        "N/A".to_string()
    };
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(format!(
            "Current: {} | FPS: {} | {}",
            state_text, fps_text, focus_text
        ));
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}
