#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod archive;
pub mod constants;
mod decoder;
mod gif;
mod helpers;
mod image_core;
mod image_entry;
mod loader;
mod settings;
mod power;
mod platform_backdrop;
mod shortcuts;
mod sort;
mod slideshow;
mod ui_theme;

use app::ViewerApp;
use eframe::egui;
use raw_window_handle::HasWindowHandle;

fn main() -> eframe::Result<()> {
    let settings_manager = settings::SettingsManager::new();
    let app_settings = settings_manager.get();

    let icon = image::load_from_memory(include_bytes!("../assets/icons/icon.ico"))
        .expect("Failed to load icon")
        .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let icon = egui::IconData {
        rgba: icon.into_raw(),
        width: icon_width,
        height: icon_height,
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_icon(icon)
        .with_min_inner_size(egui::vec2(512.0, 512.0))
        .with_decorations(app_settings.show_titlebar)
        // Keep the native window alpha-capable on platforms that implement system
        // backdrops. The preference decides whether alpha is actually used.
        .with_transparent(cfg!(any(target_os = "windows", target_os = "macos")))
        .with_resizable(true);

    if !app_settings.start_centered {
        if let Some(pos) = app_settings.window_pos {
            viewport = viewport.with_position(pos);
        }
    }

    if let Some(size) = app_settings.window_size {
        viewport = viewport.with_inner_size(size);
    }

    let options = eframe::NativeOptions {
        viewport,
        // Window geometry is persisted in the app's settings.ini using viewport
        // geometry directly. Disable eframe's second persistence path because its
        // zoom-factor conversion can accumulate size/position drift when UI scale != 1.
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
            crate::platform_backdrop::initialize(
                cc.window_handle().ok().map(|window| window.as_raw()),
            );

            let mut app = ViewerApp::default();

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

            let startup_paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
            if startup_paths.len() == 1 && startup_paths[0].extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq")) {
                app.open_queue_file(&startup_paths[0], &cc.egui_ctx, false);
            } else if startup_paths.len() == 1 {
                app.open_path(startup_paths[0].clone());
            } else if !startup_paths.is_empty() {
                // Process every path passed by the OS (including multiple files dropped on the app icon).
                app.load_dropped_files(startup_paths);
            }

            Ok(Box::new(app))
        }),
    )
}
