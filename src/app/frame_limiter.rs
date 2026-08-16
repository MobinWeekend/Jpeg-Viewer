// src/app/frame_limiter.rs

use eframe::egui;
use std::time::{Duration, Instant};

use super::types::ViewerApp;

/// FPS limit applied while an image is loading (capped at 60).
const LOADING_FPS: f32 = 60.0;

impl ViewerApp {
    // ===== Core FPS limiter =====

    /// Returns `true` if a repaint should be requested, based on the given
    /// target FPS.
    ///
    /// # Semantics for `fps`:
    /// - `fps <= 0.0` or `NaN` → unlimited (always returns `true`)
    /// - otherwise, limits to the given frames per second.
    fn apply_fps_limit(&mut self, fps: f32) -> bool {
        if !fps.is_finite() || fps <= 0.0 {
            return true;
        }

        let now = Instant::now();
        let frame_time = Duration::from_secs_f32(1.0 / fps);

        if now.duration_since(self.last_frame_request_time) >= frame_time {
            self.last_frame_request_time = now;
            true
        } else {
            false
        }
    }

    // ===== Wrappers for specific policies =====

    /// User‑configured maximum FPS.
    fn apply_max_fps_limit(&mut self) -> bool {
        self.apply_fps_limit(self.max_fps)
    }

    /// FPS when the window is focused and idle.
    fn apply_idle_fps_limit(&mut self) -> bool {
        self.apply_fps_limit(self.idle_fps_limit)
    }

    /// FPS when the window is unfocused and idle.
    ///
    /// If `unfocused_idle_fps_limit` is not positive or is `NaN`,
    /// it falls back to the regular idle FPS.
    fn apply_unfocused_idle_fps_limit(&mut self) -> bool {
        if !self.unfocused_idle_fps_limit.is_finite()
            || self.unfocused_idle_fps_limit <= 0.0
        {
            self.apply_idle_fps_limit()
        } else {
            self.apply_fps_limit(self.unfocused_idle_fps_limit)
        }
    }

    // ===== Main repaint decision =====

    /// Determines whether a repaint should be requested.
    ///
    /// This is the single entry point for the frame limiter. It evaluates
    /// priorities in order:
    ///
    /// 1. Animated content (GIF) or slideshow → max FPS
    /// 2. Loading → fixed 60 FPS
    /// 3. User interaction → max FPS
    /// 4. Idle → appropriate idle FPS (focused or unfocused)
    pub fn should_request_repaint(&mut self, ctx: &egui::Context) -> bool {
        // 1. Animated or slideshow → max FPS
        if self.is_animating || self.slideshow_enabled {
            return self.apply_max_fps_limit();
        }

        // 2. Loading → fixed 60 FPS (smooth progress)
        if self.is_loading() {
            return self.apply_fps_limit(LOADING_FPS);
        }

        // 3. Capture input state once
        let (has_input, has_key_down, has_focus) = self.get_input_state(ctx);

        // 4. Interaction → wake up and use max FPS
        if self.handle_interaction(has_input, has_key_down, has_focus) {
            return self.apply_max_fps_limit();
        }

        // 5. Update idle state based on timeouts and focus
        self.update_idle_state(has_focus);

        // 6. If idle, apply the appropriate idle FPS; otherwise max FPS
        if self.is_idle {
            self.apply_idle_limit_based_on_focus(has_focus)
        } else {
            self.apply_max_fps_limit()
        }
    }

    // ===== Helpers =====

    /// Queries egui for active input, key‑down state, and focus.
    ///
    /// This bundles three related queries into one closure to avoid
    /// repeated calls to `ctx.input()`.
    fn get_input_state(&self, ctx: &egui::Context) -> (bool, bool, bool) {
        ctx.input(|i| {
            let has_key_down = !i.keys_down.is_empty();

            let has_input = i.pointer.any_down()
                || i.pointer.delta() != egui::Vec2::ZERO
                || has_key_down
                || i.raw_scroll_delta != egui::Vec2::ZERO;

            let has_focus = i.viewport().focused.unwrap_or(false);

            (has_input, has_key_down, has_focus)
        })
    }

    /// Processes interaction and returns `true` if the UI should stay at max FPS.
    ///
    /// Keyboard input is allowed to wake the viewer even when egui reports the
    /// viewport as unfocused – this ensures shortcuts like navigation work
    /// without requiring a mouse click to regain focus.
    fn handle_interaction(
        &mut self,
        has_input: bool,
        has_key_down: bool,
        has_focus: bool,
    ) -> bool {
        if has_input && (has_focus || has_key_down) {
            self.last_interaction_time = Instant::now();
            self.is_idle = false;
            true
        } else {
            false
        }
    }

    /// Updates `self.is_idle` based on the time since the last interaction
    /// and the relevant timeout for the current focus state.
    fn update_idle_state(&mut self, has_focus: bool) {
        let now = Instant::now();
        let timeout = if has_focus {
            Duration::from_millis(self.idle_timeout_ms)
        } else {
            Duration::from_millis(self.unfocused_idle_timeout_ms)
        };

        self.is_idle = now.duration_since(self.last_interaction_time) >= timeout;
    }

    /// Applies the appropriate idle FPS limit (focused or unfocused).
    fn apply_idle_limit_based_on_focus(&mut self, has_focus: bool) -> bool {
        if has_focus {
            self.apply_idle_fps_limit()
        } else {
            self.apply_unfocused_idle_fps_limit()
        }
    }

    // ===== Public API for external state changes =====

    /// Marks an explicit interaction (e.g., from outside input handling).
    pub fn mark_interaction(&mut self) {
        self.last_interaction_time = Instant::now();
        self.is_idle = false;
    }

    /// Sets whether the current content is animated (GIF playing).
    ///
    /// When animation starts, we treat it as interaction to immediately
    /// switch to max FPS.
    pub fn set_animating(&mut self, animating: bool) {
        if animating != self.is_animating {
            self.is_animating = animating;
            if animating {
                self.last_interaction_time = Instant::now();
                self.is_idle = false;
            }
        }
    }

    /// Loads frame‑limiter settings from the settings manager.
    pub fn load_frame_limiter_settings(&mut self) {
        let settings = self.settings_manager.get();
        self.max_fps = settings.max_fps;
        self.idle_fps_limit = settings.idle_fps_limit;
        self.unfocused_idle_fps_limit = settings.unfocused_idle_fps_limit;
        self.idle_timeout_ms = settings.idle_timeout_ms;
        self.unfocused_idle_timeout_ms = settings.unfocused_idle_timeout_ms;
    }
}