use super::types::{LoadingState, ViewerApp};
use eframe::egui;

impl ViewerApp {
    pub fn navigate_images(&mut self, ctx: &egui::Context, direction: i32) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        let new_index = (self.current_index as i32 + direction).rem_euclid(len as i32) as usize;

        if new_index != self.current_index {
            // Clear file type detection BEFORE changing the image
            self.file_type_detection = None;
            self.current_index = new_index;
            self.zoom = 1.0;
            self.b_fit_to_window = true;
            self.image_rect = None;
            // Don't clear texture - keep showing current image, clearing the image every time causes flashing of screen when cycling fast
            self.gif_animation = None;
            self.is_gif = false;
            self.is_preview = false;
            self.full_image_receiver = None;
            self.full_gif_receiver = None;
            self.image_error = None;
            self.receiver = None;
            self.virtual_texture = None;
            self.vt_progress = None;
            self.vt_total_tiles = 0;

            // Make sure that navigation is not blocked by vt loading
            self.virtual_texture_thread = None;

            // Clear file type detection when navigating to a new image
            self.file_type_detection = None;

            // Reset navigation timer (no direction needed anymore)
            self.reset_navigation_timer();

            // Load new image in background while keeping current texture
            self.load_current_image_with_cache_keep_texture();
            ctx.request_repaint();
            self.update_window_title(ctx);
            self.set_loading_state(LoadingState::Idle);
        }
        self.b_zoom_used = false;
    }
}
