use super::types::ViewerApp;
use crate::shortcuts::{handle_keyboard, handle_mouse};
use eframe::egui;
use std::path::PathBuf;

impl ViewerApp {
    // Handle all hardcoded input events (keyboard, mouse, drag & drop)

    // Handle mouse input on the image response.
    pub fn handle_image_mouse_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        image_rect: Option<egui::Rect>,
    ) {
        let (fullscreen, maximized, alt, ctrl, left_down, right_down, delta, hover_pos) =
            ctx.input(|input| {
                (
                    input.viewport().fullscreen.unwrap_or(false),
                    input.viewport().maximized.unwrap_or(false),
                    input.modifiers.alt,
                    input.modifiers.ctrl,
                    input.pointer.button_down(egui::PointerButton::Primary),
                    input.pointer.button_down(egui::PointerButton::Secondary),
                    input.pointer.delta(),
                    input.pointer.hover_pos(),
                )
            });
        let windowed = !fullscreen && !maximized;

        // Select the gesture mode once at drag start. Do not recalculate it while
        // panning, because the image rectangle itself moves as pan is applied.
        if response.drag_stopped_by(egui::PointerButton::Primary) || !left_down {
            self.main_view_pan_drag_active = false;
            self.main_view_window_drag_active = false;
        }

        if !response.hovered() && !left_down && !right_down {
            return;
        }

        // Trackpad pinch gestures: preserve existing zoom behavior.
        let zoom_delta = ctx.input(|i| i.zoom_delta());
        if zoom_delta.is_finite() && (zoom_delta - 1.0).abs() > 0.001 {
            if !self.image_entries.is_empty() {
                let center = response.rect.center();
                let offset = center - response.rect.center();
                let image_point = offset / self.zoom - self.pan;
                self.zoom = (self.zoom * zoom_delta).clamp(0.005, 50.0);
                self.pan = offset / self.zoom - image_point;
                self.b_zoom_used = true;
                self.mark_interaction();
                ctx.request_repaint();
            }
            return;
        }

        // CTRL + LEFT DRAG -> RESIZE WINDOW. Native resize is unavailable while
        // maximized/fullscreen, so do not start a resize gesture in those states.
        if ctrl && windowed && response.drag_started_by(egui::PointerButton::Primary) {
            self.main_view_pan_drag_active = false;
            self.main_view_window_drag_active = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                egui::viewport::ResizeDirection::SouthEast,
            ));
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            return;
        }

        // ALT + LEFT DRAG -> MOVE WINDOW, even when the drag starts on the image.
        if alt && response.drag_started_by(egui::PointerButton::Primary) {
            self.main_view_pan_drag_active = false;
            self.main_view_window_drag_active = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            return;
        }

        // Plain left drag on the visible image pans it. A drag in the margins
        // around the image moves the window instead, but only when windowed.
        if response.drag_started_by(egui::PointerButton::Primary) {
            let drag_origin = ctx.input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            let started_on_image = drag_origin.is_some_and(|position| {
                image_rect.is_some_and(|rect| rect.contains(position))
            });

            if started_on_image && !self.image_entries.is_empty() {
                self.main_view_pan_drag_active = true;
                self.main_view_window_drag_active = false;
            } else if windowed {
                self.main_view_pan_drag_active = false;
                self.main_view_window_drag_active = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                return;
            }
        }

        // Once the OS has taken over the window drag, never also pan the image.
        if self.main_view_window_drag_active {
            if left_down {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            return;
        }

        // Alt/Ctrl are reserved for window operations; Ctrl + wheel remains valid.
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

        if response.drag_started_by(egui::PointerButton::Secondary) {
            self.zoom_drag_start_pos = response.interact_pointer_pos();
            self.zoom_drag_start_zoom = self.zoom;
            self.zoom_drag_start_pan = self.pan;
            self.zoom_drag_start_center = response.rect.center();
        }

        if right_down && response.dragged_by(egui::PointerButton::Secondary) {
            if self.image_entries.is_empty() {
                return;
            }
            let Some(start_pos) = self.zoom_drag_start_pos else {
                return;
            };
            let Some(current_pos) = response.interact_pointer_pos() else {
                return;
            };

            let start_zoom = self.zoom_drag_start_zoom;
            let vertical_delta = current_pos.y - start_pos.y;
            let new_zoom = (start_zoom * (-vertical_delta * 0.005).exp()).clamp(0.005, 50.0);
            let start_center = self.zoom_drag_start_center;
            let mouse_offset = start_pos - start_center;
            let image_point = mouse_offset / start_zoom - self.zoom_drag_start_pan;
            self.zoom = new_zoom;
            self.pan = mouse_offset / new_zoom - image_point;
            self.b_zoom_used = true;
            ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
            return;
        }

        // LEFT DRAG -> PAN, only when the press began on the image itself.
        if self.main_view_pan_drag_active
            && left_down
            && response.dragged_by(egui::PointerButton::Primary)
        {
            self.pan += delta / self.zoom.max(0.001);
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            return;
        }

        // Show the grab cursor over the actual image area, not the surrounding margins.
        if response.hovered()
            && !left_down
            && !right_down
            && hover_pos.is_some_and(|position| image_rect.is_some_and(|rect| rect.contains(position)))
            && !self.image_entries.is_empty()
        {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
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
                ctx.request_repaint(); // make sure the the overlay is shown
            }
        }

        crate::app::ui::update_overlay_visibility(self, ctx);

        // Do not steal keystrokes while a TextEdit (search, rename, numeric input,
        // etc.) owns keyboard focus. egui widgets still receive their normal keys.
        if ctx.wants_keyboard_input() {
            self.delete_key_was_pressed = false;
        } else {
            // Escape first stops playback and consumes the key; it must not also
            // toggle fullscreen or close the application on the same keypress.
            if self.handle_escape_key(ctx) {
                return;
            }

            // F2 renames the current filesystem image; the dialog preserves its extension.
            if ctx.input(|i| i.key_pressed(egui::Key::F2))
                && self.rename_dialog.is_none()
                && self.sidebar_number_edit.is_none()
            {
                self.begin_rename_current();
                if self.rename_dialog.is_some() {
                    let _ = self.settings_manager.update(|settings| settings.show_file_sidebar = true);
                }
            }

            // Standard keyboard shortcuts
            for command in handle_keyboard(ctx, &self.input_bindings) {
                self.handle_command(ctx, command);
            }

            // Delete key handling
            self.handle_delete_key(ctx);
        }

        // Drag & drop
        self.handle_drag_drop(ctx);
    }

    /// Handle Escape key: toggle fullscreen or close window
    fn handle_escape_key(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return false;
        }

        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            self.slideshow_last_advance = std::time::Instant::now();
            self.update_window_title(ctx);
            ctx.request_repaint();
            return true;
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
        true
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

        let drop_position = ctx.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.latest_pos()));
        let dropped_on_sidebar_collection = self
            .queue_drop_rect
            .is_some_and(|rect| drop_position.is_some_and(|position| rect.contains(position)));
        let dropped_on_queue = self.sidebar_queue_tab && dropped_on_sidebar_collection;
        let dropped_on_favorites = self.sidebar_favorites_tab && dropped_on_sidebar_collection;

        // Stop slideshow when dropping files
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            self.update_window_title(ctx);
        }

        let paths: Vec<PathBuf> = dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect();

        // A single .jvq is a queue document, not an image or folder.
        if paths.len() == 1 && paths[0].extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq")) {
            self.sidebar_queue_tab = dropped_on_queue || self.sidebar_queue_tab;
            self.open_queue_file(&paths[0], ctx, dropped_on_queue);
        } else if dropped_on_queue {
            self.add_dropped_paths_to_queue(paths);
        } else if dropped_on_favorites {
            let mut added = 0usize;
            let mut ignored = 0usize;
            for path in paths {
                if path.is_dir() {
                    let before = self.favorite_paths.len();
                    self.add_favorite_path(path);
                    added += usize::from(self.favorite_paths.len() > before);
                } else if path.is_file() && crate::helpers::is_supported_image(&path) {
                    let before = self.favorite_paths.len();
                    self.add_favorite_path(path);
                    added += usize::from(self.favorite_paths.len() > before);
                } else { ignored += 1; }
            }
            self.queue_status = Some(format!("Added {added} favorite(s); ignored {ignored} unsupported item(s)."));
        } else if paths.len() == 1 {
            // Single file: open it (preserve the normal drop behavior outside the queue).
            if let Some(path) = paths.first() {
                self.open_path(path.clone());
            }
        } else if paths.len() > 1 {
            // Multiple files: load them as a set.
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
