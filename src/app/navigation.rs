use super::types::{LoadingState, ViewerApp};
use eframe::egui;

impl ViewerApp {
    /// Navigate relative to the current image.
    ///
    /// `direction`:
    /// - `1`  = next image
    /// - `-1` = previous image
    /// - larger values = jump multiple images
    pub fn navigate_images(&mut self, ctx: &egui::Context, direction: i32) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();

        let new_index = if self.queue_mode && self.queue_shuffle && direction > 0 && len > 1 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let mut candidate = rng.gen_range(0..len);
            while candidate == self.current_index {
                candidate = rng.gen_range(0..len);
            }
            candidate
        } else if self.queue_mode && !self.queue_repeat {
            let candidate = self.current_index as i32 + direction;
            if candidate < 0 || candidate >= len as i32 { return; }
            candidate as usize
        } else {
            (self.current_index as i32 + direction).rem_euclid(len as i32) as usize
        };

        self.navigate_to_index(ctx, new_index);
    }

    /// Navigate directly to a specific image index.
    ///
    /// This contains all of the common image-navigation/reset/loading logic
    /// used by next/previous/first/last navigation.
    pub fn navigate_to_index(&mut self, ctx: &egui::Context, new_index: usize) {
        if self.edit_session.as_ref().is_some_and(|session| session.dirty) {
            self.edit_status = Some(
                "Save or discard the unsaved working copy before changing images.".into(),
            );
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        }
        if self.edit_session.is_some() {
            self.edit_session = None;
            self.show_edit_menu = false;
            self.crop_tool_active = false;
            self.crop_drag_start_image = None;
            self.crop_selection = None;
        }
        if self.image_entries.is_empty() {
            return;
        }

        // Invalid index
        if new_index >= self.image_entries.len() {
            return;
        }

        // Already viewing this image
        if new_index == self.current_index {
            return;
        }
        // Change image
        self.current_index = new_index;

        // Reset image view
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.b_zoom_used = false;

        // Reset GIF state
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;

        // Cancel old loading operations
        self.receiver = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;

        // Reset errors
        self.image_error = None;

        // Reset virtual texture state
        self.virtual_texture = None;
        self.virtual_texture_thread = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;

        //title bar update
        self.update_window_title(ctx);
        //update the current image path that is shown
        self.update_current_image_path();
        //Detect if wrong file format
        self.detect_current_file_type();

        // Reset loading state
        self.set_loading_state(LoadingState::Idle);

        // Reset navigation timer
        self.reset_navigation_timer();

        // Load the new image while keeping the old texture visible.
        // This prevents flashing when navigating quickly.
        self.load_current_image_with_cache_keep_texture();

        // Invalidate all previous async results (including detection)
        //self.preload_generation = self.preload_generation.wrapping_add(1);
        // Clear file type detection before changing image
        //self.set_file_type_detection(None);

        // Update meta data
        self.file_metadata = Some(self.build_file_metadata());

        // Update UI
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        // just in case!
        self.load_frame_limiter_settings();
    }

    pub fn advance_slideshow(&mut self, ctx: &egui::Context) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        /*
        let new_index = if self.slideshow_random {
            use rand::Rng;

            let mut rng = rand::thread_rng();

            if len <= 1 {
                self.current_index
            } else {
                loop {
                    let index = rng.gen_range(0..len);

                    if index != self.current_index {
                        break index;
                    }
                }
            }
        } else {
            (self.current_index + 1) % len
        };
         */
        if self.queue_mode && !self.queue_repeat && self.current_index + 1 >= len {
            self.slideshow_enabled = false;
            let _ = self.settings_manager.update(|settings| settings.slideshow_enabled = false);
            self.update_window_title(ctx);
            return;
        }
        let new_index = if self.queue_mode && self.queue_shuffle && len > 1 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let mut candidate = rng.gen_range(0..len);
            while candidate == self.current_index {
                candidate = rng.gen_range(0..len);
            }
            candidate
        } else {
            (self.current_index + 1) % len
        };

        // Keep the outgoing texture so the transition can blend/slide between
        // two distinct images instead of animating only the incoming image.
        let old_index = self.current_index;
        let old_texture = self.texture.clone();
        let should_transition = self.slideshow_enabled
            && self.slideshow_transition != "none"
            && old_texture.is_some();
        self.slideshow_previous_texture = if should_transition { old_texture.clone() } else { None };
        // Capture the exact rectangle used to render the outgoing image. The
        // next image resets zoom/pan for its own view, but this snapshot keeps
        // the outgoing image visually stable throughout the transition.
        self.slideshow_previous_rect = if should_transition { self.image_rect } else { None };
        self.slideshow_transition_started = None;

        self.navigate_to_index(ctx, new_index);

        // Cache hits replace the texture synchronously. For uncached images the
        // start time is set when the loader installs the incoming texture.
        if self.current_index != old_index {
            let old_id = old_texture.as_ref().map(egui::TextureHandle::id);
            let new_id = self.texture.as_ref().map(egui::TextureHandle::id);
            if self.slideshow_previous_texture.is_some() && old_id != new_id {
                self.slideshow_transition_started = Some(std::time::Instant::now());
            }
        }
    }
}
