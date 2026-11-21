//! Main-view zoom, padding, and image-aware window sizing settings.
use crate::app::types::ViewerApp;
use crate::settings::AppSettings;
use eframe::egui;

/// Look-profile defaults provide modest edge padding only. Navigation controls
/// are foreground overlays and do not reserve space in the image viewport.
pub(crate) fn default_look_padding(profile: &str, placement: &str) -> [f32; 4] {
    // Order: top, right, bottom, left. Platform Looks place their navigation
    // controls over the image surface; reserve a clear top lane so the image
    // never renders underneath those controls. The desktop menu/tab rows are
    // already excluded by the central layout and are not counted here.
    match (profile, placement) {
        ("win98", _) => [76.0, 12.0, 12.0, 12.0],
        ("cosmic", _) | ("gnome", _) => [64.0, 12.0, 12.0, 12.0],
        _ => [0.0; 4],
    }
}

pub(crate) fn effective_padding(
    settings: &AppSettings,
    profile: &str,
    placement: &str,
) -> [f32; 4] {
    let base = default_look_padding(profile, placement);
    match settings.view_padding_mode.as_str() {
        "custom" => settings
            .view_padding_custom
            .map(|value| value.clamp(0.0, 400.0)),
        "fine_tune" => {
            let adjustment = settings
                .view_padding_fine_tune
                .get(profile)
                .copied()
                .unwrap_or([0.0; 4]);
            std::array::from_fn(|index| (base[index] + adjustment[index]).clamp(0.0, 400.0))
        }
        _ => base,
    }
}

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("▧ Viewing").strong(), |ui| {
        ui.add_space(4.0);
        render_zoom_step(app, ui);
        ui.separator();
        ui.collapsing("Padding", |ui| render_padding(app, ui, ctx));
        ui.separator();
        ui.collapsing("Window sizing", |ui| render_window_sizing(app, ui, ctx));
    });
}

fn render_zoom_step(app: &mut ViewerApp, ui: &mut egui::Ui) {
    let mut step_mouse_right_button = app
        .settings_manager
        .get()
        .zoom_step_mouse_right_button
        .clamp(1.05, 2.0);
    ui.label(egui::RichText::new("Right mouse button").strong());
    let step_label = format!(
        "Change per step (+{:.0}%)",
        (step_mouse_right_button - 1.0) * 100.0
    );
    ui.add(
        egui::Slider::new(&mut step_mouse_right_button, 1.05..=2.0)
            .step_by(0.05)
            .text(step_label),
    );
    if (step_mouse_right_button - app.settings_manager.get().zoom_step_mouse_right_button).abs()
        > f32::EPSILON
    {
        let _ = app
            .settings_manager
            .update(|settings| settings.zoom_step_mouse_right_button = step_mouse_right_button);
    }

    let mut step_mouse_wheel = app
        .settings_manager
        .get()
        .zoom_step_mouse_wheel
        .clamp(1.05, 2.0);
    ui.label(egui::RichText::new("Mouse Wheel").strong());
    let step_label = format!(
        "Change per step (+{:.0}%)",
        (step_mouse_wheel - 1.0) * 100.0
    );
    ui.add(
        egui::Slider::new(&mut step_mouse_wheel, 1.05..=2.0)
            .step_by(0.05)
            .text(step_label),
    );
    if (step_mouse_wheel - app.settings_manager.get().zoom_step_mouse_wheel).abs() > f32::EPSILON {
        let _ = app
            .settings_manager
            .update(|settings| settings.zoom_step_mouse_wheel = step_mouse_wheel);
    }

    let mut step_keyboard = app
        .settings_manager
        .get()
        .zoom_step_keyboard
        .clamp(1.05, 2.0);
    ui.label(egui::RichText::new("Keyboard").strong());
    let step_label = format!("Change per step (+{:.0}%)", (step_keyboard - 1.0) * 100.0);
    ui.add(
        egui::Slider::new(&mut step_keyboard, 1.05..=2.0)
            .step_by(0.05)
            .text(step_label),
    );
    if (step_keyboard - app.settings_manager.get().zoom_step_keyboard).abs() > f32::EPSILON {
        let _ = app
            .settings_manager
            .update(|settings| settings.zoom_step_keyboard = step_keyboard);
    }

    ui.label(
        egui::RichText::new(
            "Used by keyboard zoom, mouse-wheel zoom, and right-button drag sensitivity.",
        )
        .small()
        .weak(),
    );
}

fn render_padding(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("Padding").strong());
    let settings = app.settings_manager.get().clone();
    let profile = crate::ui_theme::current_look_profile(ctx).to_string();
    let placement =
        crate::ui_theme::resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx);
    let defaults = default_look_padding(&profile, placement);
    ui.label(
        egui::RichText::new(format!(
            "Look defaults — Top: {:.0}, Right: {:.0}, Bottom: {:.0}, Left: {:.0} pt",
            defaults[0], defaults[1], defaults[2], defaults[3]
        ))
        .small()
        .weak(),
    );
    let effective = effective_padding(&settings, &profile, placement);
    ui.label(
        egui::RichText::new(format!(
            "Current padding — Top: {:.0}, Right: {:.0}, Bottom: {:.0}, Left: {:.0} pt",
            effective[0], effective[1], effective[2], effective[3]
        ))
        .small()
        .strong(),
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
        let _ = app
            .settings_manager
            .update(|value| value.view_padding_mode = mode.clone());
    }

    let current = app.settings_manager.get().clone();
    match current.view_padding_mode.as_str() {
        "fine_tune" => {
            let mut values = current
                .view_padding_fine_tune
                .get(&profile)
                .copied()
                .unwrap_or([0.0; 4]);
            ui.label(
                egui::RichText::new("Adjustments are saved separately for each Look profile.")
                    .small()
                    .weak(),
            );
            let labels = ["Top", "Right", "Bottom", "Left"];
            let mut changed = false;
            for index in 0..4 {
                // Build the label before borrowing this element mutably for the slider.
                let label = format!("{} ({:+.0} pt)", labels[index], values[index]);
                ui.add(
                    egui::Slider::new(&mut values[index], -100.0..=100.0)
                        .step_by(1.0)
                        .text(label),
                );
                changed |= (values[index]
                    - current
                        .view_padding_fine_tune
                        .get(&profile)
                        .copied()
                        .unwrap_or([0.0; 4])[index])
                    .abs()
                    > f32::EPSILON;
            }
            if changed {
                let _ = app.settings_manager.update(|settings| {
                    settings
                        .view_padding_fine_tune
                        .insert(profile.clone(), values);
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
                changed |= ui
                    .add(
                        egui::Slider::new(&mut values[index], 0.0..=300.0)
                            .step_by(1.0)
                            .text(label),
                    )
                    .changed();
            }
            if changed {
                let _ = app
                    .settings_manager
                    .update(|settings| settings.view_padding_custom = values);
            }
        }
        _ => {
            ui.label(egui::RichText::new("Padding follows the active Look profile. Navigation controls float above the image and do not reserve viewport space.").small().weak());
        }
    }
}

fn render_window_sizing(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(egui::RichText::new("Window sizing").strong());
    let settings = app.settings_manager.get().clone();
    let mut enabled = settings.fit_window_to_image;
    if crate::ui_theme::switch(ui, &mut enabled, "Automatically fit window to each image") {
        let _ = app
            .settings_manager
            .update(|value| value.fit_window_to_image = enabled);
        if enabled {
            app.request_wrap_window_around_image(ctx);
        }
    }
    ui.label(
        egui::RichText::new(
            "The window resizes when a new image is displayed. This only applies in windowed mode.",
        )
        .small()
        .weak(),
    );

    let monitors = crate::platform::monitors::available_monitors();
    let current_window_rect = ctx.input(|input| input.viewport().outer_rect);
    let current_monitor = crate::platform::monitors::resolve_default_monitor_at(
        &monitors,
        crate::platform::monitors::CURRENT_WINDOW_MONITOR,
        current_window_rect,
    );
    let fallback_monitor = crate::platform::monitors::resolve_default_monitor(&monitors, "primary");
    let preferred_monitor_available = crate::platform::monitors::preferred_monitor_is_available(
        &monitors,
        &settings.window_default_monitor,
    );
    let monitor_label = match settings.window_default_monitor.as_str() {
        crate::platform::monitors::CURRENT_WINDOW_MONITOR => current_monitor
            .map(|monitor| format!("Current window position · {}", monitor.display_label()))
            .unwrap_or_else(|| "Current window position".to_owned()),
        "primary" => fallback_monitor
            .map(|monitor| {
                if monitor.is_primary {
                    monitor.display_label()
                } else {
                    format!("System default · {}", monitor.display_label())
                }
            })
            .unwrap_or_else(|| "Primary monitor (system default)".to_owned()),
        _ => monitors
            .iter()
            .find(|monitor| monitor.id == settings.window_default_monitor)
            .map(|monitor| monitor.display_label())
            .unwrap_or_else(|| {
                let fallback_name = fallback_monitor
                    .map(|monitor| monitor.name.as_str())
                    .unwrap_or("system default");
                format!("Saved monitor unavailable · using {fallback_name}")
            }),
    };
    ui.label(egui::RichText::new("Default monitor").strong());
    let mut default_monitor = settings.window_default_monitor.clone();
    egui::ComboBox::from_id_salt("window_default_monitor")
        .selected_text(monitor_label)
        .width(ui.available_width().max(180.0) - 8.0)
        .show_ui(ui, |ui| {
            let current_label = current_monitor
                .map(|monitor| format!("Current window position · {}", monitor.display_label()))
                .unwrap_or_else(|| "Current window position".to_owned());
            ui.selectable_value(
                &mut default_monitor,
                crate::platform::monitors::CURRENT_WINDOW_MONITOR.to_owned(),
                current_label,
            );
            ui.selectable_value(
                &mut default_monitor,
                "primary".to_owned(),
                "Primary monitor (system default)",
            );
            for monitor in &monitors {
                ui.selectable_value(
                    &mut default_monitor,
                    monitor.id.clone(),
                    monitor.display_label(),
                );
            }
            if !preferred_monitor_available {
                ui.selectable_value(
                    &mut default_monitor,
                    settings.window_default_monitor.clone(),
                    format!("Unavailable · {}", settings.window_default_monitor),
                );
            }
        });
    if default_monitor != settings.window_default_monitor {
        let _ = app
            .settings_manager
            .update(|value| value.window_default_monitor = default_monitor.clone());
    }
    ui.label(
        egui::RichText::new(
            "Current window position selects the monitor with the largest overlap with this window. If no overlap can be determined, the primary monitor is used, then the first available display. A specifically selected disconnected monitor also falls back to the primary display until it reconnects.",
        )
        .small()
        .weak(),
    );

    let mut limit = settings.fit_window_limit.clone();
    egui::ComboBox::from_id_salt("fit_window_limit")
        .selected_text(if limit == "custom" {
            "Custom minimum and maximum"
        } else {
            "Monitor size"
        })
        .width(ui.available_width().max(160.0) - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut limit, "monitor".to_owned(), "Monitor size");
            ui.selectable_value(
                &mut limit,
                "custom".to_owned(),
                "Custom minimum and maximum",
            );
        });
    if limit != settings.fit_window_limit {
        let _ = app
            .settings_manager
            .update(|value| value.fit_window_limit = limit.clone());
    }

    if limit == "custom" {
        let mut min_width = settings.fit_window_min_width.max(512.0);
        let mut max_width = settings.fit_window_max_width.max(min_width);
        let mut min_height = settings.fit_window_min_height.max(512.0);
        let mut max_height = settings.fit_window_max_height.max(min_height);
        ui.label("Window width (logical points)");
        let min_width_changed = ui
            .horizontal(|ui| {
                ui.label("Min");
                ui.add(
                    egui::DragValue::new(&mut min_width)
                        .range(512.0..=8192.0)
                        .speed(8.0),
                )
                .changed()
            })
            .inner;
        ui.horizontal(|ui| {
            ui.label("Max");
            ui.add(
                egui::DragValue::new(&mut max_width)
                    .range(512.0..=16384.0)
                    .speed(8.0),
            );
        });
        ui.label("Window height (logical points)");
        let min_height_changed = ui
            .horizontal(|ui| {
                ui.label("Min");
                ui.add(
                    egui::DragValue::new(&mut min_height)
                        .range(512.0..=8192.0)
                        .speed(8.0),
                )
                .changed()
            })
            .inner;
        ui.horizontal(|ui| {
            ui.label("Max");
            ui.add(
                egui::DragValue::new(&mut max_height)
                    .range(512.0..=16384.0)
                    .speed(8.0),
            );
        });
        if min_width > max_width {
            if min_width_changed {
                max_width = min_width;
            } else {
                min_width = max_width;
            }
        }
        if min_height > max_height {
            if min_height_changed {
                max_height = min_height;
            } else {
                min_height = max_height;
            }
        }
        if min_width != settings.fit_window_min_width
            || max_width != settings.fit_window_max_width
            || min_height != settings.fit_window_min_height
            || max_height != settings.fit_window_max_height
        {
            let _ = app.settings_manager.update(|value| {
                value.fit_window_min_width = min_width;
                value.fit_window_max_width = max_width;
                value.fit_window_min_height = min_height;
                value.fit_window_max_height = max_height;
            });
        }
    } else {
        let mut offset = settings.fit_window_monitor_offset.clamp(0.0, 4096.0);
        let changed = ui
            .horizontal(|ui| {
                ui.label("Monitor edge padding");
                ui.add(
                    egui::DragValue::new(&mut offset)
                        .range(0.0..=4096.0)
                        .speed(4.0)
                        .fixed_decimals(0)
                        .suffix(" pt"),
                )
                .changed()
            })
            .inner;
        if changed || offset != settings.fit_window_monitor_offset {
            let _ = app
                .settings_manager
                .update(|value| value.fit_window_monitor_offset = offset);
        }
        ui.label(
            egui::RichText::new(
                "Padding is applied equally to all four edges of the selected monitor. For example, 100 pt leaves a usable area inset by 100 pt on every side. The minimum window size remains 512 × 512 pt.",
            )
            .small()
            .weak(),
        );
    }

    let mut preserve = settings.fit_window_preserve_aspect;
    if crate::ui_theme::switch(
        ui,
        &mut preserve,
        "Preserve image aspect ratio when constrained",
    ) {
        let _ = app
            .settings_manager
            .update(|value| value.fit_window_preserve_aspect = preserve);
    }
    let mut centered = settings.fit_window_center;
    if crate::ui_theme::switch(ui, &mut centered, "Keep window centered after resizing") {
        let _ = app
            .settings_manager
            .update(|value| value.fit_window_center = centered);
    }
    ui.label(
        egui::RichText::new("Quick command: View → Wrap Window Around Image · Ctrl + Alt + W")
            .small()
            .weak(),
    );
}
