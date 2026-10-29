//! General settings:
//! - Invert scroll zoom
//! - Texture filter
//! - Theme
//! - Startup fullscreen
//! - Sorting method

use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use crate::sort::SortMethod;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    // ui.collapsing(egui::RichText::new("📋 General").size(15.0), |ui| {
    general_settings(app, ui, ctx);
    // });
}

pub fn general_settings(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.add_space(4.0);

    render_toggle_compactui(app, ui);
    ui.add_space(4.0);

    render_invert_scroll_zoom(app, ui);
    ui.add_space(4.0);

    render_texture_filter(app, ui);
    ui.add_space(4.0);

    render_theme(app, ui, ctx);
    ui.add_space(4.0);
    render_ui_appearance(app, ui, ctx);
    ui.add_space(4.0);

    ui.separator();
    ui.add_space(4.0);

    render_startup_fullscreen(app, ui);
    ui.add_space(4.0);
    render_start_centered(app, ui);
    ui.add_space(4.0);

    ui.separator();
    ui.add_space(4.0);

    render_thumbnail_workers(app, ui);
    ui.separator();
    ui.add_space(8.0);
}

fn render_start_centered(app: &mut ViewerApp, ui: &mut egui::Ui) {
    let mut centered = app.settings_manager.get().start_centered;
    if crate::ui_theme::switch(ui, &mut centered, "Start centered on screen") {
        let _ = app
            .settings_manager
            .update(|settings| settings.start_centered = centered);
    }
}

fn render_thumbnail_workers(app: &mut ViewerApp, ui: &mut egui::Ui) {
    let current = app.settings_manager.get().thumbnail_workers;
    let mut workers = current as u32;
    ui.horizontal(|ui| {
        ui.label("Thumbnail workers");
        ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut workers, 1..=8).show_value(true)));
    });
    if workers as usize != current {
        let _ = app
            .settings_manager
            .update(|settings| settings.thumbnail_workers = workers as usize);
    }
    ui.label("Default: 3. Restart the app to apply worker-count changes.");
}

// Invert Scroll Zoom
fn render_invert_scroll_zoom(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        let response = crate::ui_theme::switch(ui, &mut app.b_ctrl_invert, "Invert Scroll Zoom");

        if response {
            let value = app.b_ctrl_invert;

            let _ = app.settings_manager.update(|settings| {
                settings.b_ctrl_invert = value;
            });
        }

        ui.add_space(4.0);
        ui.label(if app.b_ctrl_invert {
            "(Ctrl+Scroll to navigate)"
        } else {
            "(Ctrl+Scroll to zoom)"
        });
    });
}

fn render_toggle_compactui(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let compact = crate::ui_theme::switch(ui, &mut app.compactui, "Compact mode");

        if compact {
            let value = app.compactui;
            let _ = app.settings_manager.update(|settings| {
                settings.compact_mode = value;
            });
        }
        ui.label("Toggle by pressing F4");
    });
}

// Texture Filter
fn render_texture_filter(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Texture Filter:");
        ui.add_space(8.0);

        let current_filter = app.settings_manager.get().texture_filter.clone();
        let mut filter = current_filter.clone();

        egui::ComboBox::from_id_salt("texture_filter")
            .selected_text(texture_filter_label(&filter))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");

                ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");

                ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
            });

        if filter != current_filter {
            let _ = app.settings_manager.update(|settings| {
                settings.texture_filter = filter;
            });
            reload_current_image_for_filter(app);
        }
    });
}

fn texture_filter_label(filter: &str) -> &'static str {
    match filter {
        "nearest" => "Nearest (fast)",
        "linear" => "Linear (smooth)",
        "mipmap" => "Mipmap (best)",
        _ => "Unknown",
    }
}

fn reload_current_image_for_filter(app: &mut ViewerApp) {
    if app.image_entries.is_empty() {
        return;
    }

    app.virtual_texture = None;
    app.vt_progress = None;
    app.vt_total_tiles = 0;
    app.virtual_texture_thread = None;

    app.load_current_image_with_cache();
}

// Theme
fn render_theme(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label("Theme:");
        ui.add_space(8.0);

        // Read current theme from settings (not from ctx directly)
        let current_theme = app.settings_manager.get().theme_preference.clone();
        let mut theme = current_theme.clone();

        egui::ComboBox::from_id_salt("theme")
            .selected_text(theme_label_str(&theme))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut theme, "system".to_string(), "Follow System");
                ui.selectable_value(&mut theme, "light".to_string(), "Light");
                ui.selectable_value(&mut theme, "dark".to_string(), "Dark");
            });

        if theme != current_theme {
            // Update settings
            let _ = app.settings_manager.update(|settings| {
                settings.theme_preference = theme.clone();
            });

            // Apply to egui context
            let s = app.settings_manager.get().clone();
            let theme_pref = crate::ui_theme::effective_theme_preference(
                &theme,
                &s.look_preference,
            );
            ctx.set_theme(theme_pref);
            // Refresh desktop-specific settings such as COSMIC's light/dark frost option.
            crate::ui_theme::set_look_profile(ctx, &s.look_preference);
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &s.ui_accent_color,
                s.ui_scale,
                s.ui_text_scale,
            );
        }
    });
}

fn render_ui_appearance(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.separator();
    ui.label(egui::RichText::new("UI appearance").strong());
    let current = app.settings_manager.get().clone();

    // Keep the UI-scale draft in egui temporary data while the slider is being dragged.
    let scale_id = egui::Id::new("ui_scale_draft");
    let mut scale = ctx
        .data(|d| d.get_temp::<f32>(scale_id))
        .unwrap_or(current.ui_scale);
    ui.horizontal(|ui| {
        ui.label("UI scale");
        let response = ui.add(
            crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut scale, 0.75..=1.50)
                .step_by(0.05)
                .suffix("×"),
        ));
        ctx.data_mut(|d| d.insert_temp(scale_id, scale));
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            let _ = app.settings_manager.update(|s| s.ui_scale = scale);
            let s = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &s.ui_accent_color,
                s.ui_scale,
                s.ui_text_scale,
            );
            ctx.data_mut(|d| d.remove::<f32>(scale_id));
        }
    });

    let saved_text_scale = app.settings_manager.get().ui_text_scale;
    let text_scale_id = egui::Id::new("ui_text_scale_draft");
    let mut text_scale = ctx
        .data(|d| d.get_temp::<f32>(text_scale_id))
        .unwrap_or(saved_text_scale);
    ui.horizontal(|ui| {
        ui.label("Text size");
        let response = ui.add(
            crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut text_scale, 0.80..=1.40)
                .step_by(0.05)
                .suffix("×"),
        ));
        ctx.data_mut(|d| d.insert_temp(text_scale_id, text_scale));
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            let _ = app
                .settings_manager
                .update(|s| s.ui_text_scale = text_scale);
            let s = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &s.ui_accent_color,
                s.ui_scale,
                s.ui_text_scale,
            );
            ctx.data_mut(|d| d.remove::<f32>(text_scale_id));
        }
    });

    let current_accent = app.settings_manager.get().ui_accent_color.clone();
    let mut accent = current_accent.clone();
    ui.horizontal(|ui| {
        ui.label("Main color");
        egui::ComboBox::from_id_salt("ui_accent_color")
            .selected_text(if accent.starts_with("custom:#") {
                "Custom"
            } else {
                match accent.as_str() {
                    "auto" if crate::ui_theme::current_look_profile(ctx) == "win98" => "Classic (Windows 98 default)",
                    "auto" => "Automatic (desktop)",
                    "win98blue" => "Classic Windows 98 blue",
                    "blue" => "Blue",
                    "purple" => "Purple",
                    "orange" => "Orange",
                    "rose" => "Rose",
                    "green" => "Green",
                    "yellow" => "Yellow",
                    "red" => "Red",
                    _ => "Teal",
                }
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut accent, "auto".to_string(), if crate::ui_theme::current_look_profile(ctx) == "win98" { "Classic (Windows 98 default)" } else { "Automatic (desktop)" });
                ui.selectable_value(&mut accent, "win98blue".to_string(), "Classic Windows 98 blue");
                ui.selectable_value(&mut accent, "teal".to_string(), "Teal");
                ui.selectable_value(&mut accent, "blue".to_string(), "Blue");
                ui.selectable_value(&mut accent, "purple".to_string(), "Purple");
                ui.selectable_value(&mut accent, "orange".to_string(), "Orange");
                ui.selectable_value(&mut accent, "rose".to_string(), "Rose");
                ui.selectable_value(&mut accent, "green".to_string(), "Green");
                ui.selectable_value(&mut accent, "yellow".to_string(), "Yellow");
                ui.selectable_value(&mut accent, "red".to_string(), "Red");
                ui.selectable_value(&mut accent, "custom:#00857B".to_string(), "Custom");
            });
    });
    if accent.starts_with("custom:#") {
        let mut color = parse_setting_color(&accent);
        ui.horizontal(|ui| {
            ui.label("Custom accent");
            if ui.color_edit_button_srgba(&mut color).changed() {
                accent = format!(
                    "custom:#{:02X}{:02X}{:02X}",
                    color.r(),
                    color.g(),
                    color.b()
                );
            }
        });
    }
    if accent != current_accent {
        let _ = app.settings_manager.update(|s| s.ui_accent_color = accent);
        let s = app.settings_manager.get().clone();
        crate::ui_theme::apply_polished_style_with_options(
            ctx,
            &s.ui_accent_color,
            s.ui_scale,
            s.ui_text_scale,
        );
    }

    ui.separator();
    ui.label(egui::RichText::new("Main view").strong());
    let settings = app.settings_manager.get().clone();
    let mut bg = settings.main_view_background.clone();
    ui.horizontal(|ui| {
        ui.label("Background");
        egui::ComboBox::from_id_salt("main_view_background")
            .selected_text(if bg.starts_with("custom:#") {
                "Custom"
            } else if bg == "light" {
                "Light"
            } else if bg == "theme" {
                "Follow theme"
            } else {
                "Dark"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut bg, "theme".to_string(), "Follow theme");
                ui.selectable_value(&mut bg, "dark".to_string(), "Dark");
                ui.selectable_value(&mut bg, "light".to_string(), "Light");
                ui.selectable_value(&mut bg, "custom:#202020".to_string(), "Custom");
            });
        if bg.starts_with("custom:#") {
            let mut color = parse_setting_color(&bg);
            if ui.color_edit_button_srgba(&mut color).changed() {
                bg = format!(
                    "custom:#{:02X}{:02X}{:02X}",
                    color.r(),
                    color.g(),
                    color.b()
                );
            }
        }
    });
    if bg != settings.main_view_background {
        let _ = app.settings_manager.update(|s| s.main_view_background = bg);
    }
    let mut checker = settings.show_alpha_checkerboard;
    if crate::ui_theme::switch(ui, &mut checker, "Show transparency checkerboard") {
        let _ = app
            .settings_manager
            .update(|s| s.show_alpha_checkerboard = checker);
    }
    let settings = app.settings_manager.get().clone();
    let mut checker_size = settings.alpha_checkerboard_size;
    ui.add(
        crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut checker_size, 12.0..=64.0)
            .step_by(2.0)
            .text("Checker size"),
    ));
    let mut checker_brightness = settings.alpha_checkerboard_brightness;
    ui.add(
        crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut checker_brightness, 0.25..=1.5)
            .step_by(0.05)
            .text("Checker brightness"),
    ));
    if checker_size != settings.alpha_checkerboard_size
        || checker_brightness != settings.alpha_checkerboard_brightness
    {
        let _ = app.settings_manager.update(|s| {
            s.alpha_checkerboard_size = checker_size;
            s.alpha_checkerboard_brightness = checker_brightness;
        });
    }
}

fn parse_setting_color(value: &str) -> egui::Color32 {
    let hex = value.split_once('#').map(|(_, h)| h).unwrap_or("00857B");
    if hex.len() == 6 {
        egui::Color32::from_rgb(
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(133),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(123),
        )
    } else {
        egui::Color32::from_rgb(0, 133, 123)
    }
}

fn theme_label_str(theme: &str) -> &'static str {
    match theme {
        "system" => "Follow System",
        "light" => "Light",
        "dark" => "Dark",
        _ => "Unknown",
    }
}

// Startup Fullscreen
fn render_startup_fullscreen(app: &mut ViewerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        let mut start_fs = app.settings_manager.get().start_fullscreen;

        if crate::ui_theme::switch(ui, &mut start_fs, "Start in Fullscreen Mode") {
            let _ = app.settings_manager.update(|settings| {
                settings.start_fullscreen = start_fs;
            });
        }
    });
    ui.add_space(8.0);
    ui.label("Note: When started in fullscreen, Escape will close the app.");
}

/// Render sort method in a horizontal layout (for compact settings panels)
pub fn sort_method_horizontal_ui(app: &mut ViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    let old_method = app.sort_method;

    ui.horizontal(|ui| {
        ui.label("Sort:");

        egui::ComboBox::from_id_salt("sort_method_combobox")
            .selected_text(format!("{:?}", app.sort_method))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.sort_method, SortMethod::Natural, "Natural");
                ui.selectable_value(&mut app.sort_method, SortMethod::DateNewest, "Newest");
                ui.selectable_value(&mut app.sort_method, SortMethod::DateOldest, "Oldest");
                ui.selectable_value(&mut app.sort_method, SortMethod::Shuffle, "Shuffle");
            });
    });

    if old_method != app.sort_method {
        // Sort the collection currently shown, rather than reopening the current
        // file's parent directory (which breaks queue/local-list views).
        let selected_id = app
            .image_entries
            .get(app.current_index)
            .map(|entry| entry.get_id());
        if !app.image_entries.is_empty() {
            crate::sort::sort_images(&mut app.image_entries, app.sort_method);
            if let Some(id) = selected_id {
                if let Some(index) = app
                    .image_entries
                    .iter()
                    .position(|entry| entry.get_id() == id)
                {
                    app.current_index = index;
                }
            }
        } else {
            app.handle_command(ctx, ViewerCommand::RebuildIndex);
        }
        app.image_cache.clear();
        app.preloading_indices.clear();
        app.preload_tasks.clear();
        app.preload_generation = app.preload_generation.wrapping_add(1);
        app.sidebar_last_current_index = None;
        if !app.image_entries.is_empty() {
            app.load_current_image_with_cache();
        }
    }
}
