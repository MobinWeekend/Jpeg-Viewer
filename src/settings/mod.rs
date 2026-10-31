use ini::Ini;
use std::fs;
use std::path::{Path, PathBuf};

const SETTINGS_SCHEMA_VERSION: u32 = 4;
const WINDOW_GEOMETRY_NORMALIZED_SCHEMA: u32 = 4;

mod load;
mod save;

#[cfg(test)]
mod tests;

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
    // Tricks: decorative effects confined to the main image viewport.
    pub tricks_snow_enabled: bool,
    pub tricks_snow_particle: String,
    /// Particle color in #RRGGBB format.
    pub tricks_snow_color: String,
    pub tricks_snow_speed: f32,
    pub tricks_snow_density: f32,
    pub tricks_snow_above_image: bool,
    pub tricks_watermark_enabled: bool,
    pub tricks_watermark_text: String,
    /// Watermark text color in #RRGGBB format.
    pub tricks_watermark_text_color: String,
    pub tricks_watermark_image_path: String,
    pub tricks_watermark_mode: String,
    pub tricks_watermark_x: f32,
    pub tricks_watermark_y: f32,
    pub tricks_watermark_speed: f32,
    pub tricks_watermark_opacity: f32,
    pub tricks_watermark_size: f32,
    pub tricks_backdrop_enabled: bool,
    pub tricks_backdrop_blur: f32,
    pub tricks_backdrop_tint: f32,
    /// Opacity of the blurred image backdrop (0.05-1.0).
    pub tricks_backdrop_opacity: f32,
    // virtual texture
    pub virtual_texture_threshold: u32,
    pub tile_size: u32,
    pub show_titlebar: bool,
    /// Title-bar color source: system, main_view, or main_color.
    pub title_bar_color_mode: String,
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
            tricks_snow_enabled: false,
            tricks_snow_particle: "❄".to_string(),
            tricks_snow_color: "#FFFFFF".to_string(),
            tricks_snow_speed: 1.0,
            tricks_snow_density: 1.0,
            tricks_snow_above_image: true,
            tricks_watermark_enabled: false,
            tricks_watermark_text: String::new(),
            tricks_watermark_text_color: "#FFFFFF".to_string(),
            tricks_watermark_image_path: String::new(),
            tricks_watermark_mode: "anchored".to_string(),
            tricks_watermark_x: 50.0,
            tricks_watermark_y: 50.0,
            tricks_watermark_speed: 1.0,
            tricks_watermark_opacity: 0.55,
            tricks_watermark_size: 48.0,
            tricks_backdrop_enabled: false,
            tricks_backdrop_blur: 8.0,
            tricks_backdrop_tint: 0.30,
            tricks_backdrop_opacity: 0.80,
            virtual_texture_threshold: 8192,
            tile_size: 1024,
            show_titlebar: true,
            title_bar_color_mode: "system".to_string(),
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
        // Capture the stored schema before load_from_file applies any in-memory
        // migrations, so we can persist the geometry normalization exactly once.
        let stored_schema_version = Ini::load_from_file(&path)
            .ok()
            .and_then(|config| {
                config
                    .section(Some("Settings"))
                    .and_then(|section| section.get("schema_version"))
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .unwrap_or(0);

        let mut error: Option<String> = None;
        let settings = Self::load_from_file(&path, &mut error);

        // Create a settings file on first run, and recover from a malformed file
        // without silently destroying the user's original configuration.
        if !path.exists() {
            if let Err(e) = Self::save_to_file(&path, &settings) {
                eprintln!("Warning: Could not save default settings: {}", e);
                error = Some(format!("Could not save default settings: {}", e));
            }
        } else if stored_schema_version < WINDOW_GEOMETRY_NORMALIZED_SCHEMA {
            // Persist the one-time conversion from old UI-point geometry to stable
            // startup geometry. save_to_file preserves unrelated/unknown INI keys.
            if let Err(e) = Self::save_to_file(&path, &settings) {
                eprintln!("Warning: Could not persist migrated settings: {}", e);
                error = Some(format!("Could not persist migrated settings: {}", e));
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
