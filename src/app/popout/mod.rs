//! Floating image windows associated with Image workspace tabs.
//!
//! Pop-outs are independent native egui viewports. Their state and rendering
//! are kept here instead of mixing auxiliary-window behavior into workspace tabs.

mod jvp;

use crate::decoder::gif::animation::GifAnimation;
use crate::imaging::entry::ImageEntry;
use jvp::{LoadedJvp, PopoutPreset, SaveImageChoice};
use eframe::egui;
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_SIZE: egui::Vec2 = egui::Vec2::new(560.0, 420.0);
const MIN_SIZE: egui::Vec2 = egui::Vec2::new(120.0, 90.0);
const MAX_SIZE: egui::Vec2 = egui::Vec2::new(2400.0, 1800.0);
const DVD_SPEED: egui::Vec2 = egui::Vec2::new(190.0, 145.0);
const RESIZE_EDGE: f32 = 9.0;
const RESIZE_CORNER: f32 = 34.0;

#[derive(Default)]
pub(crate) struct PopoutManager {
    // Deferred viewports render independently of the root window. This is
    // essential on egui 0.33: immediate child viewports can be discarded when
    // the parent becomes hidden/minimized because the parent stops submitting
    // them. Each child owns a synchronized state object for its independent UI.
    windows: HashMap<u64, Arc<Mutex<PopoutWindow>>>,
    pending_close: Vec<egui::ViewportId>,
    /// A deferred viewport ID is single-use after its native window is closed.
    /// Each reopening gets a fresh generation so egui/winit can create a new
    /// native window for the same Image tab.
    next_viewport_generation: u64,
    /// Per-Image-tab options survive closing the native pop-out.
    presets: HashMap<u64, PopoutPreset>,
    save_dialog: Option<SaveDialogState>,
    load_dialog: Option<LoadDialogState>,
    pending_image_open: Option<(ImageEntry, PopoutPreset)>,
    /// Image tab whose right-hand Pop-out settings sidebar is currently open.
    settings_sidebar_tab: Option<u64>,
    status_message: Option<String>,
}

struct SaveDialogState {
    tab_id: u64,
    preset: PopoutPreset,
    image_entry: Option<ImageEntry>,
    choice: SaveImageChoice,
}

struct LoadDialogState {
    tab_id: u64,
    document: LoadedJvp,
}

struct PopoutWindow {
    viewport_id: egui::ViewportId,
    viewport_generation: u64,
    texture: egui::TextureHandle,
    animation: Option<GifAnimation>,
    last_frame_index: Option<usize>,
    size: egui::Vec2,
    base_size: egui::Vec2,
    aspect_ratio: f32,
    size_scale: f32,
    /// Mirrored live size slider value.
    pending_size_scale: f32,
    /// Last requested native size. Viewport size notifications can lag the
    /// command by a few frames; do not let an old notification undo a resize.
    expected_native_size: Option<(egui::Vec2, Instant, u8)>,
    /// Last requested native position. Native position notifications can lag
    /// while a resize keeps the opposite edge anchored; do not overwrite the
    /// in-progress geometry with stale viewport data.
    expected_native_position: Option<(egui::Pos2, Instant)>,
    /// Initial size/position are supplied to ViewportBuilder exactly once.
    /// Later geometry updates travel through the single ViewportCommand path.
    viewport_initialized: bool,
    /// Original decoded RGBA pixels for static images, allowing alpha to be
    /// adjusted without multiplying RGB and accidentally darkening the image.
    source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
    display_texture: egui::TextureHandle,
    rendered_opacity: u8,
    opacity_percent: f32,
    /// Eased opacity actually used by the image texture and painter.
    displayed_opacity_percent: f32,
    click_through: bool,
    pass_near_enabled: bool,
    pass_near_distance: f32,
    pass_near_opacity: f32,
    pass_hover_enabled: bool,
    pass_hover_opacity: f32,
    /// Screen-space pointer position forwarded by the main viewport while
    /// click-through is enabled (the pop-out itself intentionally receives no input).
    pointer_screen_pos: Option<egui::Pos2>,
    show_bounds: bool,
    dvd_mode: bool,
    dvd_circle_collision: bool,
    random_reflect_on_bounce: bool,
    random_rotate_on_bounce: bool,
    rotation_radians: f32,
    dvd_rotation_target: f32,
    dvd_rotation_ease: f32,
    dvd_speed: f32,
    ball_mode: bool,
    ball_gravity: f32,
    ball_bounciness: f32,
    /// Contact friction coefficient. It is applied only when the ball hits a
    /// boundary, coupling tangential linear velocity with angular velocity.
    ball_friction: f32,
    /// Fixed-step simulation remainder (seconds) for stable Ball mode physics.
    ball_time_accumulator: f32,
    ball_velocity: egui::Vec2,
    ball_rotate: bool,
    /// Live angular velocity in radians per second.
    ball_angular_velocity: f32,
    ball_dragging: bool,
    /// Last local pointer location during a Ball drag. Using per-frame pointer
    /// deltas avoids treating egui's cumulative drag_delta as a fresh movement
    /// every frame (which caused the pop-out to accelerate and jitter).
    ball_last_drag_pointer: Option<egui::Pos2>,
    ball_throw_velocity: egui::Vec2,
    random_bounce_enabled: bool,
    random_bounce_interval: f32,
    next_random_bounce: Instant,
    /// Inward offsets from the monitor edges in left, top, right, bottom order.
    dvd_edge_offsets: [f32; 4],
    monitor_id: Option<String>,
    stay_on_top: bool,
    stay_on_bottom: bool,
    /// Window level is sent only when it changes. Reapplying it every frame can
    /// continuously raise the child above the main application on some WMs.
    applied_window_level: Option<egui::WindowLevel>,
    position: egui::Pos2,
    dvd_velocity: egui::Vec2,
    last_dvd_tick: Instant,
    last_bottom_enforcement: Instant,
    resize_gesture: Option<ResizeGesture>,
    /// Set by the deferred child callback when the OS close button is used.
    /// The manager observes it on the next root pass and removes the tab link.
    close_requested: bool,
    /// On Windows, detach the native top-level HWND from the main window's
    /// owner relationship. Owned windows are automatically hidden when their
    /// owner is minimized, which contradicts the pop-out lifecycle contract.
    native_owner_detached: bool,
    /// DWM per-pixel alpha compositing has been enabled for this native window.
    #[cfg(target_os = "windows")]
    dwm_transparency_applied: bool,
}

#[derive(Clone, Copy)]
struct ResizeGesture {
    start_size: egui::Vec2,
    start_position: egui::Pos2,
    horizontal: f32,
    vertical: f32,
    /// Last cumulative pointer delta. Usually supplied by egui directly; the
    /// accumulated fallback keeps resizing stable if the pointer leaves a
    /// viewport and the integration briefly cannot provide a total delta.
    accumulated_delta: egui::Vec2,
}

impl PopoutWindow {
    fn automatic_window_movement_supported() -> bool {
        #[cfg(target_os = "linux")]
        {
            !crate::platform::wayland_pointer_recovery::is_running_on_wayland()
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    fn minimum_size(&self) -> egui::Vec2 {
        // Keep a feasible minimum even for extremely panoramic/portrait sources:
        // aspect ratio is more important than imposing independent min width/height.
        let max_width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        let width = MIN_SIZE.x.min(max_width);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn maximum_size(&self) -> egui::Vec2 {
        let width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn size_for_width(&self, width: f32) -> egui::Vec2 {
        let min = self.minimum_size();
        let max = self.maximum_size();
        let width = width.clamp(min.x, max.x);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn size_for_scale(&self, scale: f32) -> egui::Vec2 {
        self.size_for_width(self.base_size.x * scale)
    }

    fn viewport_builder(&mut self, tab_id: u64, parent_focused: bool) -> egui::ViewportBuilder {
        let level = if self.stay_on_top && !parent_focused {
            egui::WindowLevel::AlwaysOnTop
        } else if self.stay_on_bottom {
            egui::WindowLevel::AlwaysOnBottom
        } else {
            egui::WindowLevel::Normal
        };
        let mut builder = egui::ViewportBuilder::default()
            .with_title(format!(
                "Pop-out · JPEG Viewer · Tab {tab_id} · Instance {}",
                self.viewport_generation
            ))
            .with_min_inner_size(self.minimum_size())
            .with_max_inner_size(self.maximum_size())
            .with_clamp_size_to_monitor_size(true)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_decorations(false)
            // Resizing is deliberately owned by the eight egui edge/corner
            // handles below. Enabling native resizing as well lets the OS take
            // pointer capture at the same edges, competing with aspect-ratio
            // geometry commands and causing flicker or missed drags.
            .with_resizable(false)
            .with_mouse_passthrough(self.click_through);
        if !self.viewport_initialized {
            // ViewportBuilder is accumulated every frame. Re-supplying inner
            // size and position while gestures also issue native geometry
            // commands makes the backend repeatedly reconcile two owners of
            // the same geometry, producing resize flicker and stale-size resets.
            builder = builder
                .with_inner_size(self.size)
                .with_position(self.position)
                .with_window_level(level)
                .with_active(true);
            self.applied_window_level = Some(level);
            self.viewport_initialized = true;
        }
        builder
    }

    fn update_animation_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
        let Some(animation) = self.animation.as_mut() else {
            self.update_static_opacity_texture(ctx, tab_id);
            return;
        };
        let _ = animation.get_current_frame();
        let frame_index = animation.get_current_frame_index();
        let opacity = opacity_to_alpha(self.displayed_opacity_percent);
        if self.last_frame_index != Some(frame_index) || self.rendered_opacity != opacity {
            let Some(frame) = animation.get_current_frame_ref() else {
                return;
            };
            let size = [frame.width() as usize, frame.height() as usize];
            let rgba = frame.as_raw();
            let source = egui::ColorImage::from_rgba_unmultiplied(size, rgba);
            self.texture = ctx.load_texture(
                format!("jpeg_viewer_popout_source_{tab_id}_{frame_index}"),
                source,
                Default::default(),
            );
            if opacity == u8::MAX {
                self.display_texture = self.texture.clone();
            } else {
                let adjusted = apply_opacity_to_rgba(rgba, opacity);
                self.display_texture = ctx.load_texture(
                    format!("jpeg_viewer_popout_display_{tab_id}_{frame_index}_{opacity}"),
                    egui::ColorImage::from_rgba_unmultiplied(size, &adjusted),
                    Default::default(),
                );
            }
            self.last_frame_index = Some(frame_index);
            self.rendered_opacity = opacity;
        }
        if animation.is_animated() {
            let delay = animation
                .delays
                .get(animation.current_frame)
                .copied()
                .unwrap_or(Duration::from_millis(100));
            let adjusted = if animation.speed_multiplier > 0.0 {
                Duration::from_secs_f64(
                    delay.as_secs_f64() / animation.speed_multiplier.clamp(0.1, 10.0) as f64,
                )
            } else {
                delay
            };
            ctx.request_repaint_after(adjusted.max(Duration::from_millis(8)));
        }
    }

    fn update_static_opacity_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
        let opacity = opacity_to_alpha(self.displayed_opacity_percent);
        if self.rendered_opacity == opacity {
            return;
        }
        if opacity == u8::MAX {
            self.display_texture = self.texture.clone();
            self.rendered_opacity = opacity;
            return;
        }
        if let Some((width, height, pixels)) = &self.source_pixels {
            let adjusted = apply_opacity_to_rgba(pixels, opacity);
            self.display_texture = ctx.load_texture(
                format!("jpeg_viewer_popout_display_{tab_id}_{opacity}"),
                egui::ColorImage::from_rgba_unmultiplied([*width, *height], &adjusted),
                Default::default(),
            );
            self.rendered_opacity = opacity;
        }
    }

    fn current_motion_bounds(&self) -> Option<egui::Rect> {
        let monitors = crate::platform::monitors::available_monitors();
        let monitor = self
            .monitor_id
            .as_deref()
            .and_then(|id| monitors.iter().find(|monitor| monitor.id == id))
            .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
            .or_else(|| monitors.first())?;
        let monitor_bounds = monitor.bounds();
        let max_horizontal_inset = (monitor_bounds.width() - 1.0).max(0.0) * 0.5;
        let max_vertical_inset = (monitor_bounds.height() - 1.0).max(0.0) * 0.5;
        let left = monitor_bounds.left()
            + self.dvd_edge_offsets[0].clamp(0.0, max_horizontal_inset);
        let top = monitor_bounds.top()
            + self.dvd_edge_offsets[1].clamp(0.0, max_vertical_inset);
        let right = monitor_bounds.right()
            - self.dvd_edge_offsets[2].clamp(0.0, max_horizontal_inset);
        let bottom = monitor_bounds.bottom()
            - self.dvd_edge_offsets[3].clamp(0.0, max_vertical_inset);
        Some(egui::Rect::from_min_max(
            egui::pos2(left, top),
            egui::pos2(right.max(left), bottom.max(top)),
        ))
    }

    fn collision_position_limits(
        bounds: egui::Rect,
        size: egui::Vec2,
        circular: bool,
    ) -> (egui::Pos2, egui::Pos2) {
        if circular {
            // The native window is kept inside an imaginary circle whose radius
            // is half its largest dimension. This stays consistent for portrait
            // and landscape images.
            let radius = size.x.max(size.y) * 0.5;
            let center_min = bounds.min + egui::vec2(radius, radius);
            let center_max = bounds.max - egui::vec2(radius, radius);
            let bounds_center = bounds.center();
            let min_center = egui::pos2(
                if center_min.x <= center_max.x {
                    center_min.x
                } else {
                    bounds_center.x
                },
                if center_min.y <= center_max.y {
                    center_min.y
                } else {
                    bounds_center.y
                },
            );
            let max_center = egui::pos2(
                if center_min.x <= center_max.x {
                    center_max.x
                } else {
                    bounds_center.x
                },
                if center_min.y <= center_max.y {
                    center_max.y
                } else {
                    bounds_center.y
                },
            );
            (min_center - size * 0.5, max_center - size * 0.5)
        } else {
            let max = bounds.max - size;
            (
                bounds.min,
                egui::pos2(max.x.max(bounds.left()), max.y.max(bounds.top())),
            )
        }
    }

    fn random_direction() -> egui::Vec2 {
        let angle = rand::thread_rng().gen_range(0.0..std::f32::consts::TAU);
        egui::vec2(angle.cos(), angle.sin())
    }

    fn randomize_rotation(&mut self) {
        let rotation = rand::thread_rng().gen_range(0.0..std::f32::consts::TAU);
        if self.dvd_mode {
            self.dvd_rotation_target = rotation;
        } else {
            self.rotation_radians = rotation;
        }
    }

    /// Treat Ball mode as a solid 2D disk with unit mass. Its moment of inertia
    /// is therefore I = 1/2 m r². The contact normal points back into the
    /// playable area; the tangential impulse is limited by μ * normal_impulse
    /// (Coulomb friction), as in standard rigid-body solvers.
    fn resolve_ball_contact(&mut self, normal: egui::Vec2) {
        let incoming_normal_speed = self.ball_velocity.dot(normal);
        if incoming_normal_speed >= 0.0 {
            return;
        }

        let radius = (self.size.x.max(self.size.y) * 0.5).max(1.0);
        let inertia = 0.5 * radius * radius;
        // Suppress tiny rebounds to avoid an endless sequence of sub-pixel
        // hops when the disk is resting against the bottom boundary. The
        // resulting inelastic contact impulse still supports Coulomb friction,
        // so a sliding ball can lose tangential speed while grounded.
        const RESTITUTION_THRESHOLD: f32 = 40.0;
        let restitution = if -incoming_normal_speed < RESTITUTION_THRESHOLD {
            0.0
        } else {
            self.ball_bounciness.clamp(0.0, 1.0)
        };
        let normal_impulse = -(1.0 + restitution) * incoming_normal_speed;
        self.ball_velocity += normal * normal_impulse;

        // In screen coordinates, a positive angle is clockwise. For a contact
        // point at r = -normal * radius, its tangential speed is v·t - ωr.
        let tangent = egui::vec2(-normal.y, normal.x);
        let tangential_speed = self.ball_velocity.dot(tangent)
            - self.ball_angular_velocity * radius;
        let inverse_effective_mass = 1.0 + radius * radius / inertia;
        let max_friction_impulse = self.ball_friction.max(0.0) * normal_impulse;
        let tangent_impulse = (-tangential_speed / inverse_effective_mass)
            .clamp(-max_friction_impulse, max_friction_impulse);

        // Unit linear mass; angular impulse is r × J.
        self.ball_velocity += tangent * tangent_impulse;
        self.ball_angular_velocity -= radius * tangent_impulse / inertia;
    }

    fn rolling_angular_velocity(&self) -> f32 {
        let radius = (self.size.x.max(self.size.y) * 0.5).max(1.0);
        self.ball_velocity.x / radius
    }

    fn update_dvd_rotation(&mut self, delta: f32) {
        let angle_delta = (self.dvd_rotation_target - self.rotation_radians + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let time_constant = self.dvd_rotation_ease.clamp(0.1, 4.0);
        let amount = 1.0 - (-6.0 * delta / time_constant).exp();
        self.rotation_radians = (self.rotation_radians + angle_delta * amount)
            .rem_euclid(std::f32::consts::TAU);
        if angle_delta.abs() < 0.001 {
            self.rotation_radians = self.dvd_rotation_target;
        }
    }

    fn update_pass_opacity(&mut self, ctx: &egui::Context) {
        let mut eased_target = None;
        if self.click_through {
            if let Some(pointer) = self.pointer_screen_pos {
                let rect = egui::Rect::from_min_size(self.position, self.size);
                if self.pass_hover_enabled && rect.contains(pointer) {
                    eased_target = Some(self.pass_hover_opacity);
                } else if self.pass_near_enabled {
                    let dx = (rect.left() - pointer.x).max(0.0).max(pointer.x - rect.right());
                    let dy = (rect.top() - pointer.y).max(0.0).max(pointer.y - rect.bottom());
                    if egui::vec2(dx, dy).length() <= self.pass_near_distance {
                        eased_target = Some(self.pass_near_opacity);
                    }
                }
            }
        }
        // The ordinary opacity control is a direct setting: apply it on this
        // frame. Only pointer-triggered Pass effects use easing.
        let Some(target) = eased_target else {
            self.displayed_opacity_percent = self.opacity_percent.clamp(0.0, 100.0);
            return;
        };
        let target = target.clamp(0.0, 100.0);
        let delta = ctx.input(|input| input.stable_dt).clamp(0.0, 0.1);
        // Exponential easing gives a smooth transition independent of frame rate.
        let amount = 1.0 - (-10.0 * delta).exp();
        self.displayed_opacity_percent +=
            (target - self.displayed_opacity_percent) * amount;
        if (target - self.displayed_opacity_percent).abs() > 0.15 {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            self.displayed_opacity_percent = target;
        }
    }

    fn update_dvd_position(&mut self, ctx: &egui::Context) {
        if !Self::automatic_window_movement_supported() {
            self.last_dvd_tick = Instant::now();
            return;
        }
        if self.ball_mode {
            self.update_ball_position(ctx);
            return;
        }
        if !self.dvd_mode {
            self.last_dvd_tick = Instant::now();
            return;
        }
        let Some(bounds) = self.current_motion_bounds() else {
            return;
        };

        let now = Instant::now();
        let delta = now
            .saturating_duration_since(self.last_dvd_tick)
            .as_secs_f32()
            .clamp(0.0, 0.05);
        self.last_dvd_tick = now;
        let circular = self.dvd_circle_collision;
        let monitor_size = bounds.size();
        let max_width = if circular {
            monitor_size.x.min(monitor_size.y) * self.aspect_ratio.min(1.0)
        } else {
            monitor_size.x.min(monitor_size.y * self.aspect_ratio)
        };
        let fitted_size = self.size_for_width(self.size.x.min(max_width));
        if (fitted_size - self.size).length_sq() > 0.1 {
            self.request_inner_size(ctx, fitted_size);
        }
        let (min_position, max_position) =
            Self::collision_position_limits(bounds, self.size, circular);
        let mut position = self.position;
        position.x += DVD_SPEED.x * self.dvd_speed * delta * self.dvd_velocity.x;
        position.y += DVD_SPEED.y * self.dvd_speed * delta * self.dvd_velocity.y;

        let hit_left = position.x < min_position.x;
        let hit_right = position.x > max_position.x;
        let hit_top = position.y < min_position.y;
        let hit_bottom = position.y > max_position.y;
        if hit_left {
            position.x = min_position.x;
            self.dvd_velocity.x = self.dvd_velocity.x.abs();
        } else if hit_right {
            position.x = max_position.x;
            self.dvd_velocity.x = -self.dvd_velocity.x.abs();
        }
        if hit_top {
            position.y = min_position.y;
            self.dvd_velocity.y = self.dvd_velocity.y.abs();
        } else if hit_bottom {
            position.y = max_position.y;
            self.dvd_velocity.y = -self.dvd_velocity.y.abs();
        }

        if hit_left || hit_right || hit_top || hit_bottom {
            if self.random_reflect_on_bounce {
                let mut direction = Self::random_direction();
                if hit_left {
                    direction.x = direction.x.abs();
                } else if hit_right {
                    direction.x = -direction.x.abs();
                }
                if hit_top {
                    direction.y = direction.y.abs();
                } else if hit_bottom {
                    direction.y = -direction.y.abs();
                }
                self.dvd_velocity = direction;
            }
            if self.random_rotate_on_bounce {
                self.randomize_rotation();
            }
        }

        if self.random_bounce_enabled && now >= self.next_random_bounce {
            self.dvd_velocity = Self::random_direction();
            self.next_random_bounce = now
                + Duration::from_secs_f32(self.random_bounce_interval.clamp(0.5, 60.0));
        }

        self.update_dvd_rotation(delta);
        self.request_outer_position(ctx, position);
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn update_ball_position(&mut self, ctx: &egui::Context) {
        let Some(bounds) = self.current_motion_bounds() else {
            return;
        };
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_dvd_tick)
            .as_secs_f32()
            .clamp(0.0, 0.1);
        self.last_dvd_tick = now;
        if self.ball_dragging || self.resize_gesture.is_some() {
            self.ball_time_accumulator = 0.0;
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }

        let monitor_size = bounds.size();
        let max_width = monitor_size.x.min(monitor_size.y) * self.aspect_ratio.min(1.0);
        let fitted_size = self.size_for_width(self.size.x.min(max_width));
        if (fitted_size - self.size).length_sq() > 0.1 {
            self.request_inner_size(ctx, fitted_size);
        }
        let (min_position, max_position) =
            Self::collision_position_limits(bounds, self.size, true);

        if self.random_bounce_enabled && now >= self.next_random_bounce {
            self.ball_velocity += Self::random_direction() * 650.0;
            let speed = self.ball_velocity.length();
            if speed > 2800.0 {
                self.ball_velocity *= 2800.0 / speed;
            }
            if self.ball_rotate {
                self.ball_angular_velocity = self.rolling_angular_velocity();
            }
            self.next_random_bounce = now
                + Duration::from_secs_f32(self.random_bounce_interval.clamp(0.5, 60.0));
        }

        // A fixed 120 Hz step keeps gravity, contact impulses and rotation stable
        // when the UI frame rate varies. At most 12 steps are simulated per frame
        // so a stalled window cannot create an unbounded catch-up loop.
        const PHYSICS_STEP: f32 = 1.0 / 120.0;
        const MAX_STEPS_PER_FRAME: usize = 12;
        self.ball_time_accumulator =
            (self.ball_time_accumulator + elapsed).min(PHYSICS_STEP * MAX_STEPS_PER_FRAME as f32);
        let mut position = self.position;
        let mut steps = 0;

        while self.ball_time_accumulator >= PHYSICS_STEP && steps < MAX_STEPS_PER_FRAME {
            self.ball_velocity.y += self.ball_gravity * PHYSICS_STEP;
            position += self.ball_velocity * PHYSICS_STEP;

            let hit_left = position.x < min_position.x;
            let hit_right = position.x > max_position.x;
            let hit_top = position.y < min_position.y;
            let hit_bottom = position.y > max_position.y;
            let bounced = hit_left || hit_right || hit_top || hit_bottom;

            if hit_left {
                position.x = min_position.x;
                self.resolve_ball_contact(egui::vec2(1.0, 0.0));
            } else if hit_right {
                position.x = max_position.x;
                self.resolve_ball_contact(egui::vec2(-1.0, 0.0));
            }
            if hit_top {
                position.y = min_position.y;
                self.resolve_ball_contact(egui::vec2(0.0, 1.0));
            } else if hit_bottom {
                position.y = max_position.y;
                self.resolve_ball_contact(egui::vec2(0.0, -1.0));
            }

            if bounced {
                if self.random_reflect_on_bounce {
                    let speed = self.ball_velocity.length().max(260.0);
                    let mut direction = Self::random_direction();
                    if hit_left {
                        direction.x = direction.x.abs();
                    } else if hit_right {
                        direction.x = -direction.x.abs();
                    }
                    if hit_top {
                        direction.y = direction.y.abs();
                    } else if hit_bottom {
                        direction.y = -direction.y.abs();
                    }
                    self.ball_velocity = direction * speed;
                    if self.ball_rotate {
                        self.ball_angular_velocity = self.rolling_angular_velocity();
                    }
                }
                if self.random_rotate_on_bounce {
                    self.randomize_rotation();
                }
            }

            if self.ball_rotate {
                self.rotation_radians = (self.rotation_radians
                    + self.ball_angular_velocity * PHYSICS_STEP)
                    .rem_euclid(std::f32::consts::TAU);
            }
            self.ball_time_accumulator -= PHYSICS_STEP;
            steps += 1;
        }

        if (position - self.position).length_sq() > 0.0001 {
            self.request_outer_position(ctx, position);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn paint_image(
        &self,
        ui: &egui::Ui,
        texture: &egui::TextureHandle,
        rect: egui::Rect,
        tint: egui::Color32,
    ) {
        if self.rotation_radians.abs() < 0.001 {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                tint,
            );
            return;
        }
        let center = rect.center();
        let half = rect.size() * 0.5;
        let (sin, cos) = self.rotation_radians.sin_cos();
        let rotate = |x: f32, y: f32| {
            center + egui::vec2(x * cos - y * sin, x * sin + y * cos)
        };
        let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
        mesh.vertices = vec![
            egui::epaint::Vertex {
                pos: rotate(-half.x, -half.y),
                uv: egui::pos2(0.0, 0.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(half.x, -half.y),
                uv: egui::pos2(1.0, 0.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(half.x, half.y),
                uv: egui::pos2(1.0, 1.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(-half.x, half.y),
                uv: egui::pos2(0.0, 1.0),
                color: tint,
            },
        ];
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        ui.painter().add(egui::Shape::mesh(mesh));
    }

    fn request_inner_size(&mut self, ctx: &egui::Context, size: egui::Vec2) {
        let size = self.size_for_width(size.x);
        let size_changed = (self.size - size).length_sq() > 0.25;
        self.size = size;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        if size_changed {
            self.expected_native_size = Some((size, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.request_repaint();
        }
    }

    fn request_geometry(&mut self, ctx: &egui::Context, size: egui::Vec2, position: egui::Pos2) {
        let size = self.size_for_width(size.x);
        let size_changed = (self.size - size).length_sq() > 0.25;
        let position_changed = (self.position - position).length_sq() > 0.25;
        self.size = size;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        self.position = position;

        if size_changed {
            self.expected_native_size = Some((size, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.request_repaint();
        }
        if position_changed {
            self.expected_native_position = Some((position, Instant::now()));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
            ctx.request_repaint();
        }
    }

    fn request_outer_position(&mut self, ctx: &egui::Context, position: egui::Pos2) {
        if (self.position - position).length_sq() <= 0.25 {
            return;
        }
        self.position = position;
        self.expected_native_position = Some((position, Instant::now()));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
        ctx.request_repaint();
    }

    fn synchronize_native_position(&mut self, ctx: &egui::Context) {
        if self.resize_gesture.is_some() || self.dvd_mode || self.ball_dragging {
            return;
        }
        let Some(actual_position) =
            ctx.input(|input| input.viewport().outer_rect.map(|rect| rect.min))
        else {
            return;
        };
        if !actual_position.x.is_finite() || !actual_position.y.is_finite() {
            return;
        }
        if let Some((expected, requested_at)) = self.expected_native_position {
            let acknowledged = (actual_position - expected).length_sq() <= 4.0;
            if acknowledged {
                self.expected_native_position = None;
            } else if requested_at.elapsed() < Duration::from_millis(1200) {
                return;
            } else {
                self.expected_native_position = None;
            }
        }
        self.position = actual_position;
    }

    fn synchronize_native_size(&mut self, ctx: &egui::Context) {
        if self.resize_gesture.is_some() {
            return;
        }
        let Some(actual_size) =
            ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()))
        else {
            return;
        };
        if !actual_size.x.is_finite()
            || !actual_size.y.is_finite()
            || actual_size.x <= 0.0
            || actual_size.y <= 0.0
        {
            return;
        }
        if let Some((expected, requested_at, retry_count)) = self.expected_native_size {
            let acknowledged = (actual_size - expected).length_sq() <= 4.0;
            if acknowledged {
                self.expected_native_size = None;
            } else if requested_at.elapsed() < Duration::from_millis(650) {
                // Native viewport information may still describe the previous
                // frame. Keep our requested size until winit reports the result.
                return;
            } else if retry_count < 2 {
                // A child viewport may miss a geometry command while it is being
                // recreated or while a window manager is processing a move/resize.
                // Retry the last requested size instead of reverting to stale data.
                self.expected_native_size = Some((expected, Instant::now(), retry_count + 1));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(expected));
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            } else {
                // After bounded retries, accept the native size. This also lets
                // the OS enforce a monitor/work-area constraint without an
                // endless resize loop.
                self.expected_native_size = None;
            }
        }
        let width_delta = ((actual_size.x - self.size.x) / self.size.x.max(1.0)).abs();
        let height_delta = ((actual_size.y - self.size.y) / self.size.y.max(1.0)).abs();
        if width_delta < 0.01 && height_delta < 0.01 {
            return;
        }

        // Native resize notifications may change only one dimension. Pick the
        // axis that moved most, then derive the other dimension from the source
        // aspect ratio. This also keeps the slider in sync with edge/corner drags.
        let target_width = if width_delta >= height_delta {
            actual_size.x
        } else {
            actual_size.y * self.aspect_ratio
        };
        let constrained = self.size_for_width(target_width);
        self.size = constrained;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        if (constrained - actual_size).length_sq() > 1.0 {
            self.expected_native_size = Some((constrained, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(constrained));
            ctx.request_repaint();
        }
    }

    fn render(&mut self, ui: &mut egui::Ui, tab_id: u64) -> bool {
        let ctx = ui.ctx().clone();
        self.synchronize_native_size(&ctx);
        self.synchronize_native_position(&ctx);
        self.update_pass_opacity(&ctx);
        self.update_dvd_position(&ctx);
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        let viewport_rect = ui.max_rect();
        self.update_animation_texture(&ctx, tab_id);
        let texture = self.display_texture.clone();

        ui.painter().rect_filled(
            viewport_rect,
            egui::CornerRadius::ZERO,
            egui::Color32::TRANSPARENT,
        );
        // The native window is sized to the source aspect ratio. Painting into
        // the full client rect therefore wraps the image without letterboxing.
        // When decoded pixels are available, opacity has already been applied
        // to alpha alone. Keep the tint white so RGB channels are never darkened.
        // Fallback texture paths still use egui's premultiplied tint handling.
        let fallback_tint = if self.source_pixels.is_none() && self.animation.is_none() {
            egui::Color32::from_white_alpha(opacity_to_alpha(self.displayed_opacity_percent))
        } else {
            egui::Color32::WHITE
        };
        self.paint_image(ui, &texture, viewport_rect, fallback_tint);
        if self.show_bounds {
            let color = egui::Color32::from_rgba_unmultiplied(255, 190, 70, 235);
            let rect = viewport_rect.shrink(1.0);
            let painter = ui.painter();
            painter.line_segment([rect.left_top(), rect.right_top()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.right_top(), rect.right_bottom()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.right_bottom(), rect.left_bottom()], egui::Stroke::new(2.0_f32, color));
            painter.line_segment([rect.left_bottom(), rect.left_top()], egui::Stroke::new(2.0_f32, color));
        }

        if !self.click_through {
            // Partition the client area into explicit move regions. The previous
            // full-window drag widget overlapped all resize handles; although the
            // code tried to suppress StartDrag by checking the pointer afterward,
            // egui could still capture the pointer for that full-surface widget.
            // Non-overlapping regions make move, resize, and toolbar gestures
            // mutually exclusive before input ownership is assigned.
            let corner = RESIZE_CORNER
                .min(viewport_rect.width() * 0.25)
                .min(viewport_rect.height() * 0.25);
            // On extremely panoramic/portrait images the aspect-preserving
            // window can be shorter than the normal 9-point edge target. Scale
            // edge depth down to the corner size so horizontal/vertical handles
            // still meet cleanly instead of overlapping one another.
            let edge = RESIZE_EDGE.min(corner);
            let left = viewport_rect.left();
            let right = viewport_rect.right();
            let top = viewport_rect.top();
            let bottom = viewport_rect.bottom();
            let center_left = left + corner;
            let center_right = right - corner;
            let center_top = top + corner;
            let center_bottom = bottom - corner;
            let controls_fit = viewport_rect.height() >= 48.0;
            let move_regions = if controls_fit {
                let bar = Self::control_bar_rect(viewport_rect);
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center_above_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, bar.top()),
                        ),
                    ),
                    (
                        "center_left_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.top()),
                            egui::pos2(bar.left(), bar.bottom()),
                        ),
                    ),
                    (
                        "center_right_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(bar.right(), bar.top()),
                            egui::pos2(center_right, bar.bottom()),
                        ),
                    ),
                    (
                        "center_below_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.bottom()),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            } else {
                // Very short aspect-ratio windows cannot physically fit the
                // 30-point hover toolbar. Keep the whole non-resize interior
                // draggable and leave window actions available in the tab panel.
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            };
            for (name, rect) in move_regions {
                if !rect.is_positive() {
                    continue;
                }
                let drag = ui.interact(
                    rect,
                    egui::Id::new(("jpeg_viewer_popout_move", tab_id, name)),
                    egui::Sense::drag(),
                );
                if drag.drag_started() {
                    if self.ball_mode {
                        // Pause the simulation and delegate the actual window move to
                        // the compositor. Never feed app-issued position commands
                        // back into an OS drag: that feedback loop causes jitter.
                        self.ball_dragging = true;
                        self.ball_velocity = egui::Vec2::ZERO;
                        self.ball_throw_velocity = egui::Vec2::ZERO;
                        self.ball_time_accumulator = 0.0;
                    } else {
                        self.dvd_mode = false;
                    }
                    self.expected_native_position = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    #[cfg(target_os = "linux")]
                    crate::platform::wayland_pointer_recovery::after_native_window_operation(&ctx);
                }
                if self.ball_mode && self.ball_dragging && drag.drag_stopped() {
                    // Native drag has moved the window already; avoid an inferred
                    // velocity from surface-relative pointer coordinates, which
                    // change as the window follows the pointer.
                    self.ball_dragging = false;
                    self.ball_last_drag_pointer = None;
                    self.ball_velocity = egui::Vec2::ZERO;
                    self.ball_angular_velocity = 0.0;
                    self.ball_time_accumulator = 0.0;
                    self.last_dvd_tick = Instant::now();
                }
            }

            let pointer_hovering_window = ctx
                .pointer_hover_pos()
                .is_some_and(|position| viewport_rect.contains(position));
            // Keep resize hit zones registered while pointer is inside or a drag
            // is active. The hover toolbar appears only while pointer is inside.
            self.render_hover_controls(ui, viewport_rect, tab_id, pointer_hovering_window);
        }
        let close_requested = close_requested || self.close_requested;
        if close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        close_requested
    }

    fn control_bar_rect(viewport_rect: egui::Rect) -> egui::Rect {
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        // Keep the controls entirely inside the central area between the
        // enlarged corner-resize hotspots, so toolbar clicks never overlap a
        // move region on compact pop-out windows.
        let width = (viewport_rect.width() - corner * 2.0 - 8.0).clamp(54.0, 119.0);
        egui::Rect::from_center_size(
            egui::pos2(
                viewport_rect.center().x,
                viewport_rect.top() + corner + 16.0,
            ),
            egui::vec2(width, 30.0),
        )
    }

    fn render_hover_controls(
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        tab_id: u64,
        show_buttons: bool,
    ) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter().clone();
        if show_buttons && viewport_rect.height() >= 48.0 {
            let gap = 4.0;
            let bar_rect = Self::control_bar_rect(viewport_rect);
            let bar_width = bar_rect.width();
            let button_width = ((bar_width - 16.0 - gap * 2.0) / 3.0).max(10.0);
            let button_size = egui::vec2(button_width, 24.0);
            painter.rect_filled(
                bar_rect,
                egui::CornerRadius::same(15),
                egui::Color32::from_black_alpha(165),
            );
            let first = bar_rect.left() + (bar_width - (button_size.x * 3.0 + gap * 2.0)) * 0.5;
            let top_rect =
                egui::Rect::from_min_size(egui::pos2(first, bar_rect.top() + 3.0), button_size);
            let bottom_rect = egui::Rect::from_min_size(
                egui::pos2(first + button_size.x + gap, bar_rect.top() + 3.0),
                button_size,
            );
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(first + (button_size.x + gap) * 2.0, bar_rect.top() + 3.0),
                button_size,
            );
            if Self::hover_button(
                ui,
                tab_id,
                top_rect,
                if self.stay_on_top { "●" } else { "↑" },
                "Stay on top",
            ) {
                self.stay_on_top = !self.stay_on_top;
                if self.stay_on_top {
                    self.stay_on_bottom = false;
                }
            }
            if Self::hover_button(
                ui,
                tab_id,
                bottom_rect,
                if self.stay_on_bottom { "●" } else { "↓" },
                "Stay on bottom",
            ) {
                self.stay_on_bottom = !self.stay_on_bottom;
                if self.stay_on_bottom {
                    self.stay_on_top = false;
                }
            }
            if Self::hover_button(ui, tab_id, close_rect, "×", "Close pop-out") {
                // Notify the root manager before requesting native closure. If
                // this flag is not set, the child viewport disappears but the
                // manager still considers the tab's pop-out open, preventing
                // the user from opening it again.
                self.close_requested = true;
            }
        }

        // Full four-edge resize strips and larger diagonal corner hit zones.
        // The top strip remains usable because the floating toolbar is inset
        // well below it; corner handles win over the generic move surface.
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        let edge = RESIZE_EDGE.min(corner);
        let left = viewport_rect.left();
        let right = viewport_rect.right();
        let top = viewport_rect.top();
        let bottom = viewport_rect.bottom();
        let zones = [
            (
                "north",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, top),
                    egui::pos2(right - corner, top + edge),
                ),
                0.0,
                -1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "south",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, bottom - edge),
                    egui::pos2(right - corner, bottom),
                ),
                0.0,
                1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "west",
                egui::Rect::from_min_max(
                    egui::pos2(left, top + corner),
                    egui::pos2(left + edge, bottom - corner),
                ),
                -1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "east",
                egui::Rect::from_min_max(
                    egui::pos2(right - edge, top + corner),
                    egui::pos2(right, bottom - corner),
                ),
                1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "north_west",
                egui::Rect::from_min_size(viewport_rect.min, egui::vec2(corner, corner)),
                -1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthWest,
            ),
            (
                "north_east",
                egui::Rect::from_min_size(
                    egui::pos2(right - corner, top),
                    egui::vec2(corner, corner),
                ),
                1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthEast,
            ),
            (
                "south_west",
                egui::Rect::from_min_size(
                    egui::pos2(left, bottom - corner),
                    egui::vec2(corner, corner),
                ),
                -1.0,
                1.0,
                egui::CursorIcon::ResizeSouthWest,
            ),
            (
                "south_east",
                egui::Rect::from_min_size(
                    viewport_rect.right_bottom() - egui::vec2(corner, corner),
                    egui::vec2(corner, corner),
                ),
                1.0,
                1.0,
                egui::CursorIcon::ResizeSouthEast,
            ),
        ];
        for (name, rect, horizontal, vertical, cursor) in zones {
            let response = ui.interact(
                rect,
                egui::Id::new(("jpeg_viewer_popout_resize", tab_id, name)),
                egui::Sense::drag(),
            );
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started() {
                self.resize_gesture = Some(ResizeGesture {
                    start_size: self.size,
                    start_position: self.position,
                    horizontal,
                    vertical,
                    accumulated_delta: egui::Vec2::ZERO,
                });
                self.dvd_mode = false;
            }
            if response.dragged() {
                if let Some(gesture) = self.resize_gesture {
                    // Resize is based on total movement from the original press.
                    // drag_delta() is only the movement since the previous frame;
                    // adding it to start_size causes a reset/flicker every frame.
                    let mut gesture = gesture;
                    if let Some(total_delta) = response.total_drag_delta() {
                        gesture.accumulated_delta = total_delta;
                    } else {
                        gesture.accumulated_delta += response.drag_delta();
                    }
                    self.resize_gesture = Some(gesture);
                    let delta = gesture.accumulated_delta;
                    let horizontal_scale = if gesture.horizontal != 0.0 {
                        (gesture.start_size.x + delta.x * gesture.horizontal)
                            / gesture.start_size.x.max(1.0)
                    } else {
                        1.0
                    };
                    let vertical_scale = if gesture.vertical != 0.0 {
                        (gesture.start_size.y + delta.y * gesture.vertical)
                            / gesture.start_size.y.max(1.0)
                    } else {
                        1.0
                    };
                    let scale = match (gesture.horizontal != 0.0, gesture.vertical != 0.0) {
                        (true, true) => {
                            if (horizontal_scale - 1.0).abs() >= (vertical_scale - 1.0).abs() {
                                horizontal_scale
                            } else {
                                vertical_scale
                            }
                        }
                        (true, false) => horizontal_scale,
                        (false, true) => vertical_scale,
                        (false, false) => 1.0,
                    };
                    let new_size = self.size_for_width(gesture.start_size.x * scale);
                    let mut new_position = gesture.start_position;
                    if gesture.horizontal < 0.0 {
                        new_position.x += gesture.start_size.x - new_size.x;
                    } else if gesture.horizontal == 0.0 {
                        new_position.x += (gesture.start_size.x - new_size.x) * 0.5;
                    }
                    if gesture.vertical < 0.0 {
                        new_position.y += gesture.start_size.y - new_size.y;
                    } else if gesture.vertical == 0.0 {
                        new_position.y += (gesture.start_size.y - new_size.y) * 0.5;
                    }
                    self.request_geometry(&ctx, new_size, new_position);
                }
            }
            if response.drag_stopped() {
                self.resize_gesture = None;
            }
        }
    }

    fn hover_button(
        ui: &mut egui::Ui,
        tab_id: u64,
        rect: egui::Rect,
        label: &str,
        tooltip: &str,
    ) -> bool {
        let response = ui.interact(
            rect,
            egui::Id::new(("jpeg_viewer_popout_button", tab_id, label)),
            egui::Sense::click(),
        );
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(6),
            if response.hovered() && label == "×" {
                egui::Color32::from_rgb(175, 55, 60)
            } else if response.hovered() {
                egui::Color32::from_black_alpha(220)
            } else {
                egui::Color32::from_black_alpha(155)
            },
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        response.on_hover_text(tooltip).clicked()
    }
}

fn opacity_to_alpha(opacity_percent: f32) -> u8 {
    (opacity_percent.clamp(0.0, 100.0) * 2.55).round() as u8
}

fn apply_opacity_to_rgba(rgba: &[u8], opacity: u8) -> Vec<u8> {
    let mut adjusted = rgba.to_vec();
    for pixel in adjusted.chunks_exact_mut(4) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity) + 127) / 255) as u8;
    }
    adjusted
}

fn fit_aspect_size(aspect_ratio: f32, bounds: egui::Vec2) -> egui::Vec2 {
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

fn ensure_jvp_extension(path: std::path::PathBuf) -> std::path::PathBuf {
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

fn preset_from_window(window: &PopoutWindow) -> PopoutPreset {
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
        stay_on_top: window.stay_on_top,
        stay_on_bottom: window.stay_on_bottom,
    }
}

fn apply_preset_to_window(
    preset: &PopoutPreset,
    window: &mut PopoutWindow,
    ctx: Option<&egui::Context>,
    tab_id: u64,
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
    window.dvd_mode = preset.dvd_mode;
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
    window.next_random_bounce = Instant::now()
        + Duration::from_secs_f32(preset.random_bounce_interval);
    window.stay_on_top = preset.stay_on_top;
    window.stay_on_bottom = preset.stay_on_bottom;
    let requested_size = window.size_for_scale(preset.size_scale);
    window.size = requested_size;
    window.expected_native_size = Some((requested_size, Instant::now(), 0));
    window.rotation_radians = 0.0;
    window.dvd_rotation_target = 0.0;
    window.ball_velocity = egui::vec2(360.0, -220.0);
    window.ball_angular_velocity = window.rolling_angular_velocity();
    window.last_dvd_tick = Instant::now();
    if let Some(ctx) = ctx {
        ctx.send_viewport_cmd_to(
            window.viewport_id,
            egui::ViewportCommand::InnerSize(requested_size),
        );
        ctx.request_repaint();
    }
}

impl PopoutManager {
    pub(crate) fn has_active_popouts(&self) -> bool {
        !self.windows.is_empty()
    }

    pub(crate) fn has_tab_popout(&self, tab_id: u64) -> bool {
        self.windows.contains_key(&tab_id)
    }

    pub(crate) fn is_open(&self, tab_id: u64) -> bool {
        self.windows.contains_key(&tab_id)
    }

    pub(crate) fn settings_sidebar_open(&self, tab_id: u64) -> bool {
        self.settings_sidebar_tab == Some(tab_id)
    }

    pub(crate) fn toggle_settings_sidebar(&mut self, tab_id: u64) {
        self.settings_sidebar_tab = if self.settings_sidebar_tab == Some(tab_id) {
            None
        } else {
            Some(tab_id)
        };
    }

    pub(crate) fn take_pending_image_open(&mut self) -> Option<(ImageEntry, PopoutPreset)> {
        self.pending_image_open.take()
    }

    pub(crate) fn take_status_message(&mut self) -> Option<String> {
        self.status_message.take()
    }

    pub(crate) fn assign_preset(&mut self, tab_id: u64, mut preset: PopoutPreset, ctx: &egui::Context) {
        preset.sanitize();
        self.presets.insert(tab_id, preset.clone());
        if let Some(state) = self.windows.get(&tab_id)
            && let Ok(mut window) = state.lock()
        {
            apply_preset_to_window(&preset, &mut window, Some(ctx), tab_id);
        }
    }

    pub(crate) fn close_tab(&mut self, tab_id: u64) {
        if self.settings_sidebar_tab == Some(tab_id) {
            self.settings_sidebar_tab = None;
        }
        if let Some(state) = self.windows.remove(&tab_id) {
            if let Ok(window) = state.lock() {
                self.presets.insert(tab_id, preset_from_window(&window));
                self.pending_close.push(window.viewport_id);
            }
        }
    }

    fn open_tab(
        &mut self,
        tab_id: u64,
        texture: egui::TextureHandle,
        animation: Option<GifAnimation>,
        source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
        ctx: &egui::Context,
    ) {
        let generation = self.next_viewport_generation;
        self.next_viewport_generation = self.next_viewport_generation.wrapping_add(1);
        let viewport_id =
            egui::ViewportId::from_hash_of(("jpeg_viewer_popout", tab_id, generation));
        let image_size = texture.size_vec2().max(egui::vec2(1.0, 1.0));
        let aspect_ratio = (image_size.x / image_size.y).clamp(0.03, 32.0);
        let base_size = fit_aspect_size(aspect_ratio, DEFAULT_SIZE);
        let main_rect = ctx.input(|input| input.viewport().outer_rect);
        let monitors = crate::platform::monitors::available_monitors();
        let monitor = crate::platform::monitors::resolve_default_monitor_at(
            &monitors,
            crate::platform::monitors::CURRENT_WINDOW_MONITOR,
            main_rect,
        )
        .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
        .or_else(|| monitors.first());
        let position = monitor
            .map(|monitor| monitor.bounds().center() - base_size * 0.5)
            .unwrap_or(egui::pos2(120.0, 120.0));
        let monitor_id = monitor.map(|monitor| monitor.id.clone());
        let mut animation = animation;
        if let Some(animation) = animation.as_mut() {
            animation.is_playing = true;
            animation.last_update = Instant::now();
        }
        let display_texture = texture.clone();
        let window = PopoutWindow {
                viewport_id,
                viewport_generation: generation,
                texture,
                animation,
                last_frame_index: None,
                size: base_size,
                base_size,
                aspect_ratio,
                size_scale: 1.0,
                pending_size_scale: 1.0,
                expected_native_size: None,
                expected_native_position: None,
                viewport_initialized: false,
                source_pixels,
                display_texture,
                rendered_opacity: u8::MAX,
                opacity_percent: 100.0,
                displayed_opacity_percent: 100.0,
                click_through: false,
                pass_near_enabled: false,
                pass_near_distance: 120.0,
                pass_near_opacity: 75.0,
                pass_hover_enabled: false,
                pass_hover_opacity: 100.0,
                pointer_screen_pos: None,
                show_bounds: false,
                dvd_mode: false,
                dvd_circle_collision: false,
                random_reflect_on_bounce: false,
                random_rotate_on_bounce: false,
                rotation_radians: 0.0,
                dvd_rotation_target: 0.0,
                dvd_rotation_ease: 0.7,
                dvd_speed: 1.0,
                ball_mode: false,
                ball_gravity: 1100.0,
                ball_bounciness: 0.82,
                ball_friction: 0.18,
                ball_velocity: egui::vec2(360.0, -220.0),
                ball_rotate: true,
                ball_time_accumulator: 0.0,
                ball_angular_velocity: 0.0,
                ball_dragging: false,
                ball_last_drag_pointer: None,
                ball_throw_velocity: egui::Vec2::ZERO,
                random_bounce_enabled: false,
                random_bounce_interval: 6.0,
                next_random_bounce: Instant::now() + Duration::from_secs(6),
                dvd_edge_offsets: [0.0; 4],
                monitor_id,
                stay_on_top: false,
                stay_on_bottom: false,
                applied_window_level: None,
                position,
                dvd_velocity: egui::vec2(1.0, 1.0),
                last_dvd_tick: Instant::now(),
                last_bottom_enforcement: Instant::now(),
                resize_gesture: None,
                close_requested: false,
                native_owner_detached: false,
                #[cfg(target_os = "windows")]
                dwm_transparency_applied: false,
            };
        let mut window = window;
        if let Some(preset) = self.presets.get(&tab_id) {
            apply_preset_to_window(preset, &mut window, None, tab_id);
        }
        self.windows.insert(tab_id, Arc::new(Mutex::new(window)));
    }

    /// Right-hand Pop-out settings sidebar for the active Image tab. Tricks are
    /// a first-class section, and JVP preset actions remain available even while
    /// the native pop-out is closed.
    pub(crate) fn render_settings_sidebar(
        &mut self,
        ctx: &egui::Context,
        panel_rect: egui::Rect,
        tab_id: u64,
        texture: Option<egui::TextureHandle>,
        animation: Option<GifAnimation>,
        source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
        image_entry: Option<ImageEntry>,
    ) {
        let mut open_requested = false;
        let mut close_requested = false;
        let mut request_save = false;
        let mut request_load = false;
        let mut close_sidebar = false;
        let monitors = crate::platform::monitors::available_monitors();
        let window_state = self.windows.get(&tab_id).cloned();
        let is_open = window_state.is_some();
        let mut window_guard = window_state.as_ref().and_then(|state| state.lock().ok());
        let mut preset_snapshot = self.presets.get(&tab_id).cloned().unwrap_or_default();
        let status_message = self.status_message.clone();

        // A larger inset and generous instrument spacing separate the deck
        // visually from the image while leaving the image viewport unobscured.
        let panel_rect = panel_rect.shrink(14.0);
        // Use the panel's actual usable width rather than the remaining width
        // at the current point in a wrapped row. The latter changes after each
        // preceding control and made the size/opacity sliders wrap unpredictably.
        let panel_inner_width = (panel_rect.width() - 36.0).max(1.0);
        let slider_width = (panel_inner_width * 0.18)
            .clamp(64.0, 170.0)
            .min(panel_inner_width);
        egui::Area::new(egui::Id::new(("jpeg_viewer_popout_settings", tab_id)))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_rect.min)
            .movable(false)
            .interactable(true)
            .show(ctx, |area_ui| {
                area_ui.set_min_size(panel_rect.size());
                area_ui.set_max_size(panel_rect.size());
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .stroke(ctx.style().visuals.widgets.noninteractive.bg_stroke)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(area_ui, |ui| {
                        ui.set_min_width(panel_inner_width);
                        ui.set_max_width(panel_inner_width);
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
                        ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("JVP  /  FLIGHT DECK")
                                    .strong()
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(246, 190, 92)),
                            );
                            ui.add_space(8.0);
                            if is_open {
                                ui.label(
                                    egui::RichText::new("● POP-OUT ACTIVE")
                                        .color(egui::Color32::from_rgb(93, 223, 166))
                                        .strong()
                                        .size(12.0),
                                );
                                if ui.button("Close pop-out").clicked() {
                                    close_requested = true;
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new("STANDBY")
                                        .color(egui::Color32::from_rgb(165, 184, 201))
                                        .strong()
                                        .size(11.0),
                                );
                                if ui
                                    .add_enabled(texture.is_some(), egui::Button::new("Open pop-out"))
                                    .clicked()
                                {
                                    open_requested = true;
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Close Pop-out settings")
                                    .clicked()
                                {
                                    close_sidebar = true;
                                }
                                if ui.button("Load .jvp").clicked() {
                                    request_load = true;
                                }
                                if ui.button("Save .jvp").clicked() {
                                    request_save = true;
                                }
                            });
                        });

                        ui.separator();
                        if let Some(window) = window_guard.as_deref_mut() {
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(window.click_through, "⇢ PASS")
                                    .on_hover_text("Let pointer clicks pass through the pop-out")
                                    .clicked()
                                {
                                    window.click_through = !window.click_through;
                                }
                                if ui
                                    .selectable_label(window.show_bounds, "▢ BOUNDS")
                                    .on_hover_text("Draw the native pop-out bounds even when Pass is off")
                                    .clicked()
                                {
                                    window.show_bounds = !window.show_bounds;
                                }
                                if ui
                                    .selectable_label(window.stay_on_top, "↑ TOP")
                                    .on_hover_text("Stay above other apps until the main window is activated")
                                    .clicked()
                                {
                                    window.stay_on_top = !window.stay_on_top;
                                    if window.stay_on_top {
                                        window.stay_on_bottom = false;
                                    }
                                }
                                if ui
                                    .selectable_label(window.stay_on_bottom, "↓ BOTTOM")
                                    .on_hover_text("Keep the pop-out below other windows")
                                    .clicked()
                                {
                                    window.stay_on_bottom = !window.stay_on_bottom;
                                    if window.stay_on_bottom {
                                        window.stay_on_top = false;
                                    }
                                }

                                let min_scale =
                                    window.minimum_size().x / window.base_size.x.max(1.0);
                                let max_scale = (window.maximum_size().x
                                    / window.base_size.x.max(1.0))
                                    .max(min_scale + 0.01);
                                ui.label("SIZE");
                                let size_response = ui.add_sized(
                                    [slider_width, 18.0],
                                    egui::Slider::new(
                                        &mut window.pending_size_scale,
                                        min_scale..=max_scale,
                                    )
                                    .suffix("×")
                                    .show_value(true)
                                    .logarithmic(true),
                                );
                                if size_response.changed()
                                    && (window.pending_size_scale - window.size_scale).abs() > 0.001
                                {
                                    window.size_scale = window.pending_size_scale;
                                    let requested_size = window.size_for_scale(window.size_scale);
                                    window.size = requested_size;
                                    window.expected_native_size =
                                        Some((requested_size, Instant::now(), 0));
                                    ctx.send_viewport_cmd_to(
                                        window.viewport_id,
                                        egui::ViewportCommand::InnerSize(requested_size),
                                    );
                                    ctx.request_repaint();
                                }
                                ui.label("OPACITY");
                                ui.add_sized(
                                    [slider_width, 18.0],
                                    egui::Slider::new(&mut window.opacity_percent, 0.0..=100.0)
                                        .suffix("%")
                                        .show_value(false),
                                );
                            });

                            ui.separator();
                            ui.horizontal_wrapped(|ui| {
                                ui.label("MONITOR");
                                let selected_name = window.monitor_id.as_deref()
                                    .and_then(|id| monitors.iter().find(|monitor| monitor.id == id))
                                    .map(|monitor| monitor.display_label())
                                    .unwrap_or_else(|| "Primary".to_owned());
                                let mut selected_monitor = window.monitor_id.clone();
                                egui::ComboBox::from_id_salt(("popout_monitor_common", tab_id))
                                    .selected_text(selected_name)
                                    .width(170.0)
                                    .show_ui(ui, |combo| {
                                        for monitor in &monitors {
                                            combo.selectable_value(&mut selected_monitor,
                                                Some(monitor.id.clone()), monitor.display_label());
                                        }
                                    });
                                if selected_monitor != window.monitor_id {
                                    window.monitor_id = selected_monitor;
                                    if let Some(monitor) = window.monitor_id.as_deref()
                                        .and_then(|id| monitors.iter().find(|monitor| monitor.id == id)) {
                                        window.request_outer_position(ctx,
                                            monitor.bounds().center() - window.size * 0.5);
                                        window.last_dvd_tick = Instant::now();
                                    }
                                }
                                ui.label("ROTATE");
                                let mut angle_degrees = window.rotation_radians.to_degrees();
                                if ui.add(egui::Slider::new(&mut angle_degrees, -180.0..=180.0)
                                    .suffix("°").show_value(true)).changed() {
                                    window.rotation_radians = angle_degrees.to_radians();
                                    window.dvd_rotation_target = window.rotation_radians;
                                }
                            });

                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("TRICKS  /  MOVEMENT & OPTICS")
                                        .strong()
                                        .color(egui::Color32::from_rgb(246, 190, 92)),
                                );
                                ui.label(
                                    egui::RichText::new("FLIGHT SYSTEMS")
                                        .weak()
                                        .size(9.0),
                                );
                            });

                            let movement_supported =
                                PopoutWindow::automatic_window_movement_supported();
                            egui::ScrollArea::vertical()
                                .id_salt(("popout_tricks_scroll", tab_id))
                                .max_height((panel_rect.height() - 132.0).max(90.0))
                                .auto_shrink([false, false])
                                .show(ui, |scroll_ui| {
                                    egui::Frame::new()
                                        .fill(deck.ctx().style().visuals.window_fill())
                                        .stroke(deck.ctx().style().visuals.widgets.noninteractive.bg_stroke)
                                        .corner_radius(egui::CornerRadius::same(8))
                                        .inner_margin(egui::Margin::same(12))
                                        .show(scroll_ui, |deck| {
                                            deck.horizontal_wrapped(|deck| {
                                                deck.group(|controls| {
                                                    controls.set_min_width(210.0);
                                                    controls.label(
                                                        egui::RichText::new("01  MOVEMENT")
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(113, 190, 231)),
                                                    );
                                                    controls.add_enabled_ui(movement_supported, |controls| {
                                                        let was_dvd = window.dvd_mode;
                                                        let was_ball = window.ball_mode;
                                                        controls.checkbox(&mut window.dvd_mode, "DVD effect");
                                                        controls.checkbox(&mut window.ball_mode, "Ball mode");
                                                        if window.dvd_mode && !was_dvd {
                                                            window.ball_mode = false;
                                                            window.last_dvd_tick = Instant::now();
                                                        }
                                                        if window.ball_mode && !was_ball {
                                                            window.dvd_mode = false;
                                                            window.ball_velocity = egui::vec2(360.0, -220.0);
                                                            window.ball_angular_velocity = window.rolling_angular_velocity();
                                                            window.ball_time_accumulator = 0.0;
                                                            window.last_dvd_tick = Instant::now();
                                                        }
                                                        if was_dvd != window.dvd_mode || was_ball != window.ball_mode {
                                                            window.next_random_bounce = Instant::now()
                                                                + Duration::from_secs_f32(window.random_bounce_interval);
                                                        }

                                                        if window.dvd_mode {
                                                            controls.add(
                                                                egui::Slider::new(&mut window.dvd_speed, 0.1..=4.0)
                                                                    .text("DVD speed")
                                                                    .suffix("×"),
                                                            );
                                                            controls.add(
                                                                egui::Slider::new(&mut window.dvd_rotation_ease, 0.1..=4.0)
                                                                    .text("Rotation ease")
                                                                    .suffix(" s"),
                                                            );
                                                            controls.checkbox(
                                                                &mut window.dvd_circle_collision,
                                                                "Circular collision",
                                                            );
                                                            controls.label("Monitor edge offsets");
                                                            for (index, label) in ["Left", "Top", "Right", "Bottom"]
                                                                .into_iter()
                                                                .enumerate()
                                                            {
                                                                controls.add(
                                                                    egui::Slider::new(
                                                                        &mut window.dvd_edge_offsets[index],
                                                                        0.0..=1000.0,
                                                                    )
                                                                    .text(label)
                                                                    .suffix(" pt"),
                                                                );
                                                            }
                                                        }

                                                        if window.ball_mode {
                                                            controls.add(
                                                                egui::Slider::new(&mut window.ball_gravity, 0.0..=2400.0)
                                                                    .text("Gravity")
                                                                    .suffix(" px/s²"),
                                                            );
                                                            controls.add(
                                                                egui::Slider::new(&mut window.ball_bounciness, 0.0..=1.0)
                                                                    .text("Bounciness"),
                                                            );
                                                            controls.add(
                                                                egui::Slider::new(&mut window.ball_friction, 0.0..=5.0)
                                                                    .text("Surface friction"),
                                                            );
                                                            let was_rotating = window.ball_rotate;
                                                            controls.checkbox(&mut window.ball_rotate, "Rotate from velocity and impacts");
                                                            if window.ball_rotate && !was_rotating {
                                                                window.ball_angular_velocity = window.rolling_angular_velocity();
                                                            }
                                                            controls.label(
                                                                egui::RichText::new("Surface friction transfers motion into spin only at collisions.")
                                                                    .weak()
                                                                    .size(9.0),
                                                            );
                                                        }
                                                    });
                                                    if !movement_supported {
                                                        controls.label(
                                                            egui::RichText::new("Automatic movement needs X11 or a platform with window-position control. Manual drag still works.")
                                                                .weak()
                                                                .size(9.0),
                                                        );
                                                    }
                                                });

                                                deck.group(|controls| {
                                                    controls.set_min_width(190.0);
                                                    controls.label(
                                                        egui::RichText::new("02  BOUNCE LOGIC")
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(113, 190, 231)),
                                                    );
                                                    controls.checkbox(
                                                        &mut window.random_reflect_on_bounce,
                                                        "Random reflect after bounce",
                                                    );
                                                    controls.checkbox(
                                                        &mut window.random_rotate_on_bounce,
                                                        "Random rotate after bounce",
                                                    );
                                                    let can_random_bounce = window.dvd_mode || window.ball_mode;
                                                    if controls
                                                        .add_enabled(
                                                            can_random_bounce || window.random_bounce_enabled,
                                                            egui::Button::new(if window.random_bounce_enabled {
                                                                "■ Stop timed bounce"
                                                            } else {
                                                                "▶ Random bounce on timer"
                                                            }),
                                                        )
                                                        .clicked()
                                                    {
                                                        window.random_bounce_enabled = !window.random_bounce_enabled;
                                                        window.next_random_bounce = Instant::now()
                                                            + Duration::from_secs_f32(window.random_bounce_interval);
                                                    }
                                                    controls.add_enabled(
                                                        can_random_bounce || window.random_bounce_enabled,
                                                        egui::Slider::new(&mut window.random_bounce_interval, 0.5..=30.0)
                                                            .text("Interval")
                                                            .suffix(" s"),
                                                    );
                                                    if !can_random_bounce && !window.random_bounce_enabled {
                                                        controls.label(
                                                            egui::RichText::new("Activate DVD or Ball first.")
                                                                .weak()
                                                                .size(9.0),
                                                        );
                                                    }
                                                });

                                                deck.group(|controls| {
                                                    controls.set_min_width(210.0);
                                                    controls.label(
                                                        egui::RichText::new("03  PASS OPTICS")
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(113, 190, 231)),
                                                    );
                                                    controls.checkbox(
                                                        &mut window.pass_near_enabled,
                                                        "Fade in when pointer is near",
                                                    );
                                                    if window.pass_near_enabled {
                                                        controls.add(
                                                            egui::Slider::new(&mut window.pass_near_distance, 10.0..=500.0)
                                                                .text("Near distance")
                                                                .suffix(" pt"),
                                                        );
                                                        controls.add(
                                                            egui::Slider::new(&mut window.pass_near_opacity, 0.0..=100.0)
                                                                .text("Near opacity")
                                                                .suffix("%"),
                                                        );
                                                    }
                                                    controls.checkbox(
                                                        &mut window.pass_hover_enabled,
                                                        "Fade in when pointer is over image",
                                                    );
                                                    if window.pass_hover_enabled {
                                                        controls.add(
                                                            egui::Slider::new(&mut window.pass_hover_opacity, 0.0..=100.0)
                                                                .text("Hover opacity")
                                                                .suffix("%"),
                                                        );
                                                    }
                                                    if !window.click_through {
                                                        controls.label(
                                                            egui::RichText::new("Enable PASS for pointer-triggered opacity.")
                                                                .weak()
                                                                .size(9.0),
                                                        );
                                                    }
                                                    #[cfg(target_os = "linux")]
                                                    if !crate::platform::pointer::global_position_available() {
                                                        controls.label(
                                                            egui::RichText::new("Wayland cannot provide desktop-wide pointer coordinates.")
                                                                .weak()
                                                                .size(9.0),
                                                        );
                                                    }
                                                });
                                            });
                                        });
                                });
                        } else {
                            ui.label(
                                egui::RichText::new("Open the pop-out to configure movement, bounce and Pass optics. Saved presets can be loaded before opening it.")
                                    .weak()
                                    .size(10.0),
                            );
                        }

                        if let Some(message) = status_message.as_deref() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(message)
                                    .color(egui::Color32::from_rgb(246, 190, 92))
                                    .size(10.0),
                            );
                        }
                    });
            });

        if let Some(window) = window_guard.as_deref() {
            preset_snapshot = preset_from_window(window);
        }
        drop(window_guard);

        if is_open {
            self.presets.insert(tab_id, preset_snapshot.clone());
        }
        if close_sidebar {
            self.settings_sidebar_tab = None;
        }
        if close_requested {
            self.close_tab(tab_id);
        } else if open_requested && let Some(texture) = texture {
            self.open_tab(tab_id, texture, animation, source_pixels, ctx);
        }

        if request_save {
            self.save_dialog = Some(SaveDialogState {
                tab_id,
                preset: preset_snapshot,
                image_entry: image_entry.clone(),
                choice: if image_entry.is_some() {
                    SaveImageChoice::Link
                } else {
                    SaveImageChoice::PresetOnly
                },
            });
        }
        if request_load
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("JPEG Viewer Pop-out preset", &["jvp"])
                .pick_file()
        {
            match jvp::load_jvp(&path) {
                Ok(document) => self.load_dialog = Some(LoadDialogState { tab_id, document }),
                Err(error) => self.status_message = Some(format!("Could not load .jvp: {error}")),
            }
        }
        self.render_jvp_dialogs(ctx);
    }

    fn render_jvp_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.save_dialog.take() {
            let mut keep_open = true;
            let mut close_dialog = false;
            let mut save_requested = false;
            egui::Window::new("Save Pop-out Preset (.jvp)")
                .id(egui::Id::new(("popout_jvp_save_dialog", dialog.tab_id)))
                .collapsible(false)
                .resizable(false)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label("Choose how the current image is stored in this preset:");
                    ui.label(
                        egui::RichText::new("Bundling preserves the original file, including animation data.")
                            .weak()
                            .size(10.0),
                    );
                    ui.add_enabled_ui(dialog.image_entry.is_some(), |ui| {
                        ui.radio_value(
                            &mut dialog.choice,
                            SaveImageChoice::Bundle,
                            "Bundle image into the .jvp (portable)",
                        );
                        ui.radio_value(
                            &mut dialog.choice,
                            SaveImageChoice::Link,
                            "Link to the image on this drive",
                        );
                    });
                    ui.radio_value(
                        &mut dialog.choice,
                        SaveImageChoice::PresetOnly,
                        "Save options only (no image)",
                    );
                    if dialog.image_entry.is_none() {
                        ui.label(egui::RichText::new("No current image entry is available; only options can be saved.").weak().size(10.0));
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save as…").clicked() {
                            save_requested = true;
                            close_dialog = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                    });
                });
            if close_dialog {
                keep_open = false;
            }
            if save_requested {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JPEG Viewer Pop-out preset", &["jvp"])
                    .set_file_name("Pop-out.jvp")
                    .save_file()
                {
                    let path = ensure_jvp_extension(path);
                    match jvp::save_jvp(
                        &path,
                        &dialog.preset,
                        dialog.image_entry.as_ref(),
                        dialog.choice,
                    ) {
                        Ok(()) => {
                            self.status_message = Some(format!("Saved preset: {}", path.display()));
                        }
                        Err(error) => {
                            self.status_message = Some(format!("Could not save .jvp: {error}"));
                        }
                    }
                }
            } else if keep_open {
                self.save_dialog = Some(dialog);
            }
        }

        if let Some(dialog) = self.load_dialog.take() {
            let mut keep_open = true;
            let mut close_dialog = false;
            let mut load_image = false;
            let mut options_only = false;
            egui::Window::new("Load Pop-out Preset (.jvp)")
                .id(egui::Id::new(("popout_jvp_load_dialog", dialog.tab_id)))
                .collapsible(false)
                .resizable(false)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    if dialog.document.has_image() {
                        ui.label("This preset contains an image entry.");
                        ui.label("Do you also want to load that image into an Image tab?");
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Load image + options").clicked() {
                                load_image = true;
                                close_dialog = true;
                            }
                            if ui.button("Options only").clicked() {
                                options_only = true;
                                close_dialog = true;
                            }
                        });
                    } else {
                        ui.label("This preset contains options only.");
                        if ui.button("Apply options").clicked() {
                            options_only = true;
                            close_dialog = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            if close_dialog {
                keep_open = false;
            }

            if load_image {
                let preset = dialog.document.preset();
                match dialog.document.resolve_image() {
                    Ok(Some(entry)) => {
                        self.pending_image_open = Some((entry, preset));
                        self.status_message = Some("Opening the image from the preset…".into());
                    }
                    Ok(None) => {
                        self.assign_preset(dialog.tab_id, preset, ctx);
                        self.status_message = Some("Loaded pop-out options.".into());
                    }
                    Err(error) => {
                        self.status_message = Some(format!("Could not load the preset image: {error}"));
                    }
                }
            } else if options_only {
                self.assign_preset(dialog.tab_id, dialog.document.preset(), ctx);
                self.status_message = Some("Loaded pop-out options without changing the image.".into());
            } else if keep_open {
                self.load_dialog = Some(dialog);
            }
        }
    }

    pub(crate) fn sync_active_animation(&mut self, tab_id: u64, source: Option<&GifAnimation>) {
        let Some(window_state) = self.windows.get(&tab_id) else {
            return;
        };
        let Ok(mut window) = window_state.lock() else {
            return;
        };
        let Some(source) = source else {
            window.animation = None;
            window.last_frame_index = None;
            return;
        };

        let replace = window
            .animation
            .as_ref()
            .is_none_or(|current| current.frame_count() != source.frame_count());
        if replace {
            let mut animation = source.clone();
            animation.is_playing = true;
            animation.last_update = Instant::now();
            window.animation = Some(animation);
            window.last_frame_index = None;
        } else if let Some(animation) = window.animation.as_mut()
            && (animation.speed_multiplier - source.speed_multiplier).abs() > f32::EPSILON
        {
            animation.set_speed(source.speed_multiplier);
        }
    }

    /// Register deferred pop-out viewports. Their callbacks continue rendering
    /// and animating while the root application is minimized/occluded, unlike
    /// immediate viewports whose lifetime depended on the root update pass.
    pub(crate) fn render_viewports(&mut self, ctx: &egui::Context) {
        for viewport_id in self.pending_close.drain(..) {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
        }

        // A close button in a deferred child cannot directly borrow this manager.
        // It sets a flag in the shared window state; harvest those flags next root pass.
        let closed = self
            .windows
            .iter()
            .filter_map(|(tab_id, state)| {
                state
                    .lock()
                    .ok()
                    .filter(|window| window.close_requested)
                    .map(|window| (*tab_id, preset_from_window(&window)))
            })
            .collect::<Vec<_>>();
        for (tab_id, preset) in closed {
            self.windows.remove(&tab_id);
            self.presets.insert(tab_id, preset);
        }

        let states = self
            .windows
            .iter()
            .map(|(tab_id, state)| (*tab_id, Arc::clone(state)))
            .collect::<Vec<_>>();
        let (parent_focused, root_pointer) = ctx.input(|input| {
            let pointer = input
                .pointer
                .hover_pos()
                .zip(input.viewport().outer_rect.map(|rect| rect.min))
                .map(|(pointer, origin)| pointer + origin.to_vec2());
            (input.viewport().focused.unwrap_or(false), pointer)
        });
        let wants_pointer_position = states.iter().any(|(_, state)| {
            state.lock().is_ok_and(|window| {
                window.click_through && (window.pass_near_enabled || window.pass_hover_enabled)
            })
        });
        let pointer_screen_pos = if wants_pointer_position {
            crate::platform::pointer::global_position(ctx).or(root_pointer)
        } else {
            root_pointer
        };
        for (tab_id, window_state) in states {
            let (viewport_id, builder) = match window_state.lock() {
                Ok(mut window) => {
                    // Keep all pop-outs' Pass opacity triggers synchronized,
                    // even while their owning Image tab is not the active tab.
                    window.pointer_screen_pos = pointer_screen_pos;
                    (
                        window.viewport_id,
                        window.viewport_builder(tab_id, parent_focused),
                    )
                }
                Err(_) => continue,
            };
            // An AlwaysOnTop pop-out otherwise remains above the main window
            // even after the user activates the main app. Yield topmost status
            // while the parent is focused, then restore it when focus leaves.
            if let Ok(mut window) = window_state.lock() {
                let level = if window.stay_on_top && !parent_focused {
                    egui::WindowLevel::AlwaysOnTop
                } else if window.stay_on_bottom {
                    egui::WindowLevel::AlwaysOnBottom
                } else {
                    egui::WindowLevel::Normal
                };
                if window.applied_window_level != Some(level) {
                    ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::WindowLevel(level));
                    window.applied_window_level = Some(level);
                }
            }
            let callback_state = Arc::clone(&window_state);
            let parent_ctx = ctx.clone();
            ctx.show_viewport_deferred(viewport_id, builder, move |viewport_ctx, _class| {
                let Ok(mut window) = callback_state.lock() else {
                    return;
                };
                if window.close_requested {
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                #[cfg(target_os = "windows")]
                if !window.dwm_transparency_applied {
                    window.dwm_transparency_applied =
                        enable_popout_dwm_transparency(tab_id, window.viewport_generation);
                }
                if !window.native_owner_detached {
                    window.native_owner_detached =
                        detach_popout_owner(tab_id, window.viewport_generation);
                }
                if window.stay_on_bottom {
                    viewport_ctx.request_repaint_after(Duration::from_millis(350));
                    if window.last_bottom_enforcement.elapsed() >= Duration::from_millis(350) {
                        enforce_bottom_z_order(tab_id, window.viewport_generation);
                        window.last_bottom_enforcement = Instant::now();
                    }
                }
                let animated = window.ball_mode || window.dvd_mode;
                let should_close = egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(viewport_ctx, |ui| window.render(ui, tab_id))
                    .inner;
                // The child has its own repaint loop, but a minimized parent can
                // otherwise remain asleep after the compositor restores it. Keep
                // a low-rate wake scheduled on the root context while physics is
                // running; the root's minimized fast path remains lightweight.
                if animated {
                    parent_ctx.request_repaint_after(Duration::from_millis(250));
                }
                if should_close {
                    window.close_requested = true;
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    parent_ctx.request_repaint();
                }
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn enable_popout_dwm_transparency(tab_id: u64, generation: u64) -> bool {
    use windows::core::{BOOL, PCWSTR};
    use windows::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DWM_BLURBEHIND, DWM_BB_BLURREGION, DWM_BB_ENABLE,
    };
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    // eframe 0.33's glow backend can clear the transparent attribute while
    // finalizing child viewports on Windows. Register this top-level HWND with
    // DWM's alpha-compositing path after it has been created and shown. An empty
    // blur region enables per-pixel alpha without adding the dark blur effect.
    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };
        let region = CreateRectRgn(0, 0, -1, -1);
        if region.is_invalid() {
            return false;
        }
        let blur_behind = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: BOOL(1),
            hRgnBlur: region,
            fTransitionOnMaximized: BOOL(0),
        };
        let applied = DwmEnableBlurBehindWindow(hwnd, &blur_behind).is_ok();
        let _ = DeleteObject(region.into());
        applied
    }
}

#[cfg(target_os = "windows")]
fn detach_popout_owner(tab_id: u64, generation: u64) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GW_OWNER, GWLP_HWNDPARENT, GetWindow, SetWindowLongPtrW,
    };
    use windows::core::PCWSTR;

    // Find by our unique tab-scoped title. Deferred viewports are created as
    // top-level owned windows; Win32 hides owned windows when their owner is
    // minimized. Clearing GWLP_HWNDPARENT makes the pop-out an independent
    // top-level window while keeping it in the same process and UI lifecycle.
    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };
        let owner_is_detached = || GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| owner.0.is_null());
        if owner_is_detached() {
            return true;
        }
        let _previous_owner = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
        owner_is_detached()
    }
}

#[cfg(not(target_os = "windows"))]
fn detach_popout_owner(_tab_id: u64, _generation: u64) -> bool {
    // The native parent-owner minimize behavior addressed above is Windows
    // specific. Deferred viewports already repaint independently elsewhere.
    true
}

#[cfg(target_os = "windows")]
fn enforce_bottom_z_order(tab_id: u64, generation: u64) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };
    use windows::core::PCWSTR;

    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id} · Instance {generation}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn enforce_bottom_z_order(_tab_id: u64, _generation: u64) {
    // Other window managers receive egui's WindowLevel::AlwaysOnBottom hint.
}

#[cfg(test)]
mod tests {
    use super::{apply_opacity_to_rgba, fit_aspect_size, opacity_to_alpha, PopoutWindow};
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
    fn window_fit_keeps_the_source_aspect_ratio() {
        let size = fit_aspect_size(16.0 / 9.0, eframe::egui::vec2(560.0, 420.0));
        assert!((size.x / size.y - 16.0 / 9.0).abs() < 0.001);
        assert!(size.x <= 560.0 && size.y <= 420.0);
    }
}
