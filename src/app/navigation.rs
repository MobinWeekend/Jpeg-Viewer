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

        let new_index = (self.current_index as i32 + direction)
            .rem_euclid(len as i32) as usize;

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

        // Clear file type detection before changing image
        self.file_type_detection = None;

        // Change image
        self.current_index = new_index;

        // Reset image view
        self.zoom = 1.0;
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

        // Update UI
        ctx.request_repaint();
        self.update_window_title(ctx);
    }
}