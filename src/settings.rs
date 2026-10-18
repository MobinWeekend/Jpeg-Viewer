use ini::Ini;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_SCHEMA_VERSION: u32 = 1;

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
    pub texture_filter: String,   // "nearest", "linear", "mipmap"
    pub preload_throttle_ms: u64, // Delay between preload batches
    // Frame limiter settings
    pub max_fps: f32,                   // Maximum FPS (0 = unlimited)
    pub idle_fps_limit: f32,            // FPS limit when idle (0 = unlimited)
    pub idle_timeout_ms: u64,           // Time of inactivity before entering idle mode
    pub unfocused_idle_timeout_ms: u64, // Timeout when window is unfocused
    pub unfocused_idle_fps_limit: f32,  // FPS limit when window is unfocused and idle
    // Slideshow settings
    pub slideshow_enabled: bool,
    pub slideshow_interval_ms: u64,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    // Startup settings
    pub start_fullscreen: bool,
    pub theme_preference: String,
    // virtual texture
    pub virtual_texture_threshold: u32,
    pub tile_size: u32,
    pub show_titlebar: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            b_ctrl_invert: false,
            window_pos: None,
            window_size: None,
            cache_radius: 7,
            cache_delta_factor: 0.6,
            navigation_pause_ms: 1200,
            max_cache_task: 4,
            texture_filter: "linear".to_string(),
            preload_throttle_ms: 100,
            max_fps: 30.0,
            idle_fps_limit: 0.0,
            idle_timeout_ms: 1000,
            unfocused_idle_timeout_ms: 500,
            unfocused_idle_fps_limit: 0.0,
            slideshow_enabled: false,
            slideshow_interval_ms: 3000,
            slideshow_loop: true,
            slideshow_random: false,
            start_fullscreen: false,
            theme_preference: "system".to_string(),
            virtual_texture_threshold: 8192,
            tile_size: 1024,
            show_titlebar: true,
        }
    }
}

/// Manages loading and saving settings to an INI file
pub struct SettingsManager {
    path: PathBuf,
    pub settings: AppSettings,
}

impl SettingsManager {
    /// Create a new SettingsManager that loads settings from the default location
    pub fn new() -> Self {
        let path = Self::get_settings_path();
        let settings = Self::load_from_file(&path);

        // Create a settings file on first run, and recover from a malformed file
        // without silently destroying the user's original configuration.
        if !path.exists() {
            if let Err(e) = Self::save_to_file(&path, &settings) {
                eprintln!("Warning: Could not save default settings: {}", e);
            }
        }

        Self { path, settings }
    }

    /// Get the path where settings should be stored
    fn get_settings_path() -> PathBuf {
        // Save next to the executable (portable app style)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                return parent.join("jpeg_viewer settings.ini");
            }
        }

        // Fallback: current working directory
        PathBuf::from("jpeg_viewer settings.ini")
    }

    /// Load settings from an INI file
    fn load_from_file(path: &PathBuf) -> AppSettings {
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
                            Ok(_) => {},
                            Err(_) => eprintln!("Invalid settings schema_version; treating file as legacy settings."),
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
                            if width.is_finite() && height.is_finite() && width > 100.0 && height > 100.0 {
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
                            if factor.is_finite() && (0.0..=10.0).contains(&factor) && factor > 0.0 {
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
                    // Read slideshow settings
                    if let Some(value) = section.get("slideshow_enabled") {
                        settings.slideshow_enabled = Self::parse_bool(value);
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
                    // Read startup settings
                    if let Some(value) = section.get("start_fullscreen") {
                        settings.start_fullscreen = Self::parse_bool(value);
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
                    // theme
                    if let Some(value) = section.get("theme_preference") {
                        if value == "system" || value == "light" || value == "dark" {
                            settings.theme_preference = value.to_string();
                        }
                    }
                }

                settings
            }
            Err(e) => {
                eprintln!("Error loading settings from {:?}: {}", path, e);
                // Preserve the unreadable file for diagnosis before starting with
                // defaults. The constructor will then create a fresh settings file.
                if path.exists() {
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    let backup = path.with_file_name(format!(
                        "{}.corrupt-{}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("settings.ini"),
                        stamp
                    ));
                    if let Err(rename_error) = fs::rename(path, &backup) {
                        eprintln!("Could not preserve malformed settings file at {:?}: {}", path, rename_error);
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
        section.set("schema_version", stored_version.max(SETTINGS_SCHEMA_VERSION).to_string());

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
            "slideshow_enabled",
            if settings.slideshow_enabled {
                "true"
            } else {
                "false"
            },
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
        section.set("theme_preference", &settings.theme_preference);

        Self::write_ini_safely(path, &conf)
    }

    /// Stage settings in a sibling temporary file and flush it before replacing the
    /// old file. A backup protects the previous copy during replacement. This is a
    /// recoverable replacement strategy, not a guarantee of atomicity on every OS/filesystem.
    fn write_ini_safely(path: &Path, conf: &Ini) -> Result<(), String> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create settings directory {:?}: {}", parent, e))?;

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("settings.ini");
        let temp_path = parent.join(format!(".{}.{}.tmp", file_name, stamp));
        let backup_path = parent.join(format!(".{}.{}.bak", file_name, stamp));

        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .map_err(|e| format!("Could not create temporary settings file {:?}: {}", temp_path, e))?;
            conf.write_to(&mut file)
                .map_err(|e| format!("Could not write temporary settings file: {}", e))?;
            file.sync_all()
                .map_err(|e| format!("Could not flush temporary settings file: {}", e))?;
            drop(file);

            let had_original = path.exists();
            if had_original {
                fs::rename(path, &backup_path)
                    .map_err(|e| format!("Could not preserve existing settings file {:?}: {}", path, e))?;
            }

            if let Err(e) = fs::rename(&temp_path, path) {
                if had_original {
                    if let Err(restore_error) = fs::rename(&backup_path, path) {
                        return Err(format!("Could not install new settings file: {}; restoring the previous file also failed: {} (backup at {:?})", e, restore_error, backup_path));
                    }
                }
                return Err(format!("Could not install new settings file: {}", e));
            }

            if had_original {
                if let Err(e) = fs::remove_file(&backup_path) {
                    eprintln!("Warning: settings saved, but could not remove backup {:?}: {}", backup_path, e);
                }
            }
            Ok(())
        })();

        if temp_path.exists() {
            if let Err(e) = fs::remove_file(&temp_path) {
                eprintln!("Warning: could not remove temporary settings file {:?}: {}", temp_path, e);
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

    /// Save current settings to disk
    pub fn save(&self) -> Result<(), String> {
        Self::save_to_file(&self.path, &self.settings)
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
        if !pos[0].is_finite() || !pos[1].is_finite() || !size[0].is_finite() || !size[1].is_finite() {
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
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-test-{}.ini", unique));
        fs::write(&path, "[Settings]\nb_ctrl_invert=true\ncache_radius=12\n").unwrap();

        let loaded = SettingsManager::load_from_file(&path);
        assert!(loaded.b_ctrl_invert);
        assert_eq!(loaded.cache_radius, 12);
        assert_eq!(loaded.tile_size, AppSettings::default().tile_size);

        SettingsManager::save_to_file(&path, &loaded).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("schema_version=1"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_preserves_unknown_keys_and_future_schema_version() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
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
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-invalid-{}.ini", unique));
        fs::write(&path, "[Settings]\ntexture_filter=unexpected\nmax_fps=NaN\ncache_delta_factor=inf\n").unwrap();

        let loaded = SettingsManager::load_from_file(&path);
        assert_eq!(loaded.texture_filter, "linear");
        assert_eq!(loaded.max_fps, AppSettings::default().max_fps);
        assert_eq!(loaded.cache_delta_factor, AppSettings::default().cache_delta_factor);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert!(!settings.b_ctrl_invert);
        assert!(settings.window_pos.is_none());
        assert!(settings.window_size.is_none());
        assert_eq!(settings.cache_radius, 7);
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
        assert!(!settings.slideshow_enabled);
        assert_eq!(settings.slideshow_interval_ms, 3000);
        assert!(settings.slideshow_loop);
        assert!(!settings.slideshow_random);
        assert!(!settings.start_fullscreen);
    }
}
