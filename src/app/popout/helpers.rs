use super::*;

pub(super) fn opacity_to_alpha(opacity_percent: f32) -> u8 {
    (opacity_percent.clamp(0.0, 100.0) * 2.55).round() as u8
}

pub(super) fn apply_opacity_to_rgba(rgba: &[u8], opacity: u8) -> Vec<u8> {
    let mut adjusted = rgba.to_vec();
    for pixel in adjusted.chunks_exact_mut(4) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity) + 127) / 255) as u8;
    }
    adjusted
}

pub(super) fn clamp_vector_length(vector: egui::Vec2, max_length: f32) -> egui::Vec2 {
    let length = vector.length();
    if !vector.x.is_finite() || !vector.y.is_finite() || !length.is_finite() {
        egui::Vec2::ZERO
    } else if length > max_length && length > 0.0 {
        vector * (max_length / length)
    } else {
        vector
    }
}

pub(super) fn normalize_angle_degrees(degrees: f32) -> f32 {
    if degrees.is_finite() {
        (degrees + 180.0).rem_euclid(360.0) - 180.0
    } else {
        0.0
    }
}

pub(super) fn fit_aspect_size(aspect_ratio: f32, bounds: egui::Vec2) -> egui::Vec2 {
    let aspect_ratio = aspect_ratio.clamp(0.03, 32.0);
    let bounds = bounds.max(egui::vec2(1.0, 1.0));
    let fitted_width = if bounds.x / bounds.y > aspect_ratio {
        bounds.y * aspect_ratio
    } else {
        bounds.x
    };
    let maximum_width = MAX_SIZE.x.min(MAX_SIZE.y * aspect_ratio);
    let minimum_width = MIN_SIZE.x.min(maximum_width);
    let width = fitted_width.clamp(minimum_width, maximum_width);
    egui::vec2(width, width / aspect_ratio)
}

pub(super) fn ensure_jvp_extension(path: std::path::PathBuf) -> std::path::PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jvp"))
    {
        path
    } else {
        path.with_extension("jvp")
    }
}

pub(super) fn preset_from_window(window: &PopoutWindow) -> PopoutPreset {
    let rotation_degrees = normalize_angle_degrees(window.rotation_radians.to_degrees());
    PopoutPreset {
        size_scale: window.size_scale,
        opacity_percent: window.opacity_percent,
        click_through: window.click_through,
        pass_near_enabled: window.pass_near_enabled,
        pass_near_distance: window.pass_near_distance,
        pass_near_opacity: window.pass_near_opacity,
        pass_hover_enabled: window.pass_hover_enabled,
        pass_hover_opacity: window.pass_hover_opacity,
        show_bounds: window.show_bounds,
        remove_os_decorations: window.remove_os_decorations,
        remove_os_window_effects: window.remove_os_window_effects,
        dvd_mode: window.dvd_mode,
        dvd_circle_collision: window.dvd_circle_collision,
        random_reflect_on_bounce: window.random_reflect_on_bounce,
        random_rotate_on_bounce: window.random_rotate_on_bounce,
        dvd_speed: window.dvd_speed,
        dvd_rotation_ease: window.dvd_rotation_ease,
        dvd_edge_offsets: window.dvd_edge_offsets,
        monitor_id: window.monitor_id.clone(),
        ball_mode: window.ball_mode,
        ball_gravity: window.ball_gravity,
        ball_bounciness: window.ball_bounciness,
        ball_friction: window.ball_friction,
        ball_rotate: window.ball_rotate,
        random_bounce_enabled: window.random_bounce_enabled,
        random_bounce_interval: window.random_bounce_interval,
        rotation_degrees,
        stay_on_top: window.stay_on_top,
        stay_on_bottom: window.stay_on_bottom,
    }
}

pub(super) fn apply_preset_to_window(
    preset: &PopoutPreset,
    window: &mut PopoutWindow,
    ctx: Option<&egui::Context>,
) {
    let mut preset = preset.clone();
    preset.sanitize();
    window.size_scale = preset.size_scale;
    window.pending_size_scale = preset.size_scale;
    window.opacity_percent = preset.opacity_percent;
    window.displayed_opacity_percent = preset.opacity_percent;
    window.click_through = preset.click_through;
    window.pass_near_enabled = preset.pass_near_enabled;
    window.pass_near_distance = preset.pass_near_distance;
    window.pass_near_opacity = preset.pass_near_opacity;
    window.pass_hover_enabled = preset.pass_hover_enabled;
    window.pass_hover_opacity = preset.pass_hover_opacity;
    window.show_bounds = preset.show_bounds;
    window.remove_os_decorations = preset.remove_os_decorations;
    window.remove_os_window_effects = preset.remove_os_window_effects;
    window.dvd_mode = preset.dvd_mode;
    window.dvd_dragging = false;
    window.dvd_circle_collision = preset.dvd_circle_collision;
    window.random_reflect_on_bounce = preset.random_reflect_on_bounce;
    window.random_rotate_on_bounce = preset.random_rotate_on_bounce;
    window.dvd_speed = preset.dvd_speed;
    window.dvd_rotation_ease = preset.dvd_rotation_ease;
    window.dvd_edge_offsets = preset.dvd_edge_offsets;
    window.monitor_id = preset.monitor_id;
    window.ball_mode = preset.ball_mode;
    window.ball_gravity = preset.ball_gravity;
    window.ball_bounciness = preset.ball_bounciness;
    window.ball_friction = preset.ball_friction;
    window.ball_rotate = preset.ball_rotate;
    window.ball_time_accumulator = 0.0;
    window.random_bounce_enabled = preset.random_bounce_enabled;
    window.random_bounce_interval = preset.random_bounce_interval;
    window.rotation_radians = preset.rotation_degrees.to_radians();
    window.dvd_rotation_target = window.rotation_radians;
    window.next_random_bounce = Instant::now()
        + Duration::from_secs_f32(preset.random_bounce_interval);
    window.stay_on_top = preset.stay_on_top;
    window.stay_on_bottom = preset.stay_on_bottom;
    let requested_size = window.size_for_scale(preset.size_scale);
    window.size = requested_size;
    window.expected_native_size = Some((requested_size, Instant::now(), 0));
    window.ball_velocity = egui::vec2(360.0, -220.0);
    window.ball_angular_velocity = window.rolling_angular_velocity();
    window.last_dvd_tick = Instant::now();
    if let Some(ctx) = ctx {
        ctx.send_viewport_cmd_to(
            window.viewport_id,
            egui::ViewportCommand::InnerSize(requested_size),
        );
        ctx.send_viewport_cmd_to(
            window.viewport_id,
            egui::ViewportCommand::Decorations(!window.remove_os_decorations),
        );
        ctx.request_repaint();
    }
}
