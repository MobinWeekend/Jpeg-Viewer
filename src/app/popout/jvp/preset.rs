use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app::popout) enum SaveImageChoice {
    Bundle,
    Link,
    #[default]
    PresetOnly,
}

/// Options which belong to an Image tab rather than to the native viewport's
/// transient runtime state. Kept separately so a preset can be loaded before
/// its pop-out window is opened.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PopoutPreset {
    pub size_scale: f32,
    pub opacity_percent: f32,
    pub click_through: bool,
    pub pass_near_enabled: bool,
    pub pass_near_distance: f32,
    pub pass_near_opacity: f32,
    pub pass_hover_enabled: bool,
    pub pass_hover_opacity: f32,
    pub show_bounds: bool,
    pub remove_os_decorations: bool,
    pub remove_os_window_effects: bool,
    pub dvd_mode: bool,
    pub dvd_circle_collision: bool,
    pub random_reflect_on_bounce: bool,
    pub random_rotate_on_bounce: bool,
    pub dvd_speed: f32,
    pub dvd_rotation_ease: f32,
    pub dvd_edge_offsets: [f32; 4],
    pub monitor_id: Option<String>,
    pub ball_mode: bool,
    pub ball_gravity: f32,
    pub ball_bounciness: f32,
    pub ball_friction: f32,
    pub ball_rotate: bool,
    pub random_bounce_enabled: bool,
    pub random_bounce_interval: f32,
    /// Static rotation angle shared by DVD and Ball mode, in degrees.
    pub rotation_degrees: f32,
    pub stay_on_top: bool,
    pub stay_on_bottom: bool,
}

impl Default for PopoutPreset {
    fn default() -> Self {
        Self {
            size_scale: 1.0,
            opacity_percent: 100.0,
            click_through: false,
            pass_near_enabled: false,
            pass_near_distance: 120.0,
            pass_near_opacity: 75.0,
            pass_hover_enabled: false,
            pass_hover_opacity: 100.0,
            show_bounds: false,
            remove_os_decorations: true,
            remove_os_window_effects: true,
            dvd_mode: false,
            dvd_circle_collision: false,
            random_reflect_on_bounce: false,
            random_rotate_on_bounce: false,
            dvd_speed: 1.0,
            dvd_rotation_ease: 0.7,
            dvd_edge_offsets: [0.0; 4],
            monitor_id: None,
            ball_mode: false,
            ball_gravity: 1100.0,
            ball_bounciness: 0.82,
            ball_friction: 0.18,
            ball_rotate: true,
            random_bounce_enabled: false,
            random_bounce_interval: 6.0,
            rotation_degrees: 0.0,
            stay_on_top: false,
            stay_on_bottom: false,
        }
    }
}

impl PopoutPreset {
    pub(in crate::app::popout) fn sanitize(&mut self) {
        self.size_scale = finite_clamp(self.size_scale, 0.1, 8.0, 1.0);
        self.opacity_percent = finite_clamp(self.opacity_percent, 0.0, 100.0, 100.0);
        self.pass_near_distance = finite_clamp(self.pass_near_distance, 10.0, 500.0, 120.0);
        self.pass_near_opacity = finite_clamp(self.pass_near_opacity, 0.0, 100.0, 75.0);
        self.pass_hover_opacity = finite_clamp(self.pass_hover_opacity, 0.0, 100.0, 100.0);
        self.dvd_speed = finite_clamp(self.dvd_speed, 0.1, 4.0, 1.0);
        self.dvd_rotation_ease = finite_clamp(self.dvd_rotation_ease, 0.1, 4.0, 0.7);
        for offset in &mut self.dvd_edge_offsets {
            *offset = finite_clamp(*offset, 0.0, 1000.0, 0.0);
        }
        self.ball_gravity = finite_clamp(self.ball_gravity, 0.0, 2400.0, 1100.0);
        self.ball_bounciness = finite_clamp(self.ball_bounciness, 0.0, 1.0, 0.82);
        self.ball_friction = finite_clamp(self.ball_friction, 0.0, 5.0, 0.18);
        self.random_bounce_interval =
            finite_clamp(self.random_bounce_interval, 0.5, 30.0, 6.0);
        self.rotation_degrees = finite_clamp(self.rotation_degrees, -180.0, 180.0, 0.0);
        if self.dvd_mode {
            self.ball_mode = false;
        }
        if self.ball_mode {
            self.dvd_mode = false;
        }
        if self.stay_on_top {
            self.stay_on_bottom = false;
        }
        if self.stay_on_bottom {
            self.stay_on_top = false;
        }
    }
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}
