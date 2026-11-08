#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod decoder;
mod imaging;
mod input;
mod settings;
mod platform;
mod ui_theme;

use app::ViewerApp;
use eframe::egui;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

fn open_startup_paths(app: &mut ViewerApp, startup_paths: Vec<std::path::PathBuf>, ctx: &egui::Context) {
    if startup_paths.len() == 1
        && startup_paths[0].extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq"))
    {
        app.open_queue_file(&startup_paths[0], ctx, false);
    } else if startup_paths.len() == 1 {
        app.open_path(startup_paths[0].clone());
    } else if !startup_paths.is_empty() {
        app.load_dropped_files(startup_paths);
    }
}

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get().clone();
    let startup_paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    let instance_server = if app_settings.single_instance_enabled {
        match crate::platform::instance::launch_or_forward(settings_manager.path(), startup_paths.clone()) {
            Ok(crate::platform::instance::LaunchOutcome::Primary(server)) => Some(server),
            Ok(crate::platform::instance::LaunchOutcome::Forwarded) => return Ok(()),
            Err(error) => {
                eprintln!("Could not contact the running JPEG Viewer instance: {error}");
                // Do not start a second process when the single-instance lock is held
                // but the local forwarding endpoint is temporarily unavailable.
                return Ok(());
            }
        }
    } else {
        None
    };

    let icon = image::load_from_memory(include_bytes!("../assets/icons/icon.png"))
        .expect("Failed to load icon")
        .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let icon = egui::IconData {
        rgba: icon.into_raw(),
        width: icon_width,
        height: icon_height,
    };

    let mut viewport = egui::ViewportBuilder::default()
        // Match the Linux desktop entry ID so Wayland compositors (including COSMIC)
        // associate the window with its launcher and can restore minimized windows.
        .with_app_id("com.jpegviewer.JpegViewer")
        .with_icon(icon)
        .with_min_inner_size(egui::vec2(512.0, 512.0))
        .with_decorations(app_settings.show_titlebar)
        // Keep an alpha-capable native surface on all desktop platforms. Linux
        // Wayland uses ext-background-effect-v1 when supported; other platforms
        // retain an opaque clear color unless their native backdrop is active.
        .with_transparent(true)
        .with_resizable(true);

    if !app_settings.start_centered {
        if let Some(pos) = app_settings.window_pos {
            viewport = viewport.with_position(pos);
        }
    }

    if let Some(size) = app_settings.window_size {
        viewport = viewport.with_inner_size(size);
    }

    if app_settings.window_maximized && !app_settings.start_fullscreen {
        viewport = viewport.with_maximized(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        // Geometry is persisted in settings.ini by save_window_state(), which
        // normalizes egui's zoom-scaled viewport coordinates before saving. Disable
        // eframe's separate persistence path to avoid competing geometry restorations.
        persist_window: false,
        ..Default::default()
    };
    // Build fonts once before starting the event loop. Desktop font discovery is
    // runtime-only and silently falls back to egui defaults when unavailable.
    let initial_font_definitions =
        crate::ui_theme::font_definitions_for_profile(&app_settings.look_preference);

    eframe::run_native(
        "JPEG Viewer",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_fonts(initial_font_definitions);
            crate::ui_theme::platform_backdrop::initialize(
                cc.window_handle().ok().map(|window| window.as_raw()),
                cc.display_handle().ok().map(|display| display.as_raw()),
            );

            crate::ui_theme::platform_backdrop::set_native_window_shape(app_settings.native_window_rounding);

            let mut app = ViewerApp::default();
            app.instance_server = instance_server;
            if let Some(server) = app.instance_server.as_ref() {
                // Wake the event loop when another process forwards an open request.
                server.set_repaint_context(cc.egui_ctx.clone());
            }
            if app.instance_server.is_some() {
                // Delay startup processing slightly so separate file-manager
                // activations can be combined with this process's first path.
                app.enqueue_external_open_paths(startup_paths, &cc.egui_ctx);
            } else {
                open_startup_paths(&mut app, startup_paths, &cc.egui_ctx);
            }

            // Apply saved theme
            let saved_settings = app.settings_manager.get().clone();
            let theme_pref = crate::ui_theme::effective_theme_preference(
                &saved_settings.theme_preference,
                &saved_settings.look_preference,
            );
            cc.egui_ctx.set_theme(theme_pref);
            {
                let settings = app.settings_manager.get().clone();
                crate::ui_theme::set_look_profile(&cc.egui_ctx, &settings.look_preference);
                // Apply the master setting first to avoid briefly enabling a backdrop
                // for users who have explicitly disabled it in their saved preferences.
                crate::ui_theme::set_os_background_blur_preference(
                    &cc.egui_ctx,
                    settings.os_background_blur,
                );
                crate::ui_theme::set_disable_os_background_blur_in_fullscreen(
                    &cc.egui_ctx,
                    settings.disable_os_background_blur_in_fullscreen,
                );
                crate::app::ui::set_desktop_menu_bar_preference(
                    &cc.egui_ctx,
                    &settings.desktop_menu_bar_preference,
                );
                crate::ui_theme::apply_polished_style_with_options(&cc.egui_ctx, &settings.ui_accent_color, settings.ui_scale, settings.ui_text_scale);
            }

            Ok(Box::new(app))
        }),
    )
}
