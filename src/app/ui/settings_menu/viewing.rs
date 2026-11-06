//! Main-view padding and image-aware window sizing settings.
use crate::app::types::ViewerApp;
use crate::settings::AppSettings;
use eframe::egui;

/// Look-profile defaults are the same edge/toolbar clearances previously applied
/// by fitted_image_viewport_rect, now exposed as editable defaults.
pub(crate) fn default_look_padding(profile: &str, placement: &str) -> [f32; 4] {
    // Order: top, right, bottom, left.
    if !matches!(profile, "cosmic" | "gnome" | "win98") {
        return [0.0; 4];
    }
    let mut padding = [12.0, 12.0, 12.0, 12.0];
    match placement {
        "cosmic" => padding[2] += 58.0,
        "gnome" | "win98" => padding[0] += 58.0,
        _ => {
            padding[0] += 40.0;
            padding[2] += 40.0;
        }
    }
    padding
}

pub(crate) fn effective_padding(settings: &AppSettings, profile: &str, placement: &str) -> [f32; 4] {
    let base = default_look_padding(profile, placement);
    match settings.view_padding_mode.as_str() {
        "custom" => settings.view_padding_custom.map(|value| value.clamp(0.0, 400.0)),
        "fine_tune" => {
            let adjustment = settings.view_padding_fine_tune.get(profile).copied().unwrap_or([0.0; 4]);
            std::array::from_fn(|index| (base[index] + adjustment[index]).clamp(0.0, 400.0))
        }
        _ => base,
    }
}

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("▧ Viewing").strong(), |ui| {
        ui.add_space(4.0);
        ui.collapsing("Padding", |ui| render_padding(app, ui, ctx));
        ui.separator();
        ui.collapsing("Window sizing", |ui| render_window_sizing(app, ui, ctx));
    });
}

fn render_padding(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("Padding").strong());
    let settings = app.settings_manager.get().clone();
    let profile = crate::ui_theme::current_look_profile(ctx).to_string();
    let placement = crate::ui_theme::resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx);
    let defaults = default_look_padding(&profile, placement);
    ui.label(
        egui::RichText::new(format!(
            "Look defaults — Top: {:.0}, Right: {:.0}, Bottom: {:.0}, Left: {:.0} pt",
            defaults[0], defaults[1], defaults[2], defaults[3]
        )).small().weak(),
    );
    let effective = effective_padding(&settings, &profile, placement);
    ui.label(
        egui::RichText::new(format!(
            "Current padding — Top: {:.0}, Right: {:.0}, Bottom: {:.0}, Left: {:.0} pt",
            effective[0], effective[1], effective[2], effective[3]
        )).small().strong(),
    );

    let mut mode = settings.view_padding_mode.clone();
    egui::ComboBox::from_id_salt("view_padding_mode")
        .selected_text(match mode.as_str() {
            "fine_tune" => "Fine-tune current look",
            "custom" => "Use custom padding",
            _ => "Use look defaults",
        })
        .width(ui.available_width().max(160.0) - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, "look".to_owned(), "Use look defaults");
            ui.selectable_value(&mut mode, "fine_tune".to_owned(), "Fine-tune current look");
            ui.selectable_value(&mut mode, "custom".to_owned(), "Use custom padding");
        });
    if mode != settings.view_padding_mode {
        let _ = app.settings_manager.update(|value| value.view_padding_mode = mode.clone());
    }

    let current = app.settings_manager.get().clone();
    match current.view_padding_mode.as_str() {
        "fine_tune" => {
            let mut values = current.view_padding_fine_tune.get(&profile).copied().unwrap_or([0.0; 4]);
            ui.label(egui::RichText::new("Adjustments are saved separately for each Look profile.").small().weak());
            let labels = ["Top", "Right", "Bottom", "Left"];
            let mut changed = false;
            for index in 0..4 {
                // Build the label before borrowing this element mutably for the slider.
                let label = format!("{} ({:+.0} pt)", labels[index], values[index]);
                ui.add(egui::Slider::new(&mut values[index], -100.0..=100.0)
                    .step_by(1.0)
                    .text(label));
                changed |= (values[index] - current.view_padding_fine_tune.get(&profile).copied().unwrap_or([0.0; 4])[index]).abs() > f32::EPSILON;
            }
            if changed {
                let _ = app.settings_manager.update(|settings| {
                    settings.view_padding_fine_tune.insert(profile.clone(), values);
                });
            }
        }
        "custom" => {
            let mut values = current.view_padding_custom;
            let labels = ["Top", "Right", "Bottom", "Left"];
            let mut changed = false;
            for index in 0..4 {
                // Avoid reading `values[index]` while the slider holds its mutable borrow.
                let label = format!("{} ({:.0} pt)", labels[index], values[index]);
                changed |= ui.add(egui::Slider::new(&mut values[index], 0.0..=300.0)
                    .step_by(1.0)
                    .text(label)).changed();
            }
            if changed {
                let _ = app.settings_manager.update(|settings| settings.view_padding_custom = values);
            }
        }
        _ => {
            ui.label(egui::RichText::new("Padding follows the active Look profile and its toolbar placement.").small().weak());
        }
    }
}

fn render_window_sizing(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("Window sizing").strong());
    let settings = app.settings_manager.get().clone();
    let mut enabled = settings.fit_window_to_image;
    if crate::ui_theme::switch(ui, &mut enabled, "Automatically fit window to each image") {
        let _ = app.settings_manager.update(|value| value.fit_window_to_image = enabled);
        if enabled {
            app.request_wrap_window_around_image(ctx);
        }
    }
    ui.label(egui::RichText::new("The window resizes when a new image is displayed. This only applies in windowed mode.").small().weak());

    let mut limit = settings.fit_window_limit.clone();
    egui::ComboBox::from_id_salt("fit_window_limit")
        .selected_text(if limit == "custom" { "Custom minimum and maximum" } else { "Monitor size" })
        .width(ui.available_width().max(160.0) - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut limit, "monitor".to_owned(), "Monitor size");
            ui.selectable_value(&mut limit, "custom".to_owned(), "Custom minimum and maximum");
        });
    if limit != settings.fit_window_limit {
        let _ = app.settings_manager.update(|value| value.fit_window_limit = limit.clone());
    }

    if limit == "custom" {
        let mut min_width = settings.fit_window_min_width.max(512.0);
        let mut max_width = settings.fit_window_max_width.max(min_width);
        let mut min_height = settings.fit_window_min_height.max(512.0);
        let mut max_height = settings.fit_window_max_height.max(min_height);
        ui.label("Window width (logical points)");
        let min_width_changed = ui.horizontal(|ui| {
            ui.label("Min");
            ui.add(egui::DragValue::new(&mut min_width).range(512.0..=8192.0).speed(8.0)).changed()
        }).inner;
        ui.horizontal(|ui| {
            ui.label("Max");
            ui.add(egui::DragValue::new(&mut max_width).range(512.0..=16384.0).speed(8.0));
        });
        ui.label("Window height (logical points)");
        let min_height_changed = ui.horizontal(|ui| {
            ui.label("Min");
            ui.add(egui::DragValue::new(&mut min_height).range(512.0..=8192.0).speed(8.0)).changed()
        }).inner;
        ui.horizontal(|ui| {
            ui.label("Max");
            ui.add(egui::DragValue::new(&mut max_height).range(512.0..=16384.0).speed(8.0));
        });
        if min_width > max_width {
            if min_width_changed { max_width = min_width; } else { min_width = max_width; }
        }
        if min_height > max_height {
            if min_height_changed { max_height = min_height; } else { min_height = max_height; }
        }
        if min_width != settings.fit_window_min_width || max_width != settings.fit_window_max_width
            || min_height != settings.fit_window_min_height || max_height != settings.fit_window_max_height
        {
            let _ = app.settings_manager.update(|value| {
                value.fit_window_min_width = min_width;
                value.fit_window_max_width = max_width;
                value.fit_window_min_height = min_height;
                value.fit_window_max_height = max_height;
            });
        }
    } else {
        ui.label(egui::RichText::new("The monitor's available dimensions provide the upper bounds; the application minimum remains 512 × 512 pt.").small().weak());
    }

    let mut preserve = settings.fit_window_preserve_aspect;
    if crate::ui_theme::switch(ui, &mut preserve, "Preserve image aspect ratio when constrained") {
        let _ = app.settings_manager.update(|value| value.fit_window_preserve_aspect = preserve);
    }
    let mut centered = settings.fit_window_center;
    if crate::ui_theme::switch(ui, &mut centered, "Keep window centered after resizing") {
        let _ = app.settings_manager.update(|value| value.fit_window_center = centered);
    }
    ui.label(egui::RichText::new("Quick command: View → Wrap Window Around Image · Ctrl + Alt + W").small().weak());
}
