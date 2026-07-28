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
    pub texture_filter: String, // "nearest", "linear", "mipmap"
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            b_ctrl_invert: false,
            window_pos: None,
            window_size: None,
            cache_radius: 6,
            cache_delta_factor: 0.6,
            navigation_pause_ms: 1200,
            max_cache_task: 2,
            texture_filter: "linear".to_string(),
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
                            if tasks > 100 {
                                settings.max_cache_task = tasks;
                            }
                        }
                    }
                    if let Some(value) = section.get("texture_filter") {
                        settings.texture_filter = value.to_string();
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

    /// Get a mutable reference to settings

    //pub fn get_mut(&mut self) -> &mut AppSettings {
    //    &mut self.settings
    //}

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
        assert_eq!(settings.cache_radius, 5);
        assert_eq!(settings.cache_delta_factor, 0.6);
        assert_eq!(settings.navigation_pause_ms, 1200);
        assert_eq!(settings.max_cache_task, 4)
    }
}
