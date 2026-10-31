use super::{
    AppSettings, SETTINGS_SCHEMA_VERSION, SettingsManager, WINDOW_GEOMETRY_NORMALIZED_SCHEMA,
};
use ini::Ini;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

impl SettingsManager {
    /// Load settings from an INI file. Any read error is recorded in `error_out`.
    pub(super) fn load_from_file(path: &PathBuf, error_out: &mut Option<String>) -> AppSettings {
        if !path.exists() {
            return AppSettings::default();
        }

        match Ini::load_from_file(path) {
            Ok(conf) => {
                let mut settings = AppSettings::default();
                let mut loaded_schema_version = 0_u32;

                // Get the Settings section
                if let Some(section) = conf.section(Some("Settings")) {
                    // Older files without a version are treated as schema 0. Loading
                    // starts from defaults, so newly introduced keys migrate safely.
                    // Unknown future versions are still read conservatively by known key.
                    if let Some(version) = section.get("schema_version") {
                        match version.parse::<u32>() {
                            Ok(v) => {
                                loaded_schema_version = v;
                                if v > SETTINGS_SCHEMA_VERSION {
                                    eprintln!(
                                        "Settings file schema version {} is newer than supported version {}; known settings will be loaded.",
                                        v, SETTINGS_SCHEMA_VERSION
                                    );
                                }
                            }
                            Err(_) => eprintln!(
                                "Invalid settings schema_version; treating file as legacy settings."
                            ),
                        }
                    }
                    // Read b_ctrl_invert (boolean)
                    if let Some(value) = section.get("b_ctrl_invert") {
                        settings.b_ctrl_invert = Self::parse_bool(value);
                    }
                    // Read window position
                    if let Some(value) = section.get("window_pos") {
                        if let Some((x, y)) = Self::parse_f32_pair(value) {
                            if x.is_finite() && y.is_finite() {
                                settings.window_pos = Some([x, y]);
                            }
                        }
                    }
                    // Read window size
                    if let Some(value) = section.get("window_size") {
                        if let Some((width, height)) = Self::parse_f32_pair(value) {
                            if width.is_finite()
                                && height.is_finite()
                                && width > 100.0
                                && height > 100.0
                            {
                                settings.window_size = Some([width, height]);
                            }
                        }
                    }
                    // Read cache radius
                    if let Some(value) = section.get("cache_radius") {
                        if let Ok(radius) = value.parse::<usize>() {
                            if radius > 0 && radius <= 100 {
                                settings.cache_radius = radius;
                            }
                        }
                    }
                    // Read cache delta_factor
                    if let Some(value) = section.get("cache_delta_factor") {
                        if let Ok(factor) = value.parse::<f32>() {
                            if factor.is_finite() && (0.0..=10.0).contains(&factor) && factor > 0.0
                            {
                                settings.cache_delta_factor = factor;
                            }
                        }
                    }
                    // Read navigation pause in ms
                    if let Some(value) = section.get("navigation_pause_ms") {
                        if let Ok(pausetime) = value.parse::<u64>() {
                            if pausetime > 100 {
                                settings.navigation_pause_ms = pausetime;
                            }
                        }
                    }
                    // Read max_cache_task
                    if let Some(value) = section.get("max_cache_task") {
                        if let Ok(tasks) = value.parse::<u8>() {
                            if tasks > 0 && tasks <= 10 {
                                settings.max_cache_task = tasks;
                            }
                        }
                    }
                    if let Some(value) = section.get("thumbnail_workers") {
                        if let Ok(workers) = value.parse::<usize>() {
                            if (1..=8).contains(&workers) {
                                settings.thumbnail_workers = workers;
                            }
                        }
                    }
                    // Read texture_filter
                    if let Some(value) = section.get("texture_filter") {
                        if matches!(value, "nearest" | "linear" | "mipmap") {
                            settings.texture_filter = value.to_string();
                        }
                    }
                    // Read preload_throttle_ms
                    if let Some(value) = section.get("preload_throttle_ms") {
                        if let Ok(throttle) = value.parse::<u64>() {
                            if throttle >= 10 {
                                settings.preload_throttle_ms = throttle;
                            }
                        }
                    }
                    // Read max_fps
                    if let Some(value) = section.get("max_fps") {
                        if let Ok(fps) = value.parse::<f32>() {
                            if fps.is_finite() && fps >= 0.0 && fps <= 120.0 {
                                settings.max_fps = fps;
                            }
                        }
                    }
                    // Read idle_fps_limit
                    if let Some(value) = section.get("idle_fps_limit") {
                        if let Ok(fps) = value.parse::<f32>() {
                            if fps.is_finite() && fps >= 0.0 && fps <= 120.0 {
                                settings.idle_fps_limit = fps;
                            }
                        }
                    }
                    // Read idle_timeout_ms
                    if let Some(value) = section.get("idle_timeout_ms") {
                        if let Ok(timeout) = value.parse::<u64>() {
                            if timeout >= 100 && timeout <= 10000 {
                                settings.idle_timeout_ms = timeout;
                            }
                        }
                    }
                    // Read unfocused_idle_timeout_ms
                    if let Some(value) = section.get("unfocused_idle_timeout_ms") {
                        if let Ok(timeout) = value.parse::<u64>() {
                            if timeout >= 50 && timeout <= 5000 {
                                settings.unfocused_idle_timeout_ms = timeout;
                            }
                        }
                    }
                    // Read unfocused_idle_fps_limit
                    if let Some(value) = section.get("unfocused_idle_fps_limit") {
                        if let Ok(fps) = value.parse::<f32>() {
                            if fps.is_finite() && fps >= 0.0 && fps <= 120.0 {
                                settings.unfocused_idle_fps_limit = fps;
                            }
                        }
                    }
                    // Slideshow playback state is intentionally transient and is never restored.
                    if let Some(value) = section.get("prevent_sleep_during_slideshow") {
                        settings.prevent_sleep_during_slideshow = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("slideshow_interval_ms") {
                        if let Ok(interval) = value.parse::<u64>() {
                            if interval >= 500 && interval <= 60000 {
                                settings.slideshow_interval_ms = interval;
                            }
                        }
                    }
                    if let Some(value) = section.get("slideshow_loop") {
                        settings.slideshow_loop = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("slideshow_random") {
                        settings.slideshow_random = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("slideshow_transition") {
                        if matches!(value, "none" | "fade" | "slide" | "random") {
                            settings.slideshow_transition = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("slideshow_motion") {
                        if matches!(
                            value,
                            "none"
                                | "smart"
                                | "pan-horizontal"
                                | "pan-vertical"
                                | "zoom-in"
                                | "zoom-out"
                                | "random"
                        ) {
                            settings.slideshow_motion = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("slideshow_motion_strength") {
                        if let Ok(strength) = value.parse::<f32>() {
                            settings.slideshow_motion_strength = strength.clamp(5.0, 40.0);
                        }
                    }
                    if let Some(value) = section.get("win98_dialup_enabled") {
                        settings.win98_dialup_enabled = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("win98_dialup_preference_configured") {
                        settings.win98_dialup_preference_configured = Self::parse_bool(value);
                    }
                    // Earlier releases defaulted Dial-up to off and persisted that
                    // default. Migrate those legacy values to the new default-on
                    // behavior once; a preference chosen in this version is retained.
                    if !settings.win98_dialup_preference_configured {
                        settings.win98_dialup_enabled = true;
                        settings.win98_dialup_preference_configured = true;
                    }
                    if let Some(value) = section.get("win98_dialup_connection_kbps") {
                        if let Ok(speed) = value.parse::<u32>() {
                            if (1..=56).contains(&speed) {
                                settings.win98_dialup_connection_kbps = speed;
                            }
                        }
                    }
                    // Read startup settings
                    if let Some(value) = section.get("start_fullscreen") {
                        settings.start_fullscreen = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("start_centered") {
                        settings.start_centered = Self::parse_bool(value);
                    }
                    // Read virtual_texture_threshold
                    if let Some(value) = section.get("virtual_texture_threshold") {
                        if let Ok(v) = value.parse::<u32>() {
                            if (1024..=16384).contains(&v) {
                                settings.virtual_texture_threshold = v;
                            }
                        }
                    }
                    // Read tile_size
                    if let Some(value) = section.get("tile_size") {
                        if let Ok(v) = value.parse::<u32>() {
                            if [128, 256, 512].contains(&v) {
                                settings.tile_size = v;
                            }
                        }
                    }
                    // Titlebar visibility
                    if let Some(value) = section.get("show_titlebar") {
                        settings.show_titlebar = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("title_bar_color_mode") {
                        if matches!(value, "system" | "main_view" | "main_color") {
                            settings.title_bar_color_mode = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("custom_window_frame_thickness") {
                        if let Ok(thickness) = value.parse::<f32>() {
                            if thickness.is_finite() && (2.0..=12.0).contains(&thickness) {
                                settings.custom_window_frame_thickness = thickness;
                            }
                        }
                    }
                    if let Some(value) = section.get("show_file_sidebar") {
                        settings.show_file_sidebar = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("file_sidebar_width") {
                        if let Ok(width) = value.parse::<f32>() {
                            if width.is_finite() && (180.0..=520.0).contains(&width) {
                                settings.file_sidebar_width = width;
                            }
                        }
                    }
                    if let Some(value) = section.get("settings_sidebar_width") {
                        if let Ok(width) = value.parse::<f32>() {
                            if width.is_finite() && (260.0..=640.0).contains(&width) {
                                settings.settings_sidebar_width = width;
                            }
                        }
                    }
                    if let Some(value) = section.get("compact_mode") {
                        settings.compact_mode = Self::parse_bool(value);
                    }
                    for (key, target) in [
                        ("browse_entry_size", &mut settings.browse_entry_size),
                        ("queue_entry_size", &mut settings.queue_entry_size),
                        ("favorites_entry_size", &mut settings.favorites_entry_size),
                    ] {
                        if let Some(value) = section.get(key) {
                            if let Ok(size) = value.parse::<f32>() {
                                if size.is_finite() && (64.0..=240.0).contains(&size) {
                                    *target = size;
                                }
                            }
                        }
                    }
                    if let Some(value) = section.get("ui_scale") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() && (0.75..=1.50).contains(&v) {
                                settings.ui_scale = v;
                            }
                        }
                    }
                    if let Some(value) = section.get("ui_text_scale") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() && (0.80..=1.40).contains(&v) {
                                settings.ui_text_scale = v;
                            }
                        }
                    }
                    if let Some(value) = section.get("ui_accent_color") {
                        if [
                            "auto",
                            "teal",
                            "blue",
                            "purple",
                            "orange",
                            "rose",
                            "green",
                            "yellow",
                            "red",
                            "win98blue",
                        ]
                        .contains(&value)
                            || (value.starts_with("custom:#")
                                && value.len() == 14
                                && value[8..].chars().all(|c| c.is_ascii_hexdigit()))
                        {
                            settings.ui_accent_color = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("main_view_background") {
                        if value == "dark"
                            || value == "light"
                            || value == "theme"
                            || (value.starts_with("custom:#")
                                && value.len() == 14
                                && value[8..].chars().all(|c| c.is_ascii_hexdigit()))
                        {
                            settings.main_view_background = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("main_view_background_opacity") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() {
                                settings.main_view_background_opacity = v.clamp(0.15, 1.0);
                            }
                        }
                    }
                    if let Some(value) = section.get("show_alpha_checkerboard") {
                        settings.show_alpha_checkerboard = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("alpha_checkerboard_size") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() {
                                settings.alpha_checkerboard_size = v.clamp(8.0, 64.0);
                            }
                        }
                    }
                    if let Some(value) = section.get("alpha_checkerboard_brightness") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() {
                                settings.alpha_checkerboard_brightness = v.clamp(0.25, 1.5);
                            }
                        }
                    }
                    // Tricks decorative effects.
                    if let Some(value) = section.get("tricks_snow_enabled") {
                        settings.tricks_snow_enabled = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("tricks_snow_particle") {
                        let value = value.trim();
                        if value.chars().count() <= 8 {
                            settings.tricks_snow_particle = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("tricks_snow_color") {
                        if value.len() == 7
                            && value.starts_with('#')
                            && value[1..].chars().all(|c| c.is_ascii_hexdigit())
                        {
                            settings.tricks_snow_color = value.to_ascii_uppercase();
                        }
                    }
                    if let Some(value) = section.get("tricks_snow_speed") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() {
                                settings.tricks_snow_speed = v.clamp(0.1, 5.0);
                            }
                        }
                    }
                    if let Some(value) = section.get("tricks_snow_density") {
                        if let Ok(v) = value.parse::<f32>() {
                            if v.is_finite() {
                                settings.tricks_snow_density = v.clamp(0.25, 2.5);
                            }
                        }
                    }
                    if let Some(value) = section.get("tricks_snow_above_image") {
                        settings.tricks_snow_above_image = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("tricks_watermark_enabled") {
                        settings.tricks_watermark_enabled = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("tricks_watermark_text") {
                        settings.tricks_watermark_text = value.to_string();
                    }
                    if let Some(value) = section.get("tricks_watermark_text_color") {
                        if value.len() == 7
                            && value.starts_with('#')
                            && value[1..].chars().all(|c| c.is_ascii_hexdigit())
                        {
                            settings.tricks_watermark_text_color = value.to_ascii_uppercase();
                        }
                    }
                    if let Some(value) = section.get("tricks_watermark_image_path") {
                        settings.tricks_watermark_image_path = value.to_string();
                    }
                    if let Some(value) = section.get("tricks_watermark_mode") {
                        if matches!(value, "anchored" | "bounce") {
                            settings.tricks_watermark_mode = value.to_string();
                        }
                    }
                    for (key, target, min, max) in [
                        (
                            "tricks_watermark_x",
                            &mut settings.tricks_watermark_x,
                            0.0,
                            100.0,
                        ),
                        (
                            "tricks_watermark_y",
                            &mut settings.tricks_watermark_y,
                            0.0,
                            100.0,
                        ),
                        (
                            "tricks_watermark_speed",
                            &mut settings.tricks_watermark_speed,
                            0.1,
                            5.0,
                        ),
                        (
                            "tricks_watermark_opacity",
                            &mut settings.tricks_watermark_opacity,
                            0.05,
                            1.0,
                        ),
                        (
                            "tricks_watermark_size",
                            &mut settings.tricks_watermark_size,
                            32.0,
                            512.0,
                        ),
                        (
                            "tricks_backdrop_blur",
                            &mut settings.tricks_backdrop_blur,
                            8.0,
                            64.0,
                        ),
                        (
                            "tricks_backdrop_tint",
                            &mut settings.tricks_backdrop_tint,
                            0.0,
                            0.85,
                        ),
                        (
                            "tricks_backdrop_opacity",
                            &mut settings.tricks_backdrop_opacity,
                            0.0,
                            1.0,
                        ),
                    ] {
                        if let Some(value) = section.get(key) {
                            if let Ok(v) = value.parse::<f32>() {
                                if v.is_finite() {
                                    *target = v.clamp(min, max);
                                }
                            }
                        }
                    }
                    if let Some(value) = section.get("tricks_backdrop_enabled") {
                        settings.tricks_backdrop_enabled = Self::parse_bool(value);
                    }
                    // theme
                    if let Some(value) = section.get("theme_preference") {
                        if value == "system" || value == "light" || value == "dark" {
                            settings.theme_preference = value.to_string();
                        }
                    }
                    // Look profile. Invalid or absent values safely fall back to automatic detection.
                    if let Some(value) = section.get("look_preference") {
                        if ["auto", "native", "cosmic", "gnome", "win98"].contains(&value) {
                            settings.look_preference = value.to_string();
                        }
                    }
                    // Desktop menu bar can follow the Look profile or be explicitly enabled/disabled.
                    if let Some(value) = section.get("desktop_menu_bar_preference") {
                        if ["auto", "on", "off"].contains(&value) {
                            settings.desktop_menu_bar_preference = value.to_string();
                        }
                    }
                    // Read the newer three-state preference first. In older INI files,
                    // `true` was the default and maps to Automatic; an explicit `false`
                    // remains Off so a user's prior opt-out is preserved.
                    if let Some(value) = section.get("navigation_ui_auto_hide_preference") {
                        if ["auto", "on", "off"].contains(&value) {
                            settings.navigation_ui_auto_hide_preference = value.to_string();
                        }
                    } else if let Some(value) = section.get("navigation_ui_auto_hide") {
                        settings.navigation_ui_auto_hide_preference = if Self::parse_bool(value) {
                            "auto".to_string()
                        } else {
                            "off".to_string()
                        };
                    }
                    if let Some(value) = section.get("navigation_ui_auto_hide_ms") {
                        if let Ok(ms) = value.parse::<u64>() {
                            if (500..=5000).contains(&ms) {
                                settings.navigation_ui_auto_hide_ms = ms;
                            }
                        }
                    }
                    if let Some(value) = section.get("os_background_blur") {
                        settings.os_background_blur = Self::parse_bool(value);
                    }
                    if let Some(value) = section.get("disable_os_background_blur_in_fullscreen") {
                        settings.disable_os_background_blur_in_fullscreen = Self::parse_bool(value);
                    }
                    // Main toolbar/navigation placement is independently selectable.
                    if let Some(value) = section.get("main_ui_placement_preference") {
                        if ["auto", "native", "cosmic", "gnome"].contains(&value) {
                            settings.main_ui_placement_preference = value.to_string();
                        }
                    }
                }

                // Before schema 4, geometry was saved directly from egui's viewport
                // rectangles. Those coordinates are in UI points and include ui_scale,
                // while ViewportBuilder consumes native logical points. Convert existing
                // values once using the persisted scale to keep the user's current window
                // approximately where/what it was before the migration.
                if loaded_schema_version < WINDOW_GEOMETRY_NORMALIZED_SCHEMA {
                    let scale = settings.ui_scale.clamp(0.75, 1.50);
                    if (scale - 1.0).abs() > f32::EPSILON {
                        if let Some([x, y]) = settings.window_pos {
                            settings.window_pos = Some([x * scale, y * scale]);
                        }
                        if let Some([width, height]) = settings.window_size {
                            settings.window_size = Some([width * scale, height * scale]);
                        }
                    }
                }

                settings
            }
            Err(e) => {
                eprintln!("Error loading settings from {:?}: {}", path, e);
                *error_out = Some(format!("Could not read {}: {}", path.display(), e));
                // Preserve the unreadable file for diagnosis before starting with
                // defaults. The constructor will then create a fresh settings file.
                if path.exists() {
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    let backup = path.with_file_name(format!(
                        "{}.corrupt-{}",
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("settings.ini"),
                        stamp
                    ));
                    if let Err(rename_error) = fs::rename(path, &backup) {
                        eprintln!(
                            "Could not preserve malformed settings file at {:?}: {}",
                            path, rename_error
                        );
                    } else {
                        eprintln!("Preserved malformed settings file at {:?}", backup);
                    }
                }
                AppSettings::default()
            }
        }
    }

    /// Parse a string to boolean (supports various formats)
    pub(super) fn parse_bool(value: &str) -> bool {
        let lower = value.to_lowercase();
        matches!(lower.as_str(), "true" | "1" | "yes" | "on")
    }

    fn parse_f32_pair(value: &str) -> Option<(f32, f32)> {
        let (a, b) = value.split_once(',')?;

        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
}
