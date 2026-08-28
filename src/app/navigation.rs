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

        let new_index = (self.current_index as i32 + direction).rem_euclid(len as i32) as usize;

        self.navigate_to_index(ctx, new_index);
    }

    /// Navigate directly to a specific image index.
    ///
    /// This contains all of the common image-navigation/reset/loading logic
    /// used by next/previous/first/last navigation.
    pub fn navigate_to_index(&mut self, ctx: &egui::Context, new_index: usize) {
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

        // Reset loading state
        self.set_loading_state(LoadingState::Idle);

        // Reset navigation timer
        self.reset_navigation_timer();

        // Load the new image while keeping the old texture visible.
        // This prevents flashing when navigating quickly.
        self.load_current_image_with_cache_keep_texture();

        // Invalidate all previous async results (including detection)
        self.preload_generation = self.preload_generation.wrapping_add(1);
        // Clear file type detection before changing image
        self.set_file_type_detection(None);

        // Update UI
        ctx.request_repaint_after(std::time::Duration::from_millis(32));
        //title bar update
        self.update_window_title(ctx);
        //update the current image path that is shown
        self.update_current_image_path();
        //self.get_rename_suggestion();
        crate::app::ui::rename_warning(self, ctx);
        // just in case!
        self.load_frame_limiter_settings();
    }

    pub fn advance_slideshow(&mut self, ctx: &egui::Context) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();

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

        self.navigate_to_index(ctx, new_index);
    }
}
