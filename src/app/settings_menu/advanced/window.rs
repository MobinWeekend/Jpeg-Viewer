use crate::app::types::ViewerApp;
use eframe::egui;

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("▣ Window frame").strong(), |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let mut show_titlebar = app.settings_manager.get().show_titlebar;
            if crate::ui_theme::switch(ui, &mut show_titlebar, "Use native window title bar") {
                let _ = app.settings_manager.update(|settings| {
                    settings.show_titlebar = show_titlebar;
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(show_titlebar));
                ctx.request_repaint();
            }
        });

        let current = app.settings_manager.get().custom_window_frame_thickness;
        let mut thickness = current;
        ui.add_enabled_ui(!app.settings_manager.get().show_titlebar, |ui| {
            let response = ui.add(
                crate::ui_theme::BorderedSlider::new(
                    egui::Slider::new(&mut thickness, 1.0..=4.0)
                        .step_by(1.0)
                        .text("Custom frame thickness")
                        .suffix(" px"),
                ),
            );
            if response.changed() {
                let _ = app.settings_manager.update(|settings| {
                    settings.custom_window_frame_thickness = thickness.clamp(2.0, 12.0);
                });
                ctx.request_repaint();
            }
        });
        ui.label(egui::RichText::new("The custom frame provides the resize hit area when native window decorations are disabled.").small().weak());

        let mut center_title = app.settings_manager.get().center_custom_window_title;
        if crate::ui_theme::switch(ui, &mut center_title, "Center window title in available space") {
            let _ = app.settings_manager.update(|settings| settings.center_custom_window_title = center_title);
            ctx.request_repaint();
        }
        let mut rename_from_title = app.settings_manager.get().rename_file_from_titlebar;
        if crate::ui_theme::switch(ui, &mut rename_from_title, "Click window title to rename current file") {
            let _ = app.settings_manager.update(|settings| settings.rename_file_from_titlebar = rename_from_title);
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new("Title clicking uses the existing rename dialog and only works for ordinary filesystem images.").small().weak());

        let mut native_rounding = app.settings_manager.get().native_window_rounding;
        if crate::ui_theme::switch(ui, &mut native_rounding, "OS-managed rounded window corners") {
            let _ = app.settings_manager.update(|settings| settings.native_window_rounding = native_rounding);
            crate::ui_theme::platform_backdrop::set_native_window_shape(native_rounding);
            ctx.request_repaint();
        }
        ui.label(egui::RichText::new("Requests native corner rounding where the OS exposes it. The OS shadow is retained where supported. Linux compositors, especially under Wayland, own window shape and shadow policy and may ignore application requests.").small().weak());

        let current_integrated = app.settings_manager.get().custom_titlebar_navigation;
        let mut integrated = current_integrated;
        ui.add_enabled_ui(!app.settings_manager.get().show_titlebar, |ui| {
            if crate::ui_theme::switch(ui, &mut integrated, "Integrate navigation into custom title bar") {
                let _ = app.settings_manager.update(|settings| {
                    settings.custom_titlebar_navigation = integrated;
                    settings.custom_titlebar_navigation_preference_configured = true;
                });
                ctx.request_repaint();
            }
        });
        ui.label(egui::RichText::new("When enabled, the sidebar, menu, image navigation, slideshow start, and zoom controls move into the custom title bar. Windows 98, COSMIC, and GNOME profiles default to the standard layout; you can enable integration here.").small().weak());

        let current_mode = app.settings_manager.get().title_bar_color_mode.clone();
        let mut title_bar_color_mode = current_mode.clone();
        ui.horizontal(|ui| {
            ui.label("Title bar color");
            egui::ComboBox::from_id_salt("title_bar_color_mode")
                .selected_text(match title_bar_color_mode.as_str() {
                    "main_view" => "Follow main view background",
                    "main_color" => "Main color",
                    _ => "System default",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut title_bar_color_mode,
                        "system".to_string(),
                        "System default",
                    );
                    ui.selectable_value(
                        &mut title_bar_color_mode,
                        "main_view".to_string(),
                        "Follow main view background",
                    );
                    ui.selectable_value(
                        &mut title_bar_color_mode,
                        "main_color".to_string(),
                        "Main color",
                    );
                });
        });
        if title_bar_color_mode != current_mode {
            let _ = app.settings_manager.update(|settings| {
                settings.title_bar_color_mode = title_bar_color_mode;
            });
            ctx.request_repaint();
        }
        ui.label(
            egui::RichText::new(
                "For custom title bars, System default uses the active UI profile's neutral surface. Native caption colors use the OS default unless an override is supported (Windows 11 build 22000+); other window managers keep their own title-bar colors.",
            )
            .small()
            .weak(),
        );

        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Reset Window Position").size(13.0))
                        .min_size(egui::vec2(150.0, 28.0)),
                )
                .clicked()
            {
                let _ = app.settings_manager.update(|settings| {
                    settings.window_pos = None;
                });
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Reset Window Size").size(13.0))
                        .min_size(egui::vec2(150.0, 28.0)),
                )
                .clicked()
            {
                let _ = app.settings_manager.update(|settings| {
                    settings.window_size = None;
                    settings.window_maximized = false;
                });
            }
        });
        ui.add_space(4.0);
    });
}
