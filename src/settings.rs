use ini::Ini;
use std::path::PathBuf;

/// Application settings stored in INI format
#[derive(Debug, Clone)]
pub struct AppSettings {
    pub is_ctrl_invert: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            is_ctrl_invert: false,
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
            return Self { path, settings: default };
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
                    // Read is_ctrl_invert (boolean)
                    if let Some(value) = section.get("is_ctrl_invert") {
                        settings.is_ctrl_invert = Self::parse_bool(value);
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
        
        // Create the Settings section with is_ctrl_invert
        conf.with_section(Some("Settings"))
            .set("is_ctrl_invert", if settings.is_ctrl_invert { "true" } else { "false" });
        
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

    // ===== Public API =====
    
    /// Get a reference to the current settings
    pub fn get(&self) -> &AppSettings {
        &self.settings
    }

    /// Get a mutable reference to the current settings
    pub fn get_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
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

    /// Reload settings from disk (discards in-memory changes)
    pub fn reload(&mut self) {
        self.settings = Self::load_from_file(&self.path);
    }

    /// Get the path to the settings file
    pub fn path(&self) -> &PathBuf {
        &self.path
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
        assert!(!settings.is_ctrl_invert);
    }
}