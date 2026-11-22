use super::*;
use eframe::egui;

    #[test]
    fn opacity_scales_only_alpha_and_preserves_rgb() {
        let source = [120, 80, 40, 255, 9, 19, 29, 64];
        let half = apply_opacity_to_rgba(&source, 128);
        assert_eq!(&half[0..3], &[120, 80, 40]);
        assert_eq!(half[3], 128);
        assert_eq!(&half[4..7], &[9, 19, 29]);
        assert_eq!(half[7], 32);
    }

    #[test]
    fn zero_opacity_makes_pixels_transparent_without_changing_rgb() {
        let source = [200, 100, 50, 255];
        assert_eq!(apply_opacity_to_rgba(&source, 0), [200, 100, 50, 0]);
        assert_eq!(opacity_to_alpha(100.0), u8::MAX);
        assert_eq!(opacity_to_alpha(0.0), 0);
    }

    #[test]
    fn circular_collision_keeps_the_largest_dimension_radius_inside_bounds() {
        let bounds = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 700.0));
        let size = egui::vec2(560.0, 315.0);
        let (minimum, maximum) = PopoutWindow::collision_position_limits(bounds, size, true);
        let radius = size.x.max(size.y) * 0.5;
        let min_center = minimum + size * 0.5;
        let max_center = maximum + size * 0.5;
        assert!((min_center.x - radius).abs() < 0.01);
        assert!((min_center.y - radius).abs() < 0.01);
        assert!((bounds.right() - max_center.x - radius).abs() < 0.01);
        assert!((bounds.bottom() - max_center.y - radius).abs() < 0.01);

        // If extreme edge offsets leave less room than the ball's diameter,
        // keep it centered in the remaining area instead of sending it off-screen.
        let tiny_bounds =
            egui::Rect::from_min_max(egui::pos2(20.0, 30.0), egui::pos2(70.0, 60.0));
        let (minimum, maximum) =
            PopoutWindow::collision_position_limits(tiny_bounds, size, true);
        assert!((minimum.x - maximum.x).abs() < 0.01);
        assert!((minimum.y - maximum.y).abs() < 0.01);
        let centered = minimum + size * 0.5;
        assert!((centered.x - tiny_bounds.center().x).abs() < 0.01);
        assert!((centered.y - tiny_bounds.center().y).abs() < 0.01);
    }

    #[test]
    fn throw_velocity_is_finite_and_capped() {
        let velocity = clamp_vector_length(egui::vec2(3000.0, 4000.0), 2800.0);
        assert!((velocity.length() - 2800.0).abs() < 0.01);
        assert_eq!(
            clamp_vector_length(egui::vec2(f32::NAN, 1.0), 2800.0),
            egui::Vec2::ZERO
        );
        assert_eq!(clamp_vector_length(egui::Vec2::ZERO, 2800.0), egui::Vec2::ZERO);
    }

    #[test]
    fn angle_normalization_keeps_the_rotation_slider_within_its_range() {
        assert!((normalize_angle_degrees(270.0) + 90.0).abs() < 0.001);
        assert!((normalize_angle_degrees(-270.0) - 90.0).abs() < 0.001);
        assert_eq!(normalize_angle_degrees(f32::NAN), 0.0);
    }

    #[test]
    fn window_fit_keeps_the_source_aspect_ratio() {
        let size = fit_aspect_size(16.0 / 9.0, eframe::egui::vec2(560.0, 420.0));
        assert!((size.x / size.y - 16.0 / 9.0).abs() < 0.001);
        assert!(size.x <= 560.0 && size.y <= 420.0);
    }
