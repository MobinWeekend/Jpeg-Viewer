use super::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn navigate_images(&mut self, ctx: &egui::Context, direction: i32) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        let new_index = (self.current_index as i32 + direction).rem_euclid(len as i32) as usize;

        if new_index != self.current_index {
            self.current_index = new_index;
            self.b_fit_to_window = true;
            self.image_rect = None;
            
            // Don't clear texture here - keep showing the current image
            // Only clear GIF state that will be replaced
            self.gif_animation = None;
            self.is_gif = false;
            self.is_preview = false;
            
            // Clear loading state for new image
            self.full_image_receiver = None;
            self.full_gif_receiver = None;
            self.b_is_loading_full = false;
            self.image_error = None;
            self.receiver = None;

            // Reset navigation timer when user navigates
            self.reset_navigation_timer();

            // Load the new image in background while keeping current texture
            self.load_current_image_with_cache_keep_texture();
            
            // Update window title with the new index
            self.update_window_title(ctx);
        }
        self.b_zoom_used = false;
    }
}