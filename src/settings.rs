use ini::Ini;
use std::path::PathBuf;

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
            preload_throttle_ms: 200,
            max_fps: 0.0,
            idle_fps_limit: 15.0,
            idle_timeout_ms: 2000,
            unfocused_idle_timeout_ms: 500,
            unfocused_idle_fps_limit: 1.0,
            slideshow_enabled: false,
            slideshow_interval_ms: 3000,
            slideshow_loop: true,
            slideshow_random: false,
            start_fullscreen: false,
        }
    }
}

/// Manages loading and saving settings to an INI file
pub struct SettingsManager {
    path: PathBuf,
    settings: AppSettings,
}

impl SettingsManager {
    /// Create a new SettingsManager that loads settings from the default location
    pub fn new() -> Self {
        let path = Self::get_settings_path();
        let settings = Self::load_from_file(&path);

        // If no settings file exists, create default and save it
        if !path.exists() {
            let default = AppSettings::default();
            if let Err(e) = Self::save_to_file(&path, &default) {
                eprintln!("Warning: Could not save default settings: {}", e);
            }
            return Self {
                path,
                settings: default,
            };
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
                    // Read b_ctrl_invert (boolean)
                    if let Some(value) = section.get("b_ctrl_invert") {
                        settings.b_ctrl_invert = Self::parse_bool(value);
                    }
                    // Read window position
                    if let Some(value) = section.get("window_pos") {
                        if let Some((x, y)) = Self::parse_f32_pair(value) {
                            settings.window_pos = Some([x, y]);
                        }
                    }
                    // Read window size
                    if let Some(value) = section.get("window_size") {
                        if let Some((width, height)) = Self::parse_f32_pair(value) {
                            if width > 100.0 && height > 100.0 {
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
                            if factor > 0.0 {
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
                        settings.texture_filter = value.to_string();
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
                            if fps >= 0.0 && fps <= 120.0 {
                                settings.max_fps = fps;
                            }
                        }
                    }
                    // Read idle_fps_limit
                    if let Some(value) = section.get("idle_fps_limit") {
                        if let Ok(fps) = value.parse::<f32>() {
                            if fps >= 0.0 && fps <= 120.0 {
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
                            if fps >= 0.0 && fps <= 120.0 {
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
                }

                settings
            }
            Err(e) => {
                eprintln!("Error loading settings from {:?}: {}", path, e);
                AppSettings::default()
            }
        }
    }

    /// Save settings to an INI file
    fn save_to_file(path: &PathBuf, settings: &AppSettings) -> Result<(), String> {
        let mut conf = Ini::new();

        // Create the Settings section with b_ctrl_invert
        let mut section = conf.with_section(Some("Settings"));

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
        section.set("slideshow_enabled", if settings.slideshow_enabled { "true" } else { "false" });
        section.set("slideshow_interval_ms", settings.slideshow_interval_ms.to_string());
        section.set("slideshow_loop", if settings.slideshow_loop { "true" } else { "false" });
        section.set("slideshow_random", if settings.slideshow_random { "true" } else { "false" });
        section.set("start_fullscreen", if settings.start_fullscreen { "true" } else { "false" });

        // Write the file
        conf.write_to_file(path)
            .map_err(|e| format!("Failed to save settings: {}", e))?;

        Ok(())
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
        self.save()
    }

    pub fn update_window_state(&mut self, pos: [f32; 2], size: [f32; 2]) -> Result<(), String> {
        // Clamp to reasonable values (prevent off-screen)
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
        assert_eq!(settings.preload_throttle_ms, 200);
        assert_eq!(settings.max_fps, 0.0);
        assert_eq!(settings.idle_fps_limit, 15.0);
        assert_eq!(settings.idle_timeout_ms, 2000);
        assert_eq!(settings.unfocused_idle_timeout_ms, 500);
        assert_eq!(settings.unfocused_idle_fps_limit, 1.0);
        assert!(!settings.slideshow_enabled);
        assert_eq!(settings.slideshow_interval_ms, 3000);
        assert!(settings.slideshow_loop);
        assert!(!settings.slideshow_random);
        assert!(!settings.start_fullscreen);
    }
}