use super::{AppSettings, SettingsManager, SETTINGS_SCHEMA_VERSION};
use ini::Ini;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

impl SettingsManager {
    /// Save settings to an INI file
    pub(super) fn save_to_file(path: &PathBuf, settings: &AppSettings) -> Result<(), String> {
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
            if settings.prevent_sleep_during_slideshow {
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
            "slideshow_transition",
            settings.slideshow_transition.clone(),
        );
        section.set("slideshow_motion", settings.slideshow_motion.clone());
        section.set(
            "slideshow_motion_strength",
            settings.slideshow_motion_strength.to_string(),
        );
        section.set(
            "win98_dialup_enabled",
            if settings.win98_dialup_enabled {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "win98_dialup_preference_configured",
            if settings.win98_dialup_preference_configured {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "win98_dialup_connection_kbps",
            settings.win98_dialup_connection_kbps.to_string(),
        );
        section.set(
            "start_centered",
            if settings.start_centered {
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
        section.set("title_bar_color_mode", &settings.title_bar_color_mode);
        section.set(
            "custom_window_frame_thickness",
            settings.custom_window_frame_thickness.to_string(),
        );
        section.set("theme_preference", &settings.theme_preference);
        section.set("look_preference", &settings.look_preference);
        section.set(
            "main_ui_placement_preference",
            &settings.main_ui_placement_preference,
        );
        section.set(
            "desktop_menu_bar_preference",
            &settings.desktop_menu_bar_preference,
        );
        section.set(
            "navigation_ui_auto_hide_preference",
            &settings.navigation_ui_auto_hide_preference,
        );
        section.set(
            "navigation_ui_auto_hide_ms",
            settings.navigation_ui_auto_hide_ms.to_string(),
        );
        section.set(
            "os_background_blur",
            if settings.os_background_blur {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "disable_os_background_blur_in_fullscreen",
            if settings.disable_os_background_blur_in_fullscreen {
                "true"
            } else {
                "false"
            },
        );
        section.set("ui_scale", settings.ui_scale.to_string());
        section.set("ui_text_scale", settings.ui_text_scale.to_string());
        section.set("ui_accent_color", &settings.ui_accent_color);
        section.set("main_view_background", &settings.main_view_background);
        section.set(
            "main_view_background_opacity",
            settings.main_view_background_opacity.to_string(),
        );
        section.set(
            "show_alpha_checkerboard",
            if settings.show_alpha_checkerboard {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "alpha_checkerboard_size",
            settings.alpha_checkerboard_size.to_string(),
        );
        section.set(
            "alpha_checkerboard_brightness",
            settings.alpha_checkerboard_brightness.to_string(),
        );
        section.set(
            "tricks_snow_enabled",
            if settings.tricks_snow_enabled {
                "true"
            } else {
                "false"
            },
        );
        section.set("tricks_snow_particle", &settings.tricks_snow_particle);
        section.set("tricks_snow_color", &settings.tricks_snow_color);
        section.set("tricks_snow_speed", settings.tricks_snow_speed.to_string());
        section.set(
            "tricks_snow_density",
            settings.tricks_snow_density.to_string(),
        );
        section.set(
            "tricks_snow_above_image",
            if settings.tricks_snow_above_image {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "tricks_watermark_enabled",
            if settings.tricks_watermark_enabled {
                "true"
            } else {
                "false"
            },
        );
        section.set("tricks_watermark_text", &settings.tricks_watermark_text);
        section.set(
            "tricks_watermark_text_color",
            &settings.tricks_watermark_text_color,
        );
        section.set(
            "tricks_watermark_image_path",
            &settings.tricks_watermark_image_path,
        );
        section.set("tricks_watermark_mode", &settings.tricks_watermark_mode);
        section.set(
            "tricks_watermark_x",
            settings.tricks_watermark_x.to_string(),
        );
        section.set(
            "tricks_watermark_y",
            settings.tricks_watermark_y.to_string(),
        );
        section.set(
            "tricks_watermark_speed",
            settings.tricks_watermark_speed.to_string(),
        );
        section.set(
            "tricks_watermark_opacity",
            settings.tricks_watermark_opacity.to_string(),
        );
        section.set(
            "tricks_watermark_size",
            settings.tricks_watermark_size.to_string(),
        );
        section.set(
            "tricks_backdrop_enabled",
            if settings.tricks_backdrop_enabled {
                "true"
            } else {
                "false"
            },
        );
        section.set(
            "tricks_backdrop_blur",
            settings.tricks_backdrop_blur.to_string(),
        );
        section.set(
            "tricks_backdrop_tint",
            settings.tricks_backdrop_tint.to_string(),
        );
        section.set(
            "tricks_backdrop_opacity",
            settings.tricks_backdrop_opacity.to_string(),
        );
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
        section.set(
            "favorites_entry_size",
            settings.favorites_entry_size.to_string(),
        );

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
}
