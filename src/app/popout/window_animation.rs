use rand::RngExt;

use super::*;

impl PopoutWindow {
    pub(super) fn update_animation_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
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

    pub(super) fn update_static_opacity_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
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

    pub(super) fn current_motion_bounds(&self) -> Option<egui::Rect> {
        let monitors = crate::platform::monitors::available_monitors();
        // `None` means Active monitor: resolve from the pop-out's current
        // desktop rectangle every frame, so DVD/Ball bounds follow manual
        // window drags across displays. `Some(id)` is an explicit pinned display.
        let monitor = if let Some(id) = self.monitor_id.as_deref() {
            monitors
                .iter()
                .find(|monitor| monitor.id == id)
                .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
                .or_else(|| monitors.first())?
        } else {
            let window_rect = egui::Rect::from_min_size(self.position, self.size);
            crate::platform::monitors::resolve_default_monitor_at(
                &monitors,
                crate::platform::monitors::CURRENT_WINDOW_MONITOR,
                Some(window_rect),
            )?
        };
        let monitor_bounds = monitor.bounds();
        let max_horizontal_inset = (monitor_bounds.width() - 1.0).max(0.0) * 0.5;
        let max_vertical_inset = (monitor_bounds.height() - 1.0).max(0.0) * 0.5;
        let left =
            monitor_bounds.left() + self.dvd_edge_offsets[0].clamp(0.0, max_horizontal_inset);
        let top = monitor_bounds.top() + self.dvd_edge_offsets[1].clamp(0.0, max_vertical_inset);
        let right =
            monitor_bounds.right() - self.dvd_edge_offsets[2].clamp(0.0, max_horizontal_inset);
        let bottom =
            monitor_bounds.bottom() - self.dvd_edge_offsets[3].clamp(0.0, max_vertical_inset);
        Some(egui::Rect::from_min_max(
            egui::pos2(left, top),
            egui::pos2(right.max(left), bottom.max(top)),
        ))
    }

    pub(super) fn collision_position_limits(
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

    pub(super) fn random_direction() -> egui::Vec2 {
        let angle = rand::rng().random_range(0.0..std::f32::consts::TAU);
        egui::vec2(angle.cos(), angle.sin())
    }

    pub(super) fn randomize_rotation(&mut self) {
        let rotation = rand::rng().random_range(0.0..std::f32::consts::TAU);
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
    pub(super) fn resolve_ball_contact(&mut self, normal: egui::Vec2) {
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
        let tangential_speed =
            self.ball_velocity.dot(tangent) - self.ball_angular_velocity * radius;
        let inverse_effective_mass = 1.0 + radius * radius / inertia;
        let max_friction_impulse = self.ball_friction.max(0.0) * normal_impulse;
        let tangent_impulse = (-tangential_speed / inverse_effective_mass)
            .clamp(-max_friction_impulse, max_friction_impulse);

        // Unit linear mass; angular impulse is r × J.
        self.ball_velocity += tangent * tangent_impulse;
        self.ball_angular_velocity -= radius * tangent_impulse / inertia;
    }

    pub(super) fn rolling_angular_velocity(&self) -> f32 {
        let radius = (self.size.x.max(self.size.y) * 0.5).max(1.0);
        self.ball_velocity.x / radius
    }

    pub(super) fn finish_ball_throw(&mut self) {
        self.ball_throw_dragging = false;
        let throw_velocity = clamp_vector_length(self.ball_throw_velocity, 2800.0);
        self.ball_velocity = if throw_velocity.length() >= 80.0 {
            throw_velocity
        } else {
            egui::Vec2::ZERO
        };
        self.ball_angular_velocity = if self.ball_rotate {
            self.rolling_angular_velocity()
        } else {
            0.0
        };
        self.ball_throw_velocity = egui::Vec2::ZERO;
        self.ball_time_accumulator = 0.0;
        self.last_dvd_tick = Instant::now();
    }

    pub(super) fn update_dvd_rotation(&mut self, delta: f32) {
        let angle_delta = (self.dvd_rotation_target - self.rotation_radians + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let time_constant = self.dvd_rotation_ease.clamp(0.1, 4.0);
        let amount = 1.0 - (-6.0 * delta / time_constant).exp();
        self.rotation_radians =
            (self.rotation_radians + angle_delta * amount).rem_euclid(std::f32::consts::TAU);
        if angle_delta.abs() < 0.001 {
            self.rotation_radians = self.dvd_rotation_target;
        }
    }

    pub(super) fn update_pass_opacity(&mut self, ctx: &egui::Context) {
        let mut eased_target = None;
        if self.click_through {
            if let Some(pointer) = self.pointer_screen_pos {
                let rect = egui::Rect::from_min_size(self.position, self.size);
                if self.pass_hover_enabled && rect.contains(pointer) {
                    eased_target = Some(self.pass_hover_opacity);
                } else if self.pass_near_enabled {
                    let dx = (rect.left() - pointer.x)
                        .max(0.0)
                        .max(pointer.x - rect.right());
                    let dy = (rect.top() - pointer.y)
                        .max(0.0)
                        .max(pointer.y - rect.bottom());
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
        self.displayed_opacity_percent += (target - self.displayed_opacity_percent) * amount;
        if (target - self.displayed_opacity_percent).abs() > 0.15 {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            self.displayed_opacity_percent = target;
        }
    }

    pub(super) fn update_dvd_position(&mut self, ctx: &egui::Context) {
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
        if self.dvd_dragging {
            self.last_dvd_tick = Instant::now();
            ctx.request_repaint_after(Duration::from_millis(16));
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
            self.next_random_bounce =
                now + Duration::from_secs_f32(self.random_bounce_interval.clamp(0.5, 60.0));
        }

        self.update_dvd_rotation(delta);
        self.request_outer_position(ctx, position);
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    pub(super) fn update_ball_position(&mut self, ctx: &egui::Context) {
        let Some(bounds) = self.current_motion_bounds() else {
            return;
        };
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_dvd_tick)
            .as_secs_f32()
            .clamp(0.0, 0.1);
        self.last_dvd_tick = now;
        if self.ball_dragging || self.ball_throw_dragging || self.resize_gesture.is_some() {
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
        let (min_position, max_position) = Self::collision_position_limits(bounds, self.size, true);

        if self.random_bounce_enabled && now >= self.next_random_bounce {
            self.ball_velocity += Self::random_direction() * 650.0;
            let speed = self.ball_velocity.length();
            if speed > 2800.0 {
                self.ball_velocity *= 2800.0 / speed;
            }
            if self.ball_rotate {
                self.ball_angular_velocity = self.rolling_angular_velocity();
            }
            self.next_random_bounce =
                now + Duration::from_secs_f32(self.random_bounce_interval.clamp(0.5, 60.0));
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
}
