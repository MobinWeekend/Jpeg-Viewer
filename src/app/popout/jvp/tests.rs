    use super::*;

    #[test]
    fn preset_sanitization_keeps_values_in_supported_ranges() {
        let mut preset = PopoutPreset {
            opacity_percent: f32::NAN,
            ball_friction: 99.0,
            rotation_degrees: f32::NAN,
            dvd_mode: true,
            ball_mode: true,
            stay_on_top: true,
            stay_on_bottom: true,
            ..PopoutPreset::default()
        };
        preset.sanitize();
        assert_eq!(preset.opacity_percent, 100.0);
        assert_eq!(preset.ball_friction, 5.0);
        assert_eq!(preset.rotation_degrees, 0.0);
        assert!(preset.dvd_mode);
        assert!(!preset.ball_mode);
        assert!(preset.stay_on_top);
        assert!(!preset.stay_on_bottom);
    }

    #[test]
    fn rotation_angle_round_trips_and_legacy_presets_default_to_zero() {
        let preset: PopoutPreset = serde_json::from_str(r#"{"rotation_degrees":-72.5}"#)
            .expect("rotation angle should deserialize");
        assert_eq!(preset.rotation_degrees, -72.5);

        let legacy: PopoutPreset = serde_json::from_str(r#"{"ball_mode":true}"#)
            .expect("legacy preset should use defaults for the new field");
        assert_eq!(legacy.rotation_degrees, 0.0);
    }

    #[test]
    fn legacy_spin_setting_is_ignored_when_loading_older_jvp_files() {
        // Earlier JVP versions stored a fixed degrees-per-second spin value.
        // Rotation is now derived from throw velocity and collision impulses.
        let preset: PopoutPreset = serde_json::from_str(
            r#"{"ball_mode":true,"ball_rotation_speed":240.0}"#,
        )
        .expect("older JVP preset should remain loadable");
        assert!(preset.ball_mode);
    }

    #[test]
    fn bundle_member_validation_rejects_path_traversal() {
        assert!(super::bundle::validate_bundle_member_path("image/photo.png").is_ok());
        assert!(super::bundle::validate_bundle_member_path("image/../secret.png").is_err());
        assert!(super::bundle::validate_bundle_member_path("../secret.png").is_err());
    }
