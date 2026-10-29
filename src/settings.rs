use ini::Ini;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_SCHEMA_VERSION: u32 = 3;

/// Application settings stored in INI format
#[derive(Debug, Clone)]
pub struct AppSettings {
    pub b_ctrl_invert: bool,
    pub window_pos: Option<[f32; 2]>,
    pub window_size: Option<[f32; 2]>,
    pub cache_radius: usize,
    pub cache_delta_factor: f32,
    pub navigation_pause_ms: u64,
    pub max_cache_task: u8,
    /// Number of background workers used for sidebar thumbnail generation (1-8).
    pub thumbnail_workers: usize,
    pub texture_filter: String,   // "nearest", "linear", "mipmap"
    pub preload_throttle_ms: u64, // Delay between preload batches
    // Frame limiter settings
    pub max_fps: f32,                   // Maximum FPS (0 = unlimited)
    pub idle_fps_limit: f32,            // FPS limit when idle (0 = unlimited)
    pub idle_timeout_ms: u64,           // Time of inactivity before entering idle mode
    pub unfocused_idle_timeout_ms: u64, // Timeout when window is unfocused
    pub unfocused_idle_fps_limit: f32,  // FPS limit when window is unfocused and idle
    // Slideshow settings
    /// Whether slideshow playback should inhibit system sleep.
    pub prevent_sleep_during_slideshow: bool,
    pub slideshow_interval_ms: u64,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    pub slideshow_transition: String,
    pub slideshow_motion: String,
    pub slideshow_motion_strength: f32,
    /// Enables the Windows 98-inspired modem-style image reveal animation.
    pub win98_dialup_enabled: bool,
    /// Internal migration marker: true after the default or user preference has been migrated.
    pub win98_dialup_preference_configured: bool,
    /// Simulated modem connection speed in kbps (1-56).
    pub win98_dialup_connection_kbps: u32,
    // Startup settings
    pub start_fullscreen: bool,
    pub start_centered: bool,
    pub theme_preference: String,
    /// Visual design profile: auto, native, cosmic, or gnome.
    pub look_preference: String,
    /// Placement preset for the main toolbar and image-navigation controls: auto, native, cosmic, or gnome.
    pub main_ui_placement_preference: String,
    /// Desktop-style application menu bar preference: auto, on, or off.
    pub desktop_menu_bar_preference: String,
    /// Auto-hide mode for navigation overlays: auto, on, or off.
    /// Automatic only hides controls when the main UI uses the original JPEG Viewer placement.
    pub navigation_ui_auto_hide_preference: String,
    /// Inactivity delay before navigation/overlay controls are hidden (500–5000 ms).
    pub navigation_ui_auto_hide_ms: u64,
    /// Enables native window backdrop effects when supported, plus translucent UI fallbacks.
    pub os_background_blur: bool,
    /// Temporarily disable OS/DE blur while fullscreen to reduce visual overhead.
    pub disable_os_background_blur_in_fullscreen: bool,
    /// Global UI zoom factor (0.75–1.50).
    pub ui_scale: f32,
    /// Text-only scale multiplier (0.80–1.40).
    pub ui_text_scale: f32,
    /// Accent color preset: auto, teal, blue, purple, orange, rose, green, yellow, red.
    pub ui_accent_color: String,
    /// Main image-view background: dark, light, or custom:#RRGGBB.
    pub main_view_background: String,
    /// Main image-view background opacity when OS/DE glass is enabled (0.15-1.0).
    pub main_view_background_opacity: f32,
    /// Show a checkerboard behind images to reveal transparent pixels.
    pub show_alpha_checkerboard: bool,
    /// Checkerboard tile size in logical points (8-64).
    pub alpha_checkerboard_size: f32,
    /// Checkerboard contrast multiplier (0.25-1.5).
    pub alpha_checkerboard_brightness: f32,
    // virtual texture
    pub virtual_texture_threshold: u32,
    pub tile_size: u32,
    pub show_titlebar: bool,
    /// Thickness in logical points of the custom window border and resize hit areas (2-12).
    pub custom_window_frame_thickness: f32,
    // Sidebar and compact-view preferences
    pub show_file_sidebar: bool,
    pub file_sidebar_width: f32,
    pub settings_sidebar_width: f32,
    pub compact_mode: bool,
    /// Thumbnail entry size for the Browse, Queue, and Favorites lists.
    pub browse_entry_size: f32,
    pub queue_entry_size: f32,
    pub favorites_entry_size: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            b_ctrl_invert: false,
            window_pos: None,
            window_size: None,
            cache_radius: 5,
            cache_delta_factor: 0.6,
            navigation_pause_ms: 1200,
            max_cache_task: 4,
            thumbnail_workers: 3,
            texture_filter: "linear".to_string(),
            preload_throttle_ms: 100,
            max_fps: 30.0,
            idle_fps_limit: 0.0,
            idle_timeout_ms: 1000,
            unfocused_idle_timeout_ms: 500,
            unfocused_idle_fps_limit: 0.0,
            prevent_sleep_during_slideshow: true,
            slideshow_interval_ms: 3000,
            slideshow_loop: true,
            slideshow_random: false,
            slideshow_transition: "fade".to_string(),
            slideshow_motion: "smart".to_string(),
            slideshow_motion_strength: 20.0,
            win98_dialup_enabled: true,
            win98_dialup_preference_configured: false,
            win98_dialup_connection_kbps: 28,
            start_fullscreen: false,
            start_centered: false,
            theme_preference: "system".to_string(),
            look_preference: "auto".to_string(),
            main_ui_placement_preference: "auto".to_string(),
            desktop_menu_bar_preference: "auto".to_string(),
            navigation_ui_auto_hide_preference: "auto".to_string(),
            navigation_ui_auto_hide_ms: 1400,
            os_background_blur: true,
            disable_os_background_blur_in_fullscreen: true,
            ui_scale: 1.0,
            ui_text_scale: 1.0,
            ui_accent_color: "auto".to_string(),
            main_view_background: "dark".to_string(),
            main_view_background_opacity: 0.75,
            show_alpha_checkerboard: true,
            alpha_checkerboard_size: 18.0,
            alpha_checkerboard_brightness: 1.0,
            virtual_texture_threshold: 8192,
            tile_size: 1024,
            show_titlebar: true,
            custom_window_frame_thickness: 6.0,
            show_file_sidebar: true,
            file_sidebar_width: 250.0,
            settings_sidebar_width: 360.0,
            compact_mode: false,
            browse_entry_size: 176.0,
            queue_entry_size: 176.0,
            favorites_entry_size: 176.0,
        }
    }
}

/// Manages loading and saving settings to an INI file
pub struct SettingsManager {
    path: PathBuf,
    pub settings: AppSettings,
    /// Last read/write error, if any. Cleared on a successful save.
    last_error: Option<String>,
}

impl SettingsManager {
    /// Create a new SettingsManager that loads settings from the default location
    pub fn new() -> Self {
        let path = Self::get_settings_path();
        let mut error: Option<String> = None;
        let settings = Self::load_from_file(&path, &mut error);

        // Create a settings file on first run, and recover from a malformed file
        // without silently destroying the user's original configuration.
        if !path.exists() {
            if let Err(e) = Self::save_to_file(&path, &settings) {
                eprintln!("Warning: Could not save default settings: {}", e);
                error = Some(format!("Could not save default settings: {}", e));
            }
        }

        Self {
            path,
            settings,
            last_error: error,
        }
    }

    /// Get the per-user settings path, using the platform's standard config directory.
    /// Existing portable settings are migrated on first launch when possible.
    fn get_settings_path() -> PathBuf {
        let filename = "settings.ini";
        let config_root = dirs::config_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let app_dir = config_root.join("Jpeg Viewer");
        let target = app_dir.join(filename);

        if !target.exists() {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(parent) = exe_path.parent() {
                    for legacy_name in ["jpeg_viewer settings.ini", "settings.ini"] {
                        let legacy = parent.join(legacy_name);
                        if legacy.is_file() {
                            if fs::create_dir_all(&app_dir).is_ok()
                                && fs::copy(&legacy, &target).is_ok()
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }
        target
    }

    /// Load settings from an INI file. Any read error is recorded in `error_out`.
    fn load_from_file(path: &PathBuf, error_out: &mut Option<String>) -> AppSettings {
        if !path.exists() {
            return AppSettings::default();
        }

        match Ini::load_from_file(path) {
            Ok(conf) => {
                let mut settings = AppSettings::default();

                // Get the Settings section
                if let Some(section) = conf.section(Some("Settings")) {
                    // Older files without a version are treated as schema 0. Loading
                    // starts from defaults, so newly introduced keys migrate safely.
                    // Unknown future versions are still read conservatively by known key.
                    if let Some(version) = section.get("schema_version") {
                        match version.parse::<u32>() {
                            Ok(v) if v > SETTINGS_SCHEMA_VERSION => eprintln!(
                                "Settings file schema version {} is newer than supported version {}; known settings will be loaded.",
                                v, SETTINGS_SCHEMA_VERSION
                            ),
                            Ok(_) => {}
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
                            if (1..=8).contains(&workers) { settings.thumbnail_workers = workers; }
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
                        if matches!(value, "none" | "fade" | "slide" | "random") { settings.slideshow_transition = value.to_string(); }
                    }
                    if let Some(value) = section.get("slideshow_motion") {
                        if matches!(value, "none" | "smart" | "pan-horizontal" | "pan-vertical" | "zoom-in" | "zoom-out" | "random") { settings.slideshow_motion = value.to_string(); }
                    }
                    if let Some(value) = section.get("slideshow_motion_strength") {
                        if let Ok(strength) = value.parse::<f32>() { settings.slideshow_motion_strength = strength.clamp(5.0, 40.0); }
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
                        if let Ok(v) = value.parse::<f32>() { if v.is_finite() && (0.75..=1.50).contains(&v) { settings.ui_scale = v; } }
                    }
                    if let Some(value) = section.get("ui_text_scale") {
                        if let Ok(v) = value.parse::<f32>() { if v.is_finite() && (0.80..=1.40).contains(&v) { settings.ui_text_scale = v; } }
                    }
                    if let Some(value) = section.get("ui_accent_color") {
                        if ["auto", "teal", "blue", "purple", "orange", "rose", "green", "yellow", "red", "win98blue"].contains(&value)
                            || (value.starts_with("custom:#") && value.len() == 14 && value[8..].chars().all(|c| c.is_ascii_hexdigit())) {
                            settings.ui_accent_color = value.to_string();
                        }
                    }
                    if let Some(value) = section.get("main_view_background") {
                        if value == "dark" || value == "light" || value == "theme" || (value.starts_with("custom:#") && value.len() == 14 && value[8..].chars().all(|c| c.is_ascii_hexdigit())) {
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
                        if let Ok(v) = value.parse::<f32>() { if v.is_finite() { settings.alpha_checkerboard_size = v.clamp(8.0, 64.0); } }
                    }
                    if let Some(value) = section.get("alpha_checkerboard_brightness") {
                        if let Ok(v) = value.parse::<f32>() { if v.is_finite() { settings.alpha_checkerboard_brightness = v.clamp(0.25, 1.5); } }
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
                            if (500..=5000).contains(&ms) { settings.navigation_ui_auto_hide_ms = ms; }
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

    /// Save settings to an INI file
    fn save_to_file(path: &PathBuf, settings: &AppSettings) -> Result<(), String> {
        // Start from the existing INI when it is readable so unknown keys and
        // unrelated sections survive an update by an older application version.
        let mut conf = Ini::load_from_file(path).unwrap_or_else(|_| Ini::new());
        let stored_version = conf
            .section(Some("Settings"))
            .and_then(|section| section.get("schema_version"))
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);

        // Versioned, additive schema: absent keys in old files retain defaults.
        // Do not downgrade a file written by a newer version of the application.
        let mut section = conf.with_section(Some("Settings"));
        section.set(
            "schema_version",
            stored_version.max(SETTINGS_SCHEMA_VERSION).to_string(),
        );

        section.set(
            "b_ctrl_invert",
            if settings.b_ctrl_invert {
                "true"
            } else {
                "false"
            },
        );

        if let Some([x, y]) = settings.window_pos {
            section.set("window_pos", format!("{},{}", x, y));
        }

        if let Some([w, h]) = settings.window_size {
            section.set("window_size", format!("{},{}", w, h));
        }

        section.set("cache_radius", settings.cache_radius.to_string());
        section.set(
            "cache_delta_factor",
            settings.cache_delta_factor.to_string(),
        );
        section.set(
            "navigation_pause_ms",
            settings.navigation_pause_ms.to_string(),
        );
        section.set("max_cache_task", settings.max_cache_task.to_string());
        section.set("thumbnail_workers", settings.thumbnail_workers.to_string());
        section.set("texture_filter", &settings.texture_filter);
        section.set(
            "preload_throttle_ms",
            settings.preload_throttle_ms.to_string(),
        );

        // Frame limiter settings
        section.set("max_fps", settings.max_fps.to_string());
        section.set("idle_fps_limit", settings.idle_fps_limit.to_string());
        section.set("idle_timeout_ms", settings.idle_timeout_ms.to_string());
        section.set(
            "unfocused_idle_timeout_ms",
            settings.unfocused_idle_timeout_ms.to_string(),
        );
        section.set(
            "unfocused_idle_fps_limit",
            settings.unfocused_idle_fps_limit.to_string(),
        );

        // Slideshow settings
        section.set(
            "prevent_sleep_during_slideshow",
            if settings.prevent_sleep_during_slideshow { "true" } else { "false" },
        );
        section.set(
            "slideshow_interval_ms",
            settings.slideshow_interval_ms.to_string(),
        );
        section.set(
            "slideshow_loop",
            if settings.slideshow_loop {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "slideshow_random",
            if settings.slideshow_random {
                "true"
            } else {
                "false"
            },
        );
        section.set("slideshow_transition", settings.slideshow_transition.clone());
        section.set("slideshow_motion", settings.slideshow_motion.clone());
        section.set("slideshow_motion_strength", settings.slideshow_motion_strength.to_string());
        section.set("win98_dialup_enabled", if settings.win98_dialup_enabled { "true" } else { "false" });
        section.set("win98_dialup_preference_configured", if settings.win98_dialup_preference_configured { "true" } else { "false" });
        section.set("win98_dialup_connection_kbps", settings.win98_dialup_connection_kbps.to_string());
        section.set("start_centered", if settings.start_centered { "true" } else { "false" });
        section.set(
            "start_fullscreen",
            if settings.start_fullscreen {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "virtual_texture_threshold",
            settings.virtual_texture_threshold.to_string(),
        );
        section.set("tile_size", settings.tile_size.to_string());
        section.set(
            "show_titlebar",
            if settings.show_titlebar {
                "true"
            } else {
                "false"
            },
        );
        section.set("custom_window_frame_thickness", settings.custom_window_frame_thickness.to_string());
        section.set("theme_preference", &settings.theme_preference);
        section.set("look_preference", &settings.look_preference);
        section.set(
            "main_ui_placement_preference",
            &settings.main_ui_placement_preference,
        );
        section.set("desktop_menu_bar_preference", &settings.desktop_menu_bar_preference);
        section.set("navigation_ui_auto_hide_preference", &settings.navigation_ui_auto_hide_preference);
        section.set("navigation_ui_auto_hide_ms", settings.navigation_ui_auto_hide_ms.to_string());
        section.set("os_background_blur", if settings.os_background_blur { "true" } else { "false" });
        section.set(
            "disable_os_background_blur_in_fullscreen",
            if settings.disable_os_background_blur_in_fullscreen { "true" } else { "false" },
        );
        section.set("ui_scale", settings.ui_scale.to_string());
        section.set("ui_text_scale", settings.ui_text_scale.to_string());
        section.set("ui_accent_color", &settings.ui_accent_color);
        section.set("main_view_background", &settings.main_view_background);
        section.set("main_view_background_opacity", settings.main_view_background_opacity.to_string());
        section.set("show_alpha_checkerboard", if settings.show_alpha_checkerboard { "true" } else { "false" });
        section.set("alpha_checkerboard_size", settings.alpha_checkerboard_size.to_string());
        section.set("alpha_checkerboard_brightness", settings.alpha_checkerboard_brightness.to_string());
        section.set(
            "show_file_sidebar",
            if settings.show_file_sidebar {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "file_sidebar_width",
            settings.file_sidebar_width.to_string(),
        );
        section.set(
            "settings_sidebar_width",
            settings.settings_sidebar_width.to_string(),
        );
        section.set(
            "compact_mode",
            if settings.compact_mode {
                "true"
            } else {
                "false"
            },
        );
        section.set("browse_entry_size", settings.browse_entry_size.to_string());
        section.set("queue_entry_size", settings.queue_entry_size.to_string());
        section.set("favorites_entry_size", settings.favorites_entry_size.to_string());

        Self::write_ini_safely(path, &conf)
    }

    /// Stage settings in a sibling temporary file and flush it before replacing the
    /// old file. A backup protects the previous copy during replacement. This is a
    /// recoverable replacement strategy, not a guarantee of atomicity on every OS/filesystem.
    fn write_ini_safely(path: &Path, conf: &Ini) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create settings directory {:?}: {}", parent, e))?;

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("settings.ini");
        let temp_path = parent.join(format!(".{}.{}.tmp", file_name, stamp));
        let backup_path = parent.join(format!(".{}.{}.bak", file_name, stamp));

        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .map_err(|e| {
                    format!(
                        "Could not create temporary settings file {:?}: {}",
                        temp_path, e
                    )
                })?;
            conf.write_to(&mut file)
                .map_err(|e| format!("Could not write temporary settings file: {}", e))?;
            file.sync_all()
                .map_err(|e| format!("Could not flush temporary settings file: {}", e))?;
            drop(file);

            let had_original = path.exists();
            if had_original {
                fs::rename(path, &backup_path).map_err(|e| {
                    format!(
                        "Could not preserve existing settings file {:?}: {}",
                        path, e
                    )
                })?;
            }

            if let Err(e) = fs::rename(&temp_path, path) {
                if had_original {
                    if let Err(restore_error) = fs::rename(&backup_path, path) {
                        return Err(format!(
                            "Could not install new settings file: {}; restoring the previous file also failed: {} (backup at {:?})",
                            e, restore_error, backup_path
                        ));
                    }
                }
                return Err(format!("Could not install new settings file: {}", e));
            }

            if had_original {
                if let Err(e) = fs::remove_file(&backup_path) {
                    eprintln!(
                        "Warning: settings saved, but could not remove backup {:?}: {}",
                        backup_path, e
                    );
                }
            }
            Ok(())
        })();

        if temp_path.exists() {
            if let Err(e) = fs::remove_file(&temp_path) {
                eprintln!(
                    "Warning: could not remove temporary settings file {:?}: {}",
                    temp_path, e
                );
            }
        }
        result
    }

    /// Parse a string to boolean (supports various formats)
    fn parse_bool(value: &str) -> bool {
        let lower = value.to_lowercase();
        matches!(lower.as_str(), "true" | "1" | "yes" | "on")
    }

    fn parse_f32_pair(value: &str) -> Option<(f32, f32)> {
        let (a, b) = value.split_once(',')?;

        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }

    // ===== Public API =====

    /// Get a reference to the current settings
    pub fn get(&self) -> &AppSettings {
        &self.settings
    }

    /// Path to the persisted settings `.ini` file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Last read/write error, or `None` if the last operation succeeded.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Save current settings to disk. Records/clears `last_error` accordingly.
    pub fn save(&mut self) -> Result<(), String> {
        let result = Self::save_to_file(&self.path, &self.settings);
        match &result {
            Ok(()) => self.last_error = None,
            Err(e) => {
                self.last_error = Some(format!("Could not write {}: {}", self.path.display(), e));
            }
        }
        result
    }

    /// Update a setting and save automatically
    pub fn update<F>(&mut self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut AppSettings),
    {
        f(&mut self.settings);
        let result = self.save();
        if let Err(error) = &result {
            eprintln!("Failed to persist updated settings: {}", error);
        }
        result
    }

    pub fn update_window_state(&mut self, pos: [f32; 2], size: [f32; 2]) -> Result<(), String> {
        // Clamp to reasonable values (prevent off-screen)
        if !pos[0].is_finite()
            || !pos[1].is_finite()
            || !size[0].is_finite()
            || !size[1].is_finite()
        {
            return Err("Window position and size must contain finite numbers".to_string());
        }
        let size = [size[0].max(100.0), size[1].max(100.0)];

        self.settings.window_pos = Some(pos);
        self.settings.window_size = Some(size);
        self.save()
    }
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert!(SettingsManager::parse_bool("true"));
        assert!(SettingsManager::parse_bool("TRUE"));
        assert!(SettingsManager::parse_bool("1"));
        assert!(SettingsManager::parse_bool("yes"));
        assert!(SettingsManager::parse_bool("on"));
        assert!(!SettingsManager::parse_bool("false"));
        assert!(!SettingsManager::parse_bool("0"));
        assert!(!SettingsManager::parse_bool("no"));
        assert!(!SettingsManager::parse_bool("off"));
        assert!(!SettingsManager::parse_bool("random"));
    }

    #[test]
    fn legacy_settings_load_with_defaults_and_are_versioned_on_save() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-test-{}.ini", unique));
        fs::write(&path, "[Settings]\nb_ctrl_invert=true\ncache_radius=12\n").unwrap();

        let mut error: Option<String> = None;
        let loaded = SettingsManager::load_from_file(&path, &mut error);
        assert!(error.is_none());
        assert!(loaded.b_ctrl_invert);
        assert_eq!(loaded.cache_radius, 3);
        assert_eq!(loaded.tile_size, AppSettings::default().tile_size);

        SettingsManager::save_to_file(&path, &loaded).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("schema_version=3"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn main_ui_placement_preference_round_trips() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("jpeg-viewer-settings-placement-{}.ini", unique));
        let mut settings = AppSettings::default();
        settings.main_ui_placement_preference = "cosmic".to_string();
        settings.desktop_menu_bar_preference = "off".to_string();
        settings.look_preference = "win98".to_string();
        settings.main_view_background_opacity = 0.45;
        settings.win98_dialup_enabled = false;
        settings.win98_dialup_preference_configured = true;
        settings.win98_dialup_connection_kbps = 56;

        SettingsManager::save_to_file(&path, &settings).unwrap();
        let mut error: Option<String> = None;
        let loaded = SettingsManager::load_from_file(&path, &mut error);
        assert!(error.is_none());
        assert_eq!(loaded.main_ui_placement_preference, "cosmic");
        assert_eq!(loaded.desktop_menu_bar_preference, "off");
        assert_eq!(loaded.look_preference, "win98");
        assert_eq!(loaded.main_view_background_opacity, 0.45);
        assert!(!loaded.win98_dialup_enabled);
        assert!(loaded.win98_dialup_preference_configured);
        assert_eq!(loaded.win98_dialup_connection_kbps, 56);
        settings.navigation_ui_auto_hide_preference = "off".to_string();
        settings.navigation_ui_auto_hide_ms = 2300;
        settings.os_background_blur = false;
        settings.disable_os_background_blur_in_fullscreen = false;
        SettingsManager::save_to_file(&path, &settings).unwrap();
        let loaded = SettingsManager::load_from_file(&path, &mut error);
        assert!(error.is_none());
        assert_eq!(loaded.navigation_ui_auto_hide_preference, "off");
        assert_eq!(loaded.navigation_ui_auto_hide_ms, 2300);
        assert!(!loaded.os_background_blur);
        assert!(!loaded.disable_os_background_blur_in_fullscreen);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_preserves_unknown_keys_and_future_schema_version() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-future-{}.ini", unique));
        fs::write(
            &path,
            "[Settings]\nschema_version=7\nfuture_option=keep-me\n[OtherSection]\ncustom=value\n",
        )
        .unwrap();

        SettingsManager::save_to_file(&path, &AppSettings::default()).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("schema_version=7"));
        assert!(saved.contains("future_option=keep-me"));
        assert!(saved.contains("custom=value"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_values_fall_back_to_safe_defaults() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("jpeg-viewer-settings-invalid-{}.ini", unique));
        fs::write(
            &path,
            "[Settings]\ntexture_filter=unexpected\nmax_fps=NaN\ncache_delta_factor=inf\n",
        )
        .unwrap();

        let mut error: Option<String> = None;
        let loaded = SettingsManager::load_from_file(&path, &mut error);
        assert!(error.is_none());
        assert_eq!(loaded.texture_filter, "linear");
        assert_eq!(loaded.max_fps, AppSettings::default().max_fps);
        assert_eq!(
            loaded.cache_delta_factor,
            AppSettings::default().cache_delta_factor
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_navigation_auto_hide_setting_is_migrated() {
        for (legacy, expected) in [("true", "auto"), ("false", "off")] {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("jpeg-viewer-settings-auto-hide-{}-{}.ini", legacy, unique));
            fs::write(&path, format!("[Settings]\nnavigation_ui_auto_hide={}\n", legacy)).unwrap();
            let mut error: Option<String> = None;
            let loaded = SettingsManager::load_from_file(&path, &mut error);
            assert!(error.is_none());
            assert_eq!(loaded.navigation_ui_auto_hide_preference, expected);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn legacy_dialup_default_migrates_to_enabled() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("jpeg-viewer-settings-dialup-migration-{}.ini", unique));
        fs::write(
            &path,
            "[Settings]\nschema_version=2\nwin98_dialup_enabled=false\nwin98_dialup_connection_kbps=28\n",
        )
        .unwrap();

        let mut error = None;
        let loaded = SettingsManager::load_from_file(&path, &mut error);
        assert!(error.is_none());
        assert!(loaded.win98_dialup_enabled);
        assert!(loaded.win98_dialup_preference_configured);
        SettingsManager::save_to_file(&path, &loaded).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("win98_dialup_preference_configured=true"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert!(!settings.b_ctrl_invert);
        assert!(settings.window_pos.is_none());
        assert!(settings.window_size.is_none());
        assert_eq!(settings.cache_radius, 5);
        assert_eq!(settings.cache_delta_factor, 0.6);
        assert_eq!(settings.navigation_pause_ms, 1200);
        assert_eq!(settings.max_cache_task, 4);
        assert_eq!(settings.texture_filter, "linear");
        assert_eq!(settings.preload_throttle_ms, 100);
        assert_eq!(settings.max_fps, 30.0);
        assert_eq!(settings.idle_fps_limit, 0.0);
        assert_eq!(settings.idle_timeout_ms, 1000);
        assert_eq!(settings.unfocused_idle_timeout_ms, 500);
        assert_eq!(settings.unfocused_idle_fps_limit, 0.0);
        assert!(settings.prevent_sleep_during_slideshow);
        assert_eq!(settings.look_preference, "auto");
        assert_eq!(settings.main_ui_placement_preference, "auto");
        assert_eq!(settings.desktop_menu_bar_preference, "auto");
        assert_eq!(settings.navigation_ui_auto_hide_preference, "auto");
        assert_eq!(settings.navigation_ui_auto_hide_ms, 1400);
        assert_eq!(settings.main_view_background_opacity, 0.75);
        assert!(settings.os_background_blur);
        assert!(settings.win98_dialup_enabled);
        assert!(!settings.win98_dialup_preference_configured);
        assert_eq!(settings.win98_dialup_connection_kbps, 28);
        assert_eq!(settings.slideshow_interval_ms, 3000);
        assert!(settings.slideshow_loop);
        assert!(!settings.slideshow_random);
        assert!(!settings.start_fullscreen);
    }
}
