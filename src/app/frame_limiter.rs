use eframe::egui;
use std::time::{Duration, Instant};

use super::types::ViewerApp;
use crate::constants::OVERLAY_HIDE_DELAY;

impl ViewerApp {
    // ─── Repaint scheduling ──────────────────────────────────────

    /// Schedule the next repaint according to the current application state.
    ///
    /// This is the main FPS limiter. Instead of allowing eframe to repaint
    /// continuously and then rejecting frames, we tell egui when the next
    /// repaint should happen.
    pub fn schedule_repaint(&mut self, ctx: &egui::Context) {
        let (has_input, has_key_down, has_focus) = self.get_input_state(ctx);

        // User interaction always gets the maximum FPS.
        if self.handle_interaction(has_input, has_key_down, has_focus) {
            self.update_idle_state(has_focus);
            self.request_repaint_at_fps(ctx, self.max_fps);
            return;
        }

        self.update_idle_state(has_focus);

        // Animated GIFs always run at maximum FPS.
        if self.is_animating {
            self.request_repaint_at_fps(ctx, self.max_fps);
            return;
        }

        // Slideshow has its own timing.
        if self.slideshow_enabled && !self.image_entries.is_empty() && !self.is_loading() {
            self.handle_slideshow_repaint(ctx);
            return;
        }

        // Overlay timeout has priority when it is visible.
        if self.overlay_visible && !self.hamburger_menu_open && !self.image_entries.is_empty() {
            if self.handle_overlay_repaint(ctx) {
                return;
            }
        }

        // Normal idle/non-idle FPS.
        if self.is_idle {
            self.request_repaint_at_idle_fps(ctx, has_focus);
        } else {
            self.request_repaint_at_fps(ctx, self.max_fps);
        }
    }

    /// Request another repaint at the requested FPS.
    ///
    /// This controls the periodic repaint rate instead of allowing eframe
    /// to continuously repaint.
    fn request_repaint_at_fps(&self, ctx: &egui::Context, fps: f32) {
        if !fps.is_finite() || fps <= 0.0 {
            return;
        }

        let frame_time = Duration::from_secs_f32(1.0 / fps);

        ctx.request_repaint_after(frame_time);
    }

    fn request_repaint_at_idle_fps(&self, ctx: &egui::Context, has_focus: bool) {
        let fps = if has_focus {
            self.idle_fps_limit
        } else if self.unfocused_idle_fps_limit.is_finite() && self.unfocused_idle_fps_limit > 0.0 {
            self.unfocused_idle_fps_limit
        } else {
            self.idle_fps_limit
        };

        self.request_repaint_at_fps(ctx, fps);
    }

    // ─── Slideshow ────────────────────────────────────────────────

    fn handle_slideshow_repaint(&self, ctx: &egui::Context) {
        let elapsed = self.slideshow_last_advance.elapsed();

        if elapsed >= self.slideshow_interval {
            // The slideshow is due now.
            ctx.request_repaint();
        } else {
            // Wake up exactly when the slideshow should advance.
            let remaining = self.slideshow_interval - elapsed;
            ctx.request_repaint_after(remaining);
        }
    }

    // ─── Overlay ─────────────────────────────────────────────────

    /// Returns `true` if this function scheduled the next repaint.
    fn handle_overlay_repaint(&self, ctx: &egui::Context) -> bool {
        if !self.overlay_visible || self.hamburger_menu_open || self.image_entries.is_empty() {
            return false;
        }

        let elapsed = self.last_interaction_time.elapsed();

        if elapsed >= OVERLAY_HIDE_DELAY {
            // The timeout has been reached.
            ctx.request_repaint();
            true
        } else {
            // Wake up when the overlay timeout is reached.
            ctx.request_repaint_after(OVERLAY_HIDE_DELAY - elapsed);
            true
        }
    }

    // ─── Input state ─────────────────────────────────────────────

    fn get_input_state(&self, ctx: &egui::Context) -> (bool, bool, bool) {
        ctx.input(|i| {
            let has_focus = i.viewport().focused.unwrap_or(false);

            let has_key_down = !i.keys_down.is_empty();

            // Pointer input only counts while our window is focused.
            //
            // Mouse movement outside the window therefore cannot wake the
            // viewer's high-FPS mode.
            let has_pointer_input = has_focus
                && (i.pointer.any_down()
                    || i.pointer.delta() != egui::Vec2::ZERO
                    || i.raw_scroll_delta != egui::Vec2::ZERO);

            (has_pointer_input, has_key_down, has_focus)
        })
    }

    // ─── Interaction ─────────────────────────────────────────────

    fn handle_interaction(
        &mut self,
        has_pointer_input: bool,
        has_key_down: bool,
        has_focus: bool,
    ) -> bool {
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

    // ─── Idle state ──────────────────────────────────────────────

    fn update_idle_state(&mut self, has_focus: bool) {
        let timeout = if has_focus {
            Duration::from_millis(self.idle_timeout_ms)
        } else {
            Duration::from_millis(self.unfocused_idle_timeout_ms)
        };

        self.is_idle = self.last_interaction_time.elapsed() >= timeout;
    }

    // ─── Public interaction API ──────────────────────────────────

    pub fn mark_interaction(&mut self) {
        self.last_interaction_time = Instant::now();
        self.is_idle = false;
    }

    // ─── Animation state ─────────────────────────────────────────

    pub fn set_animating(&mut self, animating: bool) {
        if animating != self.is_animating {
            self.is_animating = animating;

            if animating {
                self.last_interaction_time = Instant::now();
                self.is_idle = false;
            }
        }
    }

    // ─── Settings ────────────────────────────────────────────────

    pub fn load_frame_limiter_settings(&mut self) {
        let settings = self.settings_manager.get();

        self.max_fps = settings.max_fps;
        self.idle_fps_limit = settings.idle_fps_limit;
        self.unfocused_idle_fps_limit = settings.unfocused_idle_fps_limit;
        self.idle_timeout_ms = settings.idle_timeout_ms;
        self.unfocused_idle_timeout_ms = settings.unfocused_idle_timeout_ms;
    }
}
