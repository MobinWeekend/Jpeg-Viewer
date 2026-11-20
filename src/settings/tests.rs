use super::{AppSettings, SettingsManager};
use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    assert!((loaded.zoom_step_mouse_right_button - 1.2).abs() < f32::EPSILON);
    assert!((loaded.zoom_step_mouse_wheel - 1.2).abs() < f32::EPSILON);
    assert!((loaded.zoom_step_keyboard - 1.2).abs() < f32::EPSILON);
    assert_eq!(loaded.cache_radius, 12);
    assert_eq!(loaded.tile_size, AppSettings::default().tile_size);

    SettingsManager::save_to_file(&path, &loaded).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("schema_version=10"));
    let _ = fs::remove_file(path);
}

#[test]
fn new_tab_recent_rows_round_trip_and_migrate_legacy_options() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-new-tab-{}.ini", unique));
    let mut settings = AppSettings::default();
    settings.new_tab_recent_rows = 0;
    settings.window_move_mouse_shortcut_enabled = false;
    settings.window_resize_mouse_shortcut_enabled = false;
    settings.zoom_step_mouse_right_button = 1.5;
    settings.zoom_step_mouse_wheel = 1.5;
    settings.zoom_step_keyboard = 1.5;

    SettingsManager::save_to_file(&path, &settings).unwrap();
    let mut error: Option<String> = None;
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.new_tab_recent_rows, 0);
    assert!(!loaded.window_move_mouse_shortcut_enabled);
    assert!(!loaded.window_resize_mouse_shortcut_enabled);
    assert!((loaded.zoom_step_mouse_right_button - 1.5).abs() < f32::EPSILON);
    assert!((loaded.zoom_step_mouse_wheel - 1.5).abs() < f32::EPSILON);
    assert!((loaded.zoom_step_keyboard - 1.5).abs() < f32::EPSILON);

    fs::write(
        &path,
        "[Settings]\nnew_tab_show_recent=true\nnew_tab_recent_count=6\n",
    )
    .unwrap();
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.new_tab_recent_rows, 1);

    fs::write(
        &path,
        "[Settings]\nnew_tab_show_recent=true\nnew_tab_recent_count=50\n",
    )
    .unwrap();
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.new_tab_recent_rows, 2);

    fs::write(
        &path,
        "[Settings]\nnew_tab_show_recent=false\nnew_tab_recent_count=6\n",
    )
    .unwrap();
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.new_tab_recent_rows, 0);

    fs::write(&path, "[Settings]\nnew_tab_recent_rows=500\n").unwrap();
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.new_tab_recent_rows, 2);
    let _ = fs::remove_file(path);
}

#[test]
fn legacy_window_geometry_is_normalized_once_for_saved_ui_scale() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "jpeg-viewer-window-geometry-migration-{}.ini",
        unique
    ));
    fs::write(
        &path,
        "[Settings]\nschema_version=3\nui_scale=0.75\nwindow_pos=400,200\nwindow_size=1200,800\n",
    )
    .unwrap();

    let mut error: Option<String> = None;
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.window_pos, Some([300.0, 150.0]));
    assert_eq!(loaded.window_size, Some([900.0, 600.0]));

    // Once written with the current schema, the normalized geometry must not be scaled again.
    SettingsManager::save_to_file(&path, &loaded).unwrap();
    let loaded_again = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded_again.window_pos, Some([300.0, 150.0]));
    assert_eq!(loaded_again.window_size, Some([900.0, 600.0]));
    let _ = fs::remove_file(path);
}

#[test]
fn main_ui_placement_preference_round_trips() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-placement-{}.ini", unique));
    let mut settings = AppSettings::default();
    settings.main_ui_placement_preference = "cosmic".to_string();
    settings.desktop_menu_bar_preference = "off".to_string();
    settings.look_preference = "win98".to_string();
    settings.main_view_background_opacity = 0.45;
    settings.win98_dialup_enabled = false;
    settings.win98_dialup_preference_configured = true;
    settings.win98_dialup_connection_kbps = 56;
    settings.center_custom_window_title = true;
    settings.rename_file_from_titlebar = true;
    settings.native_window_rounding = false;
    settings.window_maximized = true;

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
    assert!(loaded.center_custom_window_title);
    assert!(loaded.rename_file_from_titlebar);
    assert!(!loaded.native_window_rounding);
    assert!(loaded.window_maximized);
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
        "[Settings]\nschema_version=9\nfuture_option=keep-me\n[OtherSection]\ncustom=value\n",
    )
    .unwrap();

    SettingsManager::save_to_file(&path, &AppSettings::default()).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("schema_version=10"));
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
    let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-invalid-{}.ini", unique));
    fs::write(
        &path,
        "[Settings]\ntexture_filter=unexpected\nmax_fps=NaN\ncache_delta_factor=inf\n",
    )
    .unwrap();

    let mut error: Option<String> = None;
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert_eq!(loaded.texture_filter, "mipmap");
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
        let path = std::env::temp_dir().join(format!(
            "jpeg-viewer-settings-auto-hide-{}-{}.ini",
            legacy, unique
        ));
        fs::write(
            &path,
            format!("[Settings]\nnavigation_ui_auto_hide={}\n", legacy),
        )
        .unwrap();
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
    let path = std::env::temp_dir().join(format!(
        "jpeg-viewer-settings-dialup-migration-{}.ini",
        unique
    ));
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
    assert!(settings.window_move_mouse_shortcut_enabled);
    assert!(settings.window_resize_mouse_shortcut_enabled);
    assert!(settings.window_pos.is_none());
    assert!(settings.window_size.is_none());
    assert!(!settings.window_maximized);
    assert_eq!(settings.cache_radius, 6);
    assert_eq!(settings.cache_delta_factor, 0.6);
    assert_eq!(settings.max_cache_task, 4);
    assert_eq!(settings.texture_filter, "mipmap");
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
    assert_eq!(settings.main_view_background_opacity, 0.35);
    assert!(settings.os_background_blur);
    assert!(settings.win98_dialup_enabled);
    assert!(!settings.win98_dialup_preference_configured);
    assert_eq!(settings.win98_dialup_connection_kbps, 28);
    assert_eq!(settings.slideshow_interval_ms, 3000);
    assert!(settings.slideshow_loop);
    assert!(!settings.slideshow_random);
    assert!(!settings.start_fullscreen);
}

#[test]
fn viewing_and_single_instance_settings_round_trip() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jpeg-viewer-viewing-settings-{}.ini", unique));
    let mut settings = AppSettings::default();
    settings.single_instance_enabled = true;
    settings.drag_drop_folders_recursive = true;
    settings.view_padding_mode = "fine_tune".to_string();
    settings
        .view_padding_fine_tune
        .insert("cosmic".to_string(), [2.0, 4.0, 6.0, 8.0]);
    settings.view_padding_custom = [10.0, 20.0, 30.0, 40.0];
    settings.fit_window_to_image = true;
    settings.fit_window_limit = "custom".to_string();
    settings.fit_window_min_width = 640.0;
    settings.fit_window_max_width = 1800.0;
    settings.fit_window_min_height = 512.0;
    settings.fit_window_max_height = 1200.0;
    settings.fit_window_preserve_aspect = false;
    settings.fit_window_center = false;

    SettingsManager::save_to_file(&path, &settings).unwrap();
    let mut error = None;
    let loaded = SettingsManager::load_from_file(&path, &mut error);
    assert!(error.is_none());
    assert!(loaded.single_instance_enabled);
    assert!(loaded.drag_drop_folders_recursive);
    assert_eq!(loaded.view_padding_mode, "fine_tune");
    assert_eq!(
        loaded.view_padding_fine_tune.get("cosmic"),
        Some(&[2.0, 4.0, 6.0, 8.0])
    );
    assert_eq!(loaded.view_padding_custom, [10.0, 20.0, 30.0, 40.0]);
    assert!(loaded.fit_window_to_image);
    assert_eq!(loaded.fit_window_limit, "custom");
    assert_eq!(loaded.fit_window_min_width, 640.0);
    assert_eq!(loaded.fit_window_max_width, 1800.0);
    assert_eq!(loaded.fit_window_min_height, 512.0);
    assert_eq!(loaded.fit_window_max_height, 1200.0);
    assert!(!loaded.fit_window_preserve_aspect);
    assert!(!loaded.fit_window_center);
    let _ = fs::remove_file(path);
}

#[test]
fn settings_updates_are_debounced_and_unchanged_updates_stay_clean() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jpeg-viewer-settings-debounce-{}.ini", unique));
    let mut manager = SettingsManager {
        path: path.clone(),
        settings: AppSettings::default(),
        last_error: None,
        dirty: false,
        dirty_since: None,
    };

    manager
        .update(|settings| settings.cache_radius = 9)
        .unwrap();
    assert!(manager.is_dirty());
    assert!(manager.time_until_save().is_some());
    assert!(manager.save_if_due().is_none());
    assert!(
        !path.exists(),
        "debounced updates must not write immediately"
    );

    // An update that does not change the serialized settings should not
    // restart the debounce timer or cause a needless rewrite.
    manager
        .update(|settings| settings.cache_radius = 9)
        .unwrap();
    assert!(manager.is_dirty());
    manager.dirty_since = Some(Instant::now() - Duration::from_secs(1));
    assert!(manager.save_if_due().is_some());
    assert!(!manager.is_dirty());
    assert!(path.exists());

    let _ = fs::remove_file(path);
}

#[test]
fn unchanged_settings_updates_do_not_mark_the_file_dirty() {
    let path = std::env::temp_dir().join(format!(
        "jpeg-viewer-settings-clean-{}.ini",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut manager = SettingsManager {
        path: path.clone(),
        settings: AppSettings::default(),
        last_error: None,
        dirty: false,
        dirty_since: None,
    };
    manager.update(|_| {}).unwrap();
    assert!(!manager.is_dirty());
    assert!(manager.time_until_save().is_none());
    assert!(!path.exists());
}
