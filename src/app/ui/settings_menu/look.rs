//! Appearance, layout, and window-specific settings.
//!
//! These groups are separated so the former, oversized Look section is easier to scan.

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    render_appearance(app, ui, ctx);
    render_layout(app, ui, ctx);
    render_window(app, ui, ctx);
}

fn render_appearance(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("🎨 Appearance").strong(), |ui| {
        ui.add_space(4.0);
        let settings = app.settings_manager.get().clone();
        let current = settings.look_preference.clone();
        let mut selected = current.clone();

        ui.horizontal(|ui| {
            ui.label("Look profile");
            egui::ComboBox::from_id_salt("look_profile")
                .selected_text(profile_label(&selected))
                .width((ui.available_width() - 8.0).max(150.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, "auto".to_owned(), "Automatic (detect desktop)");
                    ui.selectable_value(&mut selected, "native".to_owned(), "JPEG Viewer");
                    ui.selectable_value(&mut selected, "cosmic".to_owned(), "COSMIC design");
                    ui.selectable_value(&mut selected, "gnome".to_owned(), "GNOME / Adwaita design");
                    ui.selectable_value(&mut selected, "win98".to_owned(), "Windows 98 classic");
                });
        });

        if selected != current {
            let _ = app.settings_manager.update(|settings| {
                settings.look_preference = selected.clone();
                if selected == "win98" {
                    settings.theme_preference = "light".to_owned();
                }
            });
            crate::ui_theme::set_look_profile(ctx, &selected);
            let settings = app.settings_manager.get().clone();
            ctx.set_theme(crate::ui_theme::effective_theme_preference(
                &settings.theme_preference,
                &selected,
            ));
            ctx.set_fonts(crate::ui_theme::font_definitions_for_profile(&selected));
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &settings.ui_accent_color,
                settings.ui_scale,
                settings.ui_text_scale,
            );
        }

        let effective = crate::ui_theme::resolve_look_profile(&selected);
        ui.label(
            egui::RichText::new(format!("Active profile: {}", effective_label(effective)))
                .small()
                .weak(),
        );
        let description = match effective {
            "cosmic" => "COSMIC-style surfaces, rounded controls, desktop spacing, and Open Sans when installed.",
            "gnome" => "Adwaita-style surfaces, control states, desktop accent color, and Adwaita Sans when available.",
            "win98" => "Classic gray surfaces, square controls, raised/sunken borders, and the Windows 98 menu layout.",
            _ => "The original JPEG Viewer visual design.",
        };
        ui.label(egui::RichText::new(description).small().weak());

        if effective == "win98" {
            ui.add_space(6.0);
            ui.separator();
            ui.label(egui::RichText::new("☎ Windows 98 dial-up effect").strong());
            let settings = app.settings_manager.get().clone();
            let mut enabled = settings.win98_dialup_enabled;
            if crate::ui_theme::switch(ui, &mut enabled, "Reveal images like a dial-up download") {
                let _ = app.settings_manager.update(|s| {
                    s.win98_dialup_enabled = enabled;
                    s.win98_dialup_preference_configured = true;
                });
                if !enabled {
                    app.win98_dialup_reveal_started = None;
                    app.win98_dialup_reveal_pending = false;
                }
                ctx.request_repaint();
            }
            let mut connection = settings.win98_dialup_connection_kbps;
            ui.add_enabled_ui(enabled, |ui| {
                let response = ui.add(crate::ui_theme::BorderedSlider::new(
                    egui::Slider::new(&mut connection, 1..=56)
                        .text("Connection")
                        .suffix(" kbps"),
                ));
                if response.changed() {
                    let _ = app.settings_manager.update(|s| {
                        s.win98_dialup_connection_kbps = connection.clamp(1, 56);
                    });
                }
            });
            ui.label(egui::RichText::new("Slow connections reveal horizontal image strips with small timing jitters. Higher kbps is faster.").small().weak());
        }

        ui.separator();
        ui.label(egui::RichText::new("Theme and UI appearance").strong());
        crate::app::ui::settings_menu::general::render_theme(app, ui, ctx);
        crate::app::ui::settings_menu::general::render_ui_appearance(app, ui, ctx);
    });
}

fn render_layout(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("☷ Layout & Navigation").strong(), |ui| {
        ui.add_space(4.0);
        let effective = crate::ui_theme::current_look_profile(ctx);
        ui.label(egui::RichText::new("Main toolbar and navigation placement").strong());
        let current = app.settings_manager.get().main_ui_placement_preference.clone();
        let mut selected = current.clone();
        ui.add_enabled_ui(effective != "win98", |ui| {
            egui::ComboBox::from_id_salt("main_ui_placement_profile")
                .selected_text(if effective == "win98" { "Automatic (Windows 98 layout)" } else { placement_label(&selected) })
                .width(ui.available_width().max(150.0) - 8.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, "auto".to_owned(), "Automatic (follow look / desktop)");
                    ui.selectable_value(&mut selected, "native".to_owned(), "JPEG Viewer (original positions)");
                    ui.selectable_value(&mut selected, "cosmic".to_owned(), "COSMIC (bottom center)");
                    ui.selectable_value(&mut selected, "gnome".to_owned(), "GNOME (top center)");
                });
        });
        if selected != current {
            let _ = app.settings_manager.update(|settings| {
                settings.main_ui_placement_preference = selected;
            });
        }
        let placement_note = if effective == "win98" {
            "Windows 98 uses its classic full-width toolbar below the menu bar. The saved preference for other looks is retained."
        } else {
            "Automatic follows the active Look profile. COSMIC places controls at bottom center; GNOME places them at top center."
        };
        ui.label(egui::RichText::new(placement_note).small().weak());

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("Desktop menu bar").strong());
        let current = app.settings_manager.get().desktop_menu_bar_preference.clone();
        let mut selected = current.clone();
        ui.add_enabled_ui(effective != "win98", |ui| {
            egui::ComboBox::from_id_salt("desktop_menu_bar_preference")
                .selected_text(if effective == "win98" { "Automatic (Windows 98 menu bar)" } else { menu_bar_label(&selected) })
                .width(ui.available_width().max(150.0) - 8.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, "auto".to_owned(), "Automatic (COSMIC / GNOME)");
                    ui.selectable_value(&mut selected, "on".to_owned(), "On");
                    ui.selectable_value(&mut selected, "off".to_owned(), "Off");
                });
        });
        if selected != current {
            let _ = app.settings_manager.update(|settings| {
                settings.desktop_menu_bar_preference = selected.clone();
            });
            crate::app::ui::set_desktop_menu_bar_preference(ctx, &selected);
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new(if effective == "win98" {
            "The classic menu bar is enabled automatically for this look."
        } else {
            "Automatic enables the menu bar for COSMIC and GNOME. On forces it on for any look; Off keeps floating controls."
        }).small().weak());

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("Navigation UI auto-hide").strong());
        let settings = app.settings_manager.get().clone();
        let current = settings.navigation_ui_auto_hide_preference.clone();
        let mut selected = current.clone();
        egui::ComboBox::from_id_salt("navigation_ui_auto_hide_preference")
            .selected_text(auto_hide_label(&selected))
            .width(ui.available_width().max(150.0) - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected, "auto".to_owned(), "Automatic");
                ui.selectable_value(&mut selected, "on".to_owned(), "On");
                ui.selectable_value(&mut selected, "off".to_owned(), "Off");
            });
        if selected != current {
            let _ = app.settings_manager.update(|settings| {
                settings.navigation_ui_auto_hide_preference = selected;
            });
            ctx.request_repaint();
        }
        let settings = app.settings_manager.get().clone();
        let auto_hide_enabled = crate::ui_theme::navigation_auto_hide_enabled(&settings, ctx);
        let mut hide_delay_ms = settings.navigation_ui_auto_hide_ms;
        ui.add_enabled_ui(auto_hide_enabled, |ui| {
            let response = ui.add(crate::ui_theme::BorderedSlider::new(
                egui::Slider::new(&mut hide_delay_ms, 500..=5000)
                    .step_by(100.0)
                    .text("Hide after")
                    .suffix(" ms"),
            ));
            if response.changed() {
                let _ = app.settings_manager.update(|settings| {
                    settings.navigation_ui_auto_hide_ms = hide_delay_ms.clamp(500, 5000);
                });
            }
        });
        let note = match settings.navigation_ui_auto_hide_preference.as_str() {
            "on" => "Auto-hide is forced on for every toolbar placement.",
            "off" => "Auto-hide is disabled in windowed view; fullscreen temporarily enables it.",
            _ if crate::ui_theme::resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx) == "native" => {
                "Automatic is enabled with the original JPEG Viewer placement and disabled for COSMIC/GNOME-style placements. Fullscreen always enables auto-hide."
            }
            _ => "Automatic is disabled for this desktop-style placement. Select On to force auto-hide; fullscreen enables it temporarily.",
        };
        ui.label(egui::RichText::new(note).small().weak());
    });
}

fn render_window(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("▣ Window & Backdrop").strong(), |ui| {
        ui.add_space(4.0);
        let mut background_blur = app.settings_manager.get().os_background_blur;
        if crate::ui_theme::switch(ui, &mut background_blur, "Use OS/DE blur or glass when available") {
            let _ = app.settings_manager.update(|settings| settings.os_background_blur = background_blur);
            crate::ui_theme::set_os_background_blur_preference(ctx, background_blur);
            let settings = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(ctx, &settings.ui_accent_color, settings.ui_scale, settings.ui_text_scale);
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new("Requests native backdrop effects where supported. Elsewhere, translucent surfaces depend on desktop-compositor support.").small().weak());

        let mut disable_blur_fullscreen = app.settings_manager.get().disable_os_background_blur_in_fullscreen;
        if crate::ui_theme::switch(ui, &mut disable_blur_fullscreen, "Disable OS/DE blur or glass in fullscreen") {
            let _ = app.settings_manager.update(|settings| settings.disable_os_background_blur_in_fullscreen = disable_blur_fullscreen);
            crate::ui_theme::set_disable_os_background_blur_in_fullscreen(ctx, disable_blur_fullscreen);
            let settings = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(ctx, &settings.ui_accent_color, settings.ui_scale, settings.ui_text_scale);
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new("Fullscreen uses the opaque fallback when enabled; the normal backdrop preference is restored on exit.").small().weak());

        let settings = app.settings_manager.get().clone();
        let mut opacity_percent = (settings.main_view_background_opacity.clamp(0.15, 1.0) * 100.0).round() as i32;
        ui.add_enabled_ui(crate::ui_theme::os_background_blur_enabled(ctx), |ui| {
            let response = ui.add(crate::ui_theme::BorderedSlider::new(
                egui::Slider::new(&mut opacity_percent, 15..=100)
                    .step_by(5.0)
                    .text("Main view opacity")
                    .suffix("%"),
            ));
            if response.changed() {
                let opacity = (opacity_percent as f32 / 100.0).clamp(0.15, 1.0);
                let _ = app.settings_manager.update(|settings| settings.main_view_background_opacity = opacity);
                ctx.request_repaint();
            }
        });
        ui.label(egui::RichText::new("Lower values reveal more of the available OS/DE backdrop behind the main-view background.").small().weak());

        ui.add_space(6.0);
        ui.separator();
        super::advanced::window::render(app, ui, ctx);
    });
}

fn profile_label(profile: &str) -> &'static str {
    match profile {
        "native" => "JPEG Viewer",
        "cosmic" => "COSMIC design",
        "gnome" => "GNOME / Adwaita design",
        "win98" => "Windows 98 classic",
        _ => "Automatic (detect desktop)",
    }
}

fn effective_label(profile: &str) -> &'static str {
    match profile {
        "cosmic" => "COSMIC design",
        "gnome" => "GNOME / Adwaita design",
        "win98" => "Windows 98 classic",
        _ => "JPEG Viewer",
    }
}

fn auto_hide_label(preference: &str) -> &'static str {
    match preference {
        "on" => "On",
        "off" => "Off",
        _ => "Automatic",
    }
}

fn placement_label(placement: &str) -> &'static str {
    match placement {
        "native" => "JPEG Viewer (original positions)",
        "cosmic" => "COSMIC (bottom center)",
        "gnome" => "GNOME (top center)",
        _ => "Automatic (follow look / desktop)",
    }
}

fn menu_bar_label(preference: &str) -> &'static str {
    match preference {
        "on" => "On",
        "off" => "Off",
        _ => "Automatic (COSMIC / GNOME)",
    }
}
