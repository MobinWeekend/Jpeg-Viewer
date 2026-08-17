use super::types::ViewerApp;
use crate::shortcuts::{handle_keyboard, handle_mouse};
use eframe::egui;
use std::path::PathBuf;

impl ViewerApp {
    // Handle all hardcoded input events (keyboard, mouse, drag & drop)

    // Handle mouse input on the image response.
    pub fn handle_image_mouse_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        if !response.hovered() {
            return;
        }

        let (alt, ctrl) = ctx.input(|i| (i.modifiers.alt, i.modifiers.ctrl));

        // CTRL + LEFT DRAG -> RESIZE WINDOW
        // BeginResize expects the LEFT mouse button to be the
        // button that initiated the resize.
        if ctrl && response.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                egui::viewport::ResizeDirection::SouthEast,
            ));

            ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            return;
        }

        // ALT + LEFT DRAG -> MOVE WINDOW
        if alt && response.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            return;
        }

        // Alt/Ctrl are reserved for window operations.
        // ctrl + wheel bypass
        if alt || ctrl {
            let modified_wheel = ctx.input(|i| {
                i.events
                    .iter()
                    .any(|event| matches!(event, egui::Event::MouseWheel { .. }))
            });

            if !modified_wheel {
                return;
            }
        }

        // NORMAL MOUSE BINDINGS
        for command in handle_mouse(
            ctx,
            &self.input_bindings,
            response.hovered(),
            ctx.is_pointer_over_area(),
            self.b_ctrl_invert,
        ) {
            self.handle_command(ctx, command);
        }

        let (left_down, right_down, delta) = ctx.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.delta(),
            )
        });

        // RIGHT DRAG -> ZOOM
        if right_down && response.dragged_by(egui::PointerButton::Secondary) {
            self.zoom = (self.zoom - delta.y * 0.005).clamp(0.005, 50.0);
            self.b_zoom_used = true;

            ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
            return;
        }

        // LEFT DRAG -> PAN
        if left_down && response.dragged_by(egui::PointerButton::Primary) {
            self.pan += delta / self.zoom;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    pub fn handle_input(&mut self, ctx: &egui::Context) {
        // Mark interaction if any input occurs
        let has_input = ctx.input(|i| {
            i.pointer.any_down()
                || i.pointer.delta().length() > 0.0
                || !i.keys_down.is_empty()
                || i.raw_scroll_delta != egui::Vec2::ZERO
        });

        if has_input {
            self.mark_interaction();

            if !self.overlay_visible {
                self.overlay_visible = true;
                ctx.request_repaint();
            }
        }

        self.update_overlay_visibility(ctx);

        // Handle Escape key for fullscreen toggle / close
        self.handle_escape_key(ctx);

        // Standard keyboard shortcuts
        for command in handle_keyboard(ctx, &self.input_bindings) {
            self.handle_command(ctx, command);
        }

        // Delete key handling
        self.handle_delete_key(ctx);

        // Drag & drop
        self.handle_drag_drop(ctx);
    }

    /// Handle Escape key: toggle fullscreen or close window
    fn handle_escape_key(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return;
        }

        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

        // Check if we started in fullscreen mode
        let start_fullscreen = self.settings_manager.get().start_fullscreen;

        if fullscreen {
            // If we started in fullscreen mode, close the app on Escape
            if start_fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
        } else {
            self.save_window_state(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Handle Delete key - triggers on key release
    fn handle_delete_key(&mut self, ctx: &egui::Context) {
        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete));
        let delete_released = ctx.input(|i| i.key_released(egui::Key::Delete));

        if delete_pressed {
            self.delete_key_was_pressed = true;
        } else if delete_released && self.delete_key_was_pressed {
            self.delete_current_image();
            self.delete_key_was_pressed = false;
        }
    }

    /// Handle drag and drop of files
    fn handle_drag_drop(&mut self, ctx: &egui::Context) {
        // If a file/folder dialog is open, ignore any drop events
        if self.dialog_open {
            return;
        }
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());

        if dropped_files.is_empty() {
            return;
        }

        // Stop slideshow when dropping files
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            let _ = self.settings_manager.update(|settings| {
                settings.slideshow_enabled = false;
            });
            self.update_window_title(ctx);
        }

        let paths: Vec<PathBuf> = dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect();

        if paths.len() == 1 {
            // Single file: open it
            if let Some(path) = paths.first() {
                self.open_path(path.clone());
            }
        } else if paths.len() > 1 {
            // Multiple files: load them as a set
            self.load_dropped_files(paths);
        }
    }

    /// Check for window resize and update fit-to-window accordingly
    pub fn handle_window_resize(&mut self, ctx: &egui::Context) {
        let current_size = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size());

        if let (Some(prev), Some(curr)) = (self.last_window_size, current_size) {
            if prev != curr && !self.b_zoom_used {
                self.b_fit_to_window = true;
            }
        }

        self.last_window_size = current_size;
    }
}
