//! Platform-aware visual design selection.

use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("Look").strong(), |ui| {
        ui.add_space(4.0);
        let settings = app.settings_manager.get().clone();
        let current = settings.look_preference;
        let mut selected = current.clone();

        egui::ComboBox::from_id_salt("look_profile")
            .selected_text(profile_label(&selected))
            .width(ui.available_width().max(160.0) - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut selected,
                    "auto".to_owned(),
                    "Automatic (detect desktop)",
                );
                ui.selectable_value(&mut selected, "native".to_owned(), "JPEG Viewer");
                ui.selectable_value(&mut selected, "cosmic".to_owned(), "COSMIC design");
                ui.selectable_value(&mut selected, "gnome".to_owned(), "GNOME / Adwaita design");
                ui.selectable_value(&mut selected, "win98".to_owned(), "Windows 98 classic");
            });

        if selected != current {
            let _ = app.settings_manager.update(|settings| {
                settings.look_preference = selected.clone();
                // Entering the classic profile starts in the expected Windows 98
                // light palette once, without restricting later Light/Dark/System choices.
                if selected == "win98" {
                    settings.theme_preference = "light".to_owned();
                }
            });
            crate::ui_theme::set_look_profile(ctx, &selected);
            // Selecting Win98 initializes the theme preference to Light once. The
            // user's subsequent Light/Dark/Follow System selection remains editable.
            let settings = app.settings_manager.get().clone();
            ctx.set_theme(crate::ui_theme::effective_theme_preference(
                &settings.theme_preference,
                &selected,
            ));
            // Apply the selected desktop's system font when available. Missing fonts or
            // fontconfig never block the application and fall back to egui's bundled fonts.
            ctx.set_fonts(crate::ui_theme::font_definitions_for_profile(&selected));
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &settings.ui_accent_color,
                settings.ui_scale,
                settings.ui_text_scale,
            );
        }

        let effective = crate::ui_theme::resolve_look_profile(&selected);
        ui.label(format!("Active profile: {}", effective_label(effective)));
        match effective {
            "cosmic" => {
                ui.label(
                    egui::RichText::new(
                        "Uses COSMIC-style surface layers, rounded controls, spacing, and Open Sans when installed.",
                    )
                    .small()
                    .weak(),
                );
            }
            "gnome" => {
                ui.label(
                    egui::RichText::new(
                        "Uses Adwaita-style surfaces, control states, spacing, desktop accent color, and Adwaita Sans when available.",
                    )
                    .small()
                    .weak(),
                );
            }
            "win98" => {
                ui.label(
                    egui::RichText::new(
                        "Classic gray surfaces, square controls, raised/sunken borders, a full-width menu bar, and a toolbar aligned to the upper-right.",
                    )
                    .small()
                    .weak(),
                );
            }
            _ => {
                ui.label(
                    egui::RichText::new("Uses the original JPEG Viewer visual design.")
                        .small()
                        .weak(),
                );
            }
        }
        if effective == "win98" {
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Dial-up!").strong());
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
                let response = ui.add(
                    crate::ui_theme::BorderedSlider::new(
                        egui::Slider::new(&mut connection, 1..=56)
                            .text("Connection")
                            .suffix(" kbps"),
                    ),
                );
                if response.changed() {
                    let _ = app.settings_manager.update(|s| {
                        s.win98_dialup_connection_kbps = connection.clamp(1, 56);
                    });
                }
            });
            ui.label(egui::RichText::new("Slow connections reveal horizontal image strips with small timing jitters. Higher kbps is faster.").small().weak());
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Main toolbar and navigation placement").strong());
        let placement_current = app
            .settings_manager
            .get()
            .main_ui_placement_preference
            .clone();
        let mut placement_selected = placement_current.clone();
        ui.add_enabled_ui(effective != "win98", |ui| {
            egui::ComboBox::from_id_salt("main_ui_placement_profile")
                .selected_text(if effective == "win98" {
                    "Automatic (Windows 98 layout)"
                } else {
                    placement_label(&placement_selected)
                })
                .width(ui.available_width().max(160.0) - 8.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut placement_selected,
                        "auto".to_owned(),
                        "Automatic (follow look / desktop)",
                    );
                    ui.selectable_value(
                        &mut placement_selected,
                        "native".to_owned(),
                        "JPEG Viewer (original positions)",
                    );
                    ui.selectable_value(
                        &mut placement_selected,
                        "cosmic".to_owned(),
                        "COSMIC (bottom center)",
                    );
                    ui.selectable_value(
                        &mut placement_selected,
                        "gnome".to_owned(),
                        "GNOME (top center)",
                    );
                });
        });
        if placement_selected != placement_current {
            let _ = app.settings_manager.update(|settings| {
                settings.main_ui_placement_preference = placement_selected;
            });
        }
        ui.label(
            egui::RichText::new(
                if effective == "win98" {
                    "Automatic resolves to the classic full-width toolbar below the menu bar in the Windows 98 profile. The saved placement for other looks is retained."
                } else {
                    "Automatic follows the active Look profile. COSMIC places the main controls together at bottom center; GNOME places them together at top center."
                },
            )
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Desktop menu bar").strong());
        let current_menu_bar = app
            .settings_manager
            .get()
            .desktop_menu_bar_preference
            .clone();
        let mut selected_menu_bar = current_menu_bar.clone();
        ui.add_enabled_ui(effective != "win98", |ui| {
            egui::ComboBox::from_id_salt("desktop_menu_bar_preference")
                .selected_text(if effective == "win98" { "Automatic (Windows 98 menu bar)" } else { menu_bar_label(&selected_menu_bar) })
                .width(ui.available_width().max(160.0) - 8.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut selected_menu_bar,
                        "auto".to_owned(),
                        "Automatic (COSMIC / GNOME)",
                    );
                    ui.selectable_value(&mut selected_menu_bar, "on".to_owned(), "On");
                    ui.selectable_value(&mut selected_menu_bar, "off".to_owned(), "Off");
                });
        });
        if selected_menu_bar != current_menu_bar {
            let _ = app.settings_manager.update(|settings| {
                settings.desktop_menu_bar_preference = selected_menu_bar.clone();
            });
            crate::app::ui::set_desktop_menu_bar_preference(ctx, &selected_menu_bar);
            ctx.request_repaint();
        }
        ui.label(
            egui::RichText::new(if effective == "win98" {
                "Automatic resolves to the classic menu bar in the Windows 98 profile. The saved preference for other looks is retained."
            } else {
                "Automatic enables the menu bar for COSMIC and GNOME looks. On forces it on for any look; Off keeps the floating controls instead."
            })
            .small()
            .weak(),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Navigation UI auto-hide").strong());
        let current_settings = app.settings_manager.get().clone();
        let current_auto_hide = current_settings.navigation_ui_auto_hide_preference.clone();
        let mut selected_auto_hide = current_auto_hide.clone();
        egui::ComboBox::from_id_salt("navigation_ui_auto_hide_preference")
            .selected_text(auto_hide_label(&selected_auto_hide))
            .width(ui.available_width().max(160.0) - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected_auto_hide, "auto".to_owned(), "Automatic");
                ui.selectable_value(&mut selected_auto_hide, "on".to_owned(), "On");
                ui.selectable_value(&mut selected_auto_hide, "off".to_owned(), "Off");
            });
        if selected_auto_hide != current_auto_hide {
            let _ = app.settings_manager.update(|settings| {
                settings.navigation_ui_auto_hide_preference = selected_auto_hide;
            });
            ctx.request_repaint();
        }
        let settings = app.settings_manager.get().clone();
        let auto_hide_enabled = crate::ui_theme::navigation_auto_hide_enabled(&settings, ctx);
        let mut hide_delay_ms = settings.navigation_ui_auto_hide_ms;
        ui.add_enabled_ui(auto_hide_enabled, |ui| {
            let response = ui.add(
                crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut hide_delay_ms, 500..=5000)
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
        let auto_hide_note = match settings.navigation_ui_auto_hide_preference.as_str() {
            "on" => "Auto-hide is forced on for every toolbar placement.",
            "off" => "Auto-hide is disabled; navigation controls remain visible.",
            _ => {
                if crate::ui_theme::resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx) == "native" {
                    "Automatic is enabled with the original JPEG Viewer placement and disabled for COSMIC/GNOME-style placements."
                } else {
                    "Automatic is disabled for this desktop-style placement. Select On to force auto-hide."
                }
            }
        };
        ui.label(egui::RichText::new(auto_hide_note).small().weak());

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Background blur / glass").strong());
        let mut background_blur = app.settings_manager.get().os_background_blur;
        if crate::ui_theme::switch(ui, &mut background_blur, "Use OS/DE blur or glass when available") {
            let _ = app.settings_manager.update(|settings| {
                settings.os_background_blur = background_blur;
            });
            crate::ui_theme::set_os_background_blur_preference(ctx, background_blur);
            let settings = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &settings.ui_accent_color,
                settings.ui_scale,
                settings.ui_text_scale,
            );
            ctx.request_repaint();
        }
        ui.label(
            egui::RichText::new(
                "Requests the native window backdrop on supported Windows/macOS versions. Elsewhere, uses translucent surfaces where practical; actual compositor blur depends on the desktop compositor.",
            )
            .small()
            .weak(),
        );

        let mut disable_blur_fullscreen = app.settings_manager.get().disable_os_background_blur_in_fullscreen;
        if crate::ui_theme::switch(ui, &mut disable_blur_fullscreen, "Disable OS/DE blur or glass in fullscreen") {
            let _ = app.settings_manager.update(|settings| {
                settings.disable_os_background_blur_in_fullscreen = disable_blur_fullscreen;
            });
            crate::ui_theme::set_disable_os_background_blur_in_fullscreen(ctx, disable_blur_fullscreen);
            let settings = app.settings_manager.get().clone();
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &settings.ui_accent_color,
                settings.ui_scale,
                settings.ui_text_scale,
            );
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new("When enabled, fullscreen uses the opaque fallback and native backdrop effects are suspended. The effect is restored when leaving fullscreen.").small().weak());

        let settings = app.settings_manager.get().clone();
        let mut opacity_percent = (settings.main_view_background_opacity.clamp(0.15, 1.0) * 100.0).round() as i32;
        ui.add_enabled_ui(crate::ui_theme::os_background_blur_enabled(ctx), |ui| {
            let response = ui.add(
                crate::ui_theme::BorderedSlider::new(
                    egui::Slider::new(&mut opacity_percent, 15..=100)
                        .step_by(5.0)
                        .text("Main view opacity")
                        .suffix("%"),
                ),
            );
            if response.changed() {
                let opacity = (opacity_percent as f32 / 100.0).clamp(0.15, 1.0);
                let _ = app.settings_manager.update(|settings| {
                    settings.main_view_background_opacity = opacity;
                });
                ctx.request_repaint();
            }
        });
        ui.label(egui::RichText::new("Lower values reveal more of the available OS/DE backdrop behind the main-view background. Disabled when glass is off or the Windows 98 profile is active.").small().weak());

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "System window decorations and platform file dialogs are used where supported. Widgets remain egui-rendered; native GTK/libadwaita or COSMIC widget trees cannot be plugged into an eframe window at runtime.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);
        super::advanced::window::render(app, ui, ctx);
        ui.add_space(4.0);
        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new("Theme and UI appearance").strong());
        crate::app::ui::settings_menu::general::render_theme(app, ui, ctx);
        ui.add_space(4.0);
        crate::app::ui::settings_menu::general::render_ui_appearance(app, ui, ctx);

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
