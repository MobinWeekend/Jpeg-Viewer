use eframe::egui;
use std::time::{Duration, Instant};

use super::types::ViewerApp;
use crate::app::constants::OVERLAY_HIDE_DELAY;

impl ViewerApp {
    // ─── Scheduled repaints ──────────────────────────────────────

    fn schedule_timed_repaints(&self, ctx: &egui::Context) {
        self.handle_slideshow_repaint(ctx);
        self.handle_overlay_repaint(ctx);
    }

    fn handle_slideshow_repaint(&self, ctx: &egui::Context) {
        if !self.slideshow_enabled || self.image_entries.is_empty() || self.is_loading() {
            return;
        }
        let elapsed = self.slideshow_last_advance.elapsed();
        let remaining = self.slideshow_interval.saturating_sub(elapsed);
        ctx.request_repaint_after(remaining);
    }

    fn handle_overlay_repaint(&self, ctx: &egui::Context) {
        if !self.overlay_visible || self.hamburger_menu_open || self.image_entries.is_empty() {
            return;
        }
        let elapsed = self.last_interaction_time.elapsed();
        if elapsed < OVERLAY_HIDE_DELAY {
            ctx.request_repaint_after(OVERLAY_HIDE_DELAY - elapsed);
        }
    }

    // ─── FPS limiter core ────────────────────────────────────────

    fn apply_fps_limit(&mut self, fps: f32) -> bool {
        if !fps.is_finite() || fps <= 0.0 {
            return true;
        }
        let now = Instant::now();
        let frame_time = Duration::from_secs_f32(1.0 / fps);
        if now.duration_since(self.last_repaint_time) >= frame_time {
            self.last_repaint_time = now;
            true
        } else {
            false
        }
    }

    fn apply_max_fps_limit(&mut self) -> bool {
        self.apply_fps_limit(self.max_fps)
    }

    fn apply_idle_fps_limit(&mut self) -> bool {
        self.apply_fps_limit(self.idle_fps_limit)
    }

    fn apply_unfocused_idle_fps_limit(&mut self) -> bool {
        if !self.unfocused_idle_fps_limit.is_finite() || self.unfocused_idle_fps_limit <= 0.0 {
            self.apply_idle_fps_limit()
        } else {
            self.apply_fps_limit(self.unfocused_idle_fps_limit)
        }
    }

    // ─── Main repaint decision ────────────────────────────────────

    pub fn should_request_repaint(&mut self, ctx: &egui::Context) -> bool {
        self.schedule_timed_repaints(ctx);

        // 1. Animated GIF → max FPS
        if self.is_animating {
            return self.apply_max_fps_limit();
        }

        // 3. Capture input state
        let (has_input, has_key_down, has_focus) = self.get_input_state(ctx);

        // 4. Interaction → max FPS
        if self.handle_interaction(has_input, has_key_down, has_focus) {
            return self.apply_max_fps_limit();
        }

        // 5. Update idle state
        self.update_idle_state(has_focus);

        // 6. Idle → appropriate idle FPS
        if self.is_idle {
            self.apply_idle_limit_based_on_focus(has_focus)
        } else {
            self.apply_max_fps_limit()
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────

    fn get_input_state(&self, ctx: &egui::Context) -> (bool, bool, bool) {
        ctx.input(|i| {
            let has_focus = i.viewport().focused.unwrap_or(false);
            let has_key_down = !i.keys_down.is_empty();

            // Pointer input only counts while our window is focused.
            // Mouse movement outside the window cannot wake the FPS limiter.
            let has_pointer_input = has_focus
                && (i.pointer.any_down()
                    || i.pointer.delta() != egui::Vec2::ZERO
                    || i.raw_scroll_delta != egui::Vec2::ZERO);

            (has_pointer_input, has_key_down, has_focus)
        })
    }

    fn handle_interaction(
        &mut self,
        has_pointer_input: bool,
        has_key_down: bool,
        has_focus: bool,
    ) -> bool {
        // Pointer interaction only counts when the window is focused.
        // Keyboard input can still wake the viewer even when unfocused.
        let is_pointer_interaction = has_focus && has_pointer_input;
        let is_keyboard_interaction = has_key_down;

        if is_pointer_interaction || is_keyboard_interaction {
            self.last_interaction_time = Instant::now();
            self.is_idle = false;
            true
        } else {
            false
        }
    }

    fn update_idle_state(&mut self, has_focus: bool) {
        let now = Instant::now();

        let timeout = if has_focus {
            Duration::from_millis(self.idle_timeout_ms)
        } else {
            Duration::from_millis(self.unfocused_idle_timeout_ms)
        };

        self.is_idle = now.duration_since(self.last_interaction_time) >= timeout;
    }

    fn apply_idle_limit_based_on_focus(&mut self, has_focus: bool) -> bool {
        if has_focus {
            self.apply_idle_fps_limit()
        } else {
            self.apply_unfocused_idle_fps_limit()
        }
    }

    // ─── Public API ──────────────────────────────────────────────

    pub fn mark_interaction(&mut self) {
        self.last_interaction_time = Instant::now();
        self.is_idle = false;
    }

    pub fn set_animating(&mut self, animating: bool) {
        if animating != self.is_animating {
            self.is_animating = animating;
            if animating {
                self.last_interaction_time = Instant::now();
                self.is_idle = false;
            }
        }
    }

    pub fn load_frame_limiter_settings(&mut self) {
        let settings = self.settings_manager.get();
        self.max_fps = settings.max_fps;
        self.idle_fps_limit = settings.idle_fps_limit;
        self.unfocused_idle_fps_limit = settings.unfocused_idle_fps_limit;
        self.idle_timeout_ms = settings.idle_timeout_ms;
        self.unfocused_idle_timeout_ms = settings.unfocused_idle_timeout_ms;
    }
}
