use crate::app::types::ViewerApp;
use eframe::egui;
use std::num::NonZeroUsize;
use std::time::Instant;

const RESET_CONFIRM_ID: &str = "settings_reset_all_confirmation";
const RESET_FEEDBACK_ID: &str = "settings_reset_all_feedback";

pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        crate::app::ui::paint_svg_icon(
            ui,
            icon_rect,
            crate::app::ui::SvgIcon::Reset,
            ui.visuals().text_color(),
        );
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Reset All Settings to Default").size(13.0),
                )
                .min_size(egui::vec2(220.0, 32.0)),
            )
            .clicked()
        {
            ctx.data_mut(|data| data.insert_temp(egui::Id::unique(RESET_CONFIRM_ID), true));
        }
    });

    let mut confirmation_open = ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::unique(RESET_CONFIRM_ID))
            .unwrap_or(false)
    });
    if confirmation_open {
        let mut window_open = confirmation_open;
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Reset all settings?")
            .id(egui::Id::unique("settings_reset_all_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.label("This restores application preferences to their defaults.");
                ui.label("Your images, folders, queues, and favorites will not be deleted.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                    if ui.button("Reset settings").clicked() {
                        confirmed = true;
                    }
                });
            });
        confirmation_open = window_open && !cancelled;

        if confirmed {
            let defaults = crate::settings::AppSettings::default();
            let update_result = app.settings_manager.update(|settings| {
                *settings = defaults.clone();
            });
            // Keep reset changes in memory while the Settings panel is open. The
            // normal save path writes them after the panel closes.
            apply_runtime_defaults(app, ctx, &defaults);

            let feedback = match update_result {
                Ok(()) => (
                    false,
                    "Settings were reset. Changes will be written when Settings closes. Restart to apply startup-only options, the thumbnail worker pool, and default window placement.".to_owned(),
                ),
                Err(error) => (true, format!("Could not reset settings: {error}")),
            };
            ctx.data_mut(|data| data.insert_temp(egui::Id::unique(RESET_FEEDBACK_ID), feedback));
            confirmation_open = false;
        }
    }
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(RESET_CONFIRM_ID), confirmation_open));

    if let Some((is_error, message)) =
        ctx.data(|data| data.get_temp::<(bool, String)>(egui::Id::unique(RESET_FEEDBACK_ID)))
    {
        let color = if is_error {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().weak_text_color()
        };
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(message).small().color(color));
        });
    }

    ui.add_space(4.0);
}

fn apply_runtime_defaults(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    defaults: &crate::settings::AppSettings,
) {
    app.load_frame_limiter_settings();
    app.load_slideshow_settings();
    app.slideshow_settings_loaded = true;
    app.slideshow_enabled = false;
    app.power_inhibitor.set_active(false);
    app.slideshow_has_advanced = false;
    app.slideshow_previous_texture = None;
    app.slideshow_previous_rect = None;
    app.slideshow_transition_started = None;
    app.slideshow_last_advance = Instant::now();
    app.win98_dialup_reveal_pending = false;
    app.win98_dialup_previous_texture_id = None;
    app.win98_dialup_reveal_started = None;

    app.b_ctrl_invert = defaults.b_ctrl_invert;
    app.compactui = defaults.compact_mode;
    app.window_always_on_top = false;
    // Prevent the deferred startup restore logic from re-maximizing the window
    // or re-entering fullscreen after the reset commands have been sent.
    app.startup_restore_maximize_pending = false;
    // Reset defaults intentionally clear saved window position and size.
    // Preserve those `None` values during this session's close handling instead
    // of immediately writing the still-running native window's old geometry.
    app.preserve_reset_window_geometry =
        defaults.window_pos.is_none() && defaults.window_size.is_none();
    app.startup_fullscreen_handled = true;
    app.startup_window_placement_handled = true;
    app.main_view_pan_drag_active = false;
    app.main_view_window_drag_active = false;
    app.window_mouse_operation_active = false;
    app.sidebar_resize_dragging = false;
    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
        egui::WindowLevel::Normal,
    ));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(defaults.show_titlebar));

    // Rebuild the image LRU at the default capacity and invalidate all running
    // speculative work. Resetting only the settings struct would otherwise leave
    // the runtime cache radius, worker limits, and preload window at old values.
    let radius = defaults.cache_radius.clamp(1, 100);
    let cache_capacity = (radius * 2 + 2).max(3);
    app.preload_generation = app.preload_generation.wrapping_add(1);
    app.cancel_all_preload_tasks();
    app.preloading_indices.clear();
    app.preload_skipped.clear();
    app.preload_working = false;
    app.should_stop_caching = false;
    app.preload_origin = app.current_index;
    app.last_preload_start = None;

    app.cache_radius = radius;
    app.cache_delta_factor = defaults.cache_delta_factor;
    app.delta_threshold = ((radius as f32 * defaults.cache_delta_factor).round() as usize).max(1);
    app.max_cache_task = defaults.max_cache_task;
    app.max_cache_size = cache_capacity;
    app.image_cache = lru::LruCache::new(
        NonZeroUsize::new(cache_capacity).expect("cache capacity must be non-zero"),
    );
    app.sync_image_cache_capacity();
    app.preloaded_mipmap_pixels = None;
    app.current_mipmap_state = None;
    if let Ok(mut gif_cache) = app.gif_detection_cache.lock() {
        gif_cache.clear();
    }

    // Update all context-backed appearance preferences now rather than waiting
    // for a later frame or leaving fonts/theme from the previous look active.
    ctx.set_theme(crate::ui_theme::effective_theme_preference(
        &defaults.theme_preference,
        &defaults.look_preference,
    ));
    crate::ui_theme::set_look_profile(ctx, &defaults.look_preference);
    ctx.set_fonts(crate::ui_theme::font_definitions_for_profile(
        &defaults.look_preference,
    ));
    crate::ui_theme::set_os_background_blur_preference(ctx, defaults.os_background_blur);
    crate::ui_theme::set_disable_os_background_blur_in_fullscreen(
        ctx,
        defaults.disable_os_background_blur_in_fullscreen,
    );
    crate::ui_theme::sync_os_background_blur(ctx);
    crate::ui_theme::platform_backdrop::set_native_window_shape(defaults.native_window_rounding);
    crate::app::ui::set_desktop_menu_bar_preference(ctx, &defaults.desktop_menu_bar_preference);
    crate::ui_theme::apply_polished_style_with_options(
        ctx,
        &defaults.ui_accent_color,
        defaults.ui_scale,
        defaults.ui_text_scale,
    );

    app.update_window_title(ctx);
    if !app.image_entries.is_empty() {
        app.current_index = app
            .current_index
            .min(app.image_entries.len().saturating_sub(1));
        app.load_current_image_with_cache();
    }
    ctx.request_repaint();
}
