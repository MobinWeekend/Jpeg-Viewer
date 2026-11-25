use super::*;

impl PopoutWindow {
    pub(super) fn automatic_window_movement_supported() -> bool {
        #[cfg(target_os = "linux")]
        {
            !crate::platform::wayland_pointer_recovery::is_running_on_wayland()
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    pub(super) fn minimum_size(&self) -> egui::Vec2 {
        // Keep a feasible minimum even for extremely panoramic/portrait sources:
        // aspect ratio is more important than imposing independent min width/height.
        let max_width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        let width = MIN_SIZE.x.min(max_width);
        egui::vec2(width, width / self.aspect_ratio)
    }

    pub(super) fn maximum_size(&self) -> egui::Vec2 {
        let width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        egui::vec2(width, width / self.aspect_ratio)
    }

    pub(super) fn size_for_width(&self, width: f32) -> egui::Vec2 {
        let min = self.minimum_size();
        let max = self.maximum_size();
        let width = width.clamp(min.x, max.x);
        egui::vec2(width, width / self.aspect_ratio)
    }

    pub(super) fn size_for_scale(&self, scale: f32) -> egui::Vec2 {
        self.size_for_width(self.base_size.x * scale)
    }

    pub(super) fn viewport_builder(&mut self, tab_id: u64, parent_focused: bool) -> egui::ViewportBuilder {
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
            .with_has_shadow(!self.remove_os_window_effects)
            .with_decorations(!self.remove_os_decorations)
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

    pub(super) fn request_inner_size(&mut self, ctx: &egui::Context, size: egui::Vec2) {
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

    pub(super) fn request_geometry(&mut self, ctx: &egui::Context, size: egui::Vec2, position: egui::Pos2) {
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

    pub(super) fn request_outer_position(&mut self, ctx: &egui::Context, position: egui::Pos2) {
        if (self.position - position).length_sq() <= 0.25 {
            return;
        }
        self.position = position;
        self.expected_native_position = Some((position, Instant::now()));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
        ctx.request_repaint();
    }

    pub(super) fn synchronize_native_position(&mut self, ctx: &egui::Context) {
        if self.resize_gesture.is_some()
            || (self.dvd_mode && !self.dvd_dragging)
            || self.ball_dragging
        {
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

    pub(super) fn synchronize_native_size(&mut self, ctx: &egui::Context) {
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
}
