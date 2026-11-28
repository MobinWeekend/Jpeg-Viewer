#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod decoder;
mod imaging;
mod input;
mod io_metrics;
mod platform;
mod settings;
mod ui_theme;

use app::ViewerApp;
use eframe::egui;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

fn open_startup_paths(
    app: &mut ViewerApp,
    startup_paths: Vec<std::path::PathBuf>,
    ctx: &egui::Context,
) {
    if startup_paths.len() == 1
        && startup_paths[0]
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq"))
    {
        app.open_queue_file(&startup_paths[0], ctx, false);
    } else if startup_paths.len() == 1 {
        app.open_path(startup_paths[0].clone());
    } else if !startup_paths.is_empty() {
        app.load_dropped_files(startup_paths);
    }
}

/// Shared application icon data for the root and utility native viewports.
pub(crate) fn application_window_icon_data() -> egui::IconData {
    static ICON: std::sync::OnceLock<egui::IconData> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let image = image::load_from_memory(include_bytes!("../assets/icons/icon.png"))
            .expect("Failed to load JPEG Viewer window icon")
            .into_rgba8();
        let (width, height) = image.dimensions();
        egui::IconData {
            rgba: image.into_raw(),
            width,
            height,
        }
    })
    .clone()
}

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get().clone();
    let startup_paths: Vec<std::path::PathBuf> =
        std::env::args_os().skip(1).map(Into::into).collect();
    let instance_server = if app_settings.single_instance_enabled {
        match crate::platform::instance::launch_or_forward(
            settings_manager.path(),
            startup_paths.clone(),
        ) {
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

    let icon = application_window_icon_data();

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
        // Keep file drops enabled for platform backends. On native Wayland,
        // the pinned winit patch below supplies the missing data-device events.
        .with_drag_and_drop(true)
        .with_resizable(true);

    // A maximized Win32 window must let the window manager calculate its
    // monitor/work-area bounds. Combining a restored inner size (and position)
    // with `with_maximized(true)` can leave a decorated window partially sized
    // on Windows 10/11, with unused strips of the monitor exposed. Saved geometry
    // is only applied when starting in the restored state.
    let start_maximized = app_settings.window_maximized && !app_settings.start_fullscreen;
    if !start_maximized {
        if !app_settings.start_centered
            && let Some(pos) = app_settings.window_pos
        {
            viewport = viewport.with_position(pos);
        }
        if let Some(size) = app_settings.window_size {
            viewport = viewport.with_inner_size(size);
        }
    } else {
        // Place the initial native window on the selected display before asking
        // the window manager to maximize it. Moving an already-maximized window
        // is not reliable across all desktop environments.
        let monitors = crate::platform::monitors::available_monitors();
        let saved_window_rect = app_settings
            .window_pos
            .zip(app_settings.window_size)
            .map(|(position, size)| {
                egui::Rect::from_min_size(
                    egui::pos2(position[0], position[1]),
                    egui::vec2(size[0], size[1]),
                )
            });
        if let Some(monitor) = crate::platform::monitors::resolve_default_monitor_at(
            &monitors,
            &app_settings.window_default_monitor,
            saved_window_rect,
        ) {
            viewport = viewport.with_position(
                egui::pos2(monitor.x + 24.0, monitor.y + 24.0),
            );
        }
        viewport = viewport.with_maximized(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        // The custom progressive mipmap path uses egui_glow callbacks and GL textures.
        renderer: eframe::Renderer::Glow,
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
            let raw_display_handle = cc.display_handle().ok().map(|display| display.as_raw());
            #[cfg(target_os = "linux")]
            crate::platform::wayland_pointer_recovery::initialize(raw_display_handle.clone());

            crate::ui_theme::platform_backdrop::initialize(
                cc.window_handle().ok().map(|window| window.as_raw()),
                raw_display_handle,
            );

            crate::ui_theme::platform_backdrop::set_native_window_shape(
                app_settings.native_window_rounding,
            );

            let mut app = ViewerApp::new(settings_manager);
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
                crate::ui_theme::apply_polished_style_with_options(
                    &cc.egui_ctx,
                    &settings.ui_accent_color,
                    settings.ui_scale,
                    settings.ui_text_scale,
                );
            }

            Ok(Box::new(app))
        }),
    )
}
