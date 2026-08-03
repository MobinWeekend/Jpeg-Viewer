use eframe::egui;
use std::time::{Duration, Instant};

use super::types::ViewerApp;

impl ViewerApp {
    /// Check if we should request a repaint based on idle state and FPS limit
    pub fn should_request_repaint(&mut self, ctx: &egui::Context) -> bool {
        // Always repaint if we have an animated GIF playing
        if self.is_animating {
            return true;
        }

        // Always repaint if we're loading
        if self.b_is_loading || self.b_is_loading_full {
            return true;
        }

        // Always repaint if we have completed preload tasks to process
        if !self.preload_tasks.is_empty() {
            for task in &self.preload_tasks {
                if let Ok(_) = task.receiver.try_recv() {
                    return true;
                }
            }
        }

        // Check if window has focus
        let has_focus = ctx.input(|i| i.viewport().focused).unwrap_or(false);

        // Check for any input activity
        let has_input = ctx.input(|i| {
            i.pointer.any_down()
                || i.pointer.delta().length() > 0.0
                || !i.keys_down.is_empty()
                || i.raw_scroll_delta != egui::Vec2::ZERO
        });

        // If window is in focus and has input, mark interaction and return full FPS
        if has_focus && has_input {
            self.last_interaction_time = Instant::now();
            self.is_idle = false;
            return self.apply_max_fps_limit();
        }

        // If window is not in focus but has input (key press), treat as interaction
        if !has_focus && has_input {
            let has_key_press = ctx.input(|i| !i.keys_down.is_empty());
            if has_key_press {
                self.last_interaction_time = Instant::now();
                self.is_idle = false;
                return self.apply_max_fps_limit();
            }
        }

        // Determine idle timeout based on focus state
        let now = Instant::now();
        let idle_duration = if has_focus {
            Duration::from_millis(self.idle_timeout_ms)
        } else {
            Duration::from_millis(self.unfocused_idle_timeout_ms)
        };

        // Check if we've been idle long enough
        if now.duration_since(self.last_interaction_time) >= idle_duration {
            self.is_idle = true;
        } else {
            self.is_idle = false;
        }

        // When not idle, use max FPS limit
        if !self.is_idle {
            return self.apply_max_fps_limit();
        }

        // Idle mode - apply appropriate FPS limit based on focus state
        if has_focus {
            self.apply_idle_fps_limit()
        } else {
            self.apply_unfocused_idle_fps_limit()
        }
    }

    /// Apply maximum FPS limit (if set)
    fn apply_max_fps_limit(&mut self) -> bool {
        if self.max_fps <= 0.0 {
            // Unlimited FPS - always repaint
            return true;
        }

        // Calculate if enough time has passed since last repaint
        let now = Instant::now();
        let frame_time = Duration::from_secs_f32(1.0 / self.max_fps);
        let elapsed = now.duration_since(self.last_repaint_time);

        if elapsed >= frame_time {
            self.last_repaint_time = now;
            true
        } else {
            false
        }
    }

    /// Apply idle FPS limit (when focused)
    fn apply_idle_fps_limit(&mut self) -> bool {
        if self.idle_fps_limit <= 0.0 {
            // Unlimited idle FPS - use max_fps if set, otherwise unlimited
            return self.apply_max_fps_limit();
        }

        // Calculate if enough time has passed since last repaint
        let now = Instant::now();
        let frame_time = Duration::from_secs_f32(1.0 / self.idle_fps_limit);
        let elapsed = now.duration_since(self.last_repaint_time);

        if elapsed >= frame_time {
            self.last_repaint_time = now;
            true
        } else {
            false
        }
    }

    /// Apply unfocused idle FPS limit (when unfocused)
    fn apply_unfocused_idle_fps_limit(&mut self) -> bool {
        if self.unfocused_idle_fps_limit <= 0.0 {
            // If unfocused idle FPS is 0, use the regular idle FPS limit
            return self.apply_idle_fps_limit();
        }

        // Calculate if enough time has passed since last repaint
        let now = Instant::now();
        let frame_time = Duration::from_secs_f32(1.0 / self.unfocused_idle_fps_limit);
        let elapsed = now.duration_since(self.last_repaint_time);

        if elapsed >= frame_time {
            self.last_repaint_time = now;
            true
        } else {
            false
        }
    }

    /// Mark an interaction that should keep the app responsive
    pub fn mark_interaction(&mut self) {
        self.last_interaction_time = Instant::now();
        self.is_idle = false;
    }

    /// Set whether the current content is animated (GIF playing)
    pub fn set_animating(&mut self, animating: bool) {
        if animating != self.is_animating {
            self.is_animating = animating;
            if animating {
                self.last_interaction_time = Instant::now();
                self.is_idle = false;
            }
        }
    }

    /// Load settings for frame limiter
    pub fn load_frame_limiter_settings(&mut self) {
        let settings = self.settings_manager.get();
        self.max_fps = settings.max_fps;
        self.idle_fps_limit = settings.idle_fps_limit;
        self.unfocused_idle_fps_limit = settings.unfocused_idle_fps_limit;
        self.idle_timeout_ms = settings.idle_timeout_ms;
        self.unfocused_idle_timeout_ms = settings.unfocused_idle_timeout_ms;
    }
}
