use super::*;

impl PopoutManager {
    pub(super) fn mark_lifecycle_repaint(&mut self) {
        let until = Instant::now() + Duration::from_secs(1);
        self.lifecycle_repaint_until = Some(until);
        for state in self.windows.values() {
            if let Ok(mut window) = state.lock() {
                window.lifecycle_repaint_until = Some(until);
            }
        }
    }

    pub(crate) fn has_active_popouts(&self) -> bool {
        !self.windows.is_empty()
    }

    pub(crate) fn has_tab_popout(&self, tab_id: u64) -> bool {
        self.windows.contains_key(&tab_id)
    }

    pub(crate) fn settings_sidebar_open(&self, tab_id: u64) -> bool {
        self.settings_sidebar_tab == Some(tab_id)
    }

    pub(crate) fn close_settings_sidebar(&mut self) {
        self.settings_sidebar_tab = None;
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

    pub(crate) fn assign_preset(
        &mut self,
        tab_id: u64,
        mut preset: PopoutPreset,
        ctx: &egui::Context,
    ) {
        preset.sanitize();
        self.presets.insert(tab_id, preset.clone());
        if let Some(state) = self.windows.get(&tab_id)
            && let Ok(mut window) = state.lock()
        {
            apply_preset_to_window(&preset, &mut window, Some(ctx));
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
            self.mark_lifecycle_repaint();
        }
    }

    pub(super) fn open_tab(
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
        // Start on the monitor containing the main window, but leave the
        // preference unpinned so the pop-out follows whichever monitor it is
        // moved to later.
        let monitor_id = None;
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
            remove_os_decorations: true,
            remove_os_window_effects: true,
            lifecycle_repaint_until: None,
            native_window_effects_applied: None,
            dvd_mode: false,
            dvd_dragging: false,
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
            ball_throw_dragging: false,
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
            apply_preset_to_window(preset, &mut window, None);
        }
        self.windows.insert(tab_id, Arc::new(Mutex::new(window)));
        self.mark_lifecycle_repaint();
        ctx.request_repaint();
    }
}
