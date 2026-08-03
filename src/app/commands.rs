use super::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn handle_command(&mut self, ctx: &egui::Context, command: ViewerCommand) {
        match command {
            ViewerCommand::NextImage => {
                self.navigate_images(ctx, 1);
            }
            ViewerCommand::PreviousImage => {
                self.navigate_images(ctx, -1);
            }
            ViewerCommand::ZoomIn => {
                self.zoom *= 1.1;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }
            ViewerCommand::ZoomOut => {
                self.zoom /= 1.1;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }
            ViewerCommand::ResetZoom => {
                self.zoom = 1.0;
            }
            ViewerCommand::MakeFit => {
                self.b_fit_to_window = true;
                self.b_zoom_used = false;
            }
            ViewerCommand::OpenFile => {
                self.open_file_dialog();
            }
            ViewerCommand::ToggleFullscreen => {
                self.toggle_fullscreen(ctx);
                self.b_fit_to_window = false;
            }
            ViewerCommand::JumpToFirst => {
                if !self.image_entries.is_empty() {
                    self.current_index = 0;
                    self.b_fit_to_window = true;
                    self.image_rect = None;
                    self.gif_animation = None;
                    self.is_gif = false;
                    self.is_preview = false;
                    self.texture = None;
                    self.full_image_receiver = None;
                    self.full_gif_receiver = None;
                    self.b_is_loading_full = false;
                    self.load_current_image_with_cache();
                    self.preload_adjacent_images();
                }
            }
            ViewerCommand::JumpToLast => {
                if !self.image_entries.is_empty() {
                    self.current_index = self.image_entries.len() - 1;
                    self.b_fit_to_window = true;
                    self.image_rect = None;
                    self.gif_animation = None;
                    self.is_gif = false;
                    self.is_preview = false;
                    self.texture = None;
                    self.full_image_receiver = None;
                    self.full_gif_receiver = None;
                    self.b_is_loading_full = false;
                    self.load_current_image_with_cache();
                    self.preload_adjacent_images();
                }
            }
            ViewerCommand::ToggleGifPlay => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.toggle_play();
                }
            }
            ViewerCommand::GifSpeedHalf => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.set_speed(0.5);
                }
            }
            ViewerCommand::GifSpeedUp => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.set_speed(2.0);
                }
            }
            ViewerCommand::GifSpeedReset => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.set_speed(1.0);
                }
            }
            ViewerCommand::Settings => {
                self.toggle_settings_menu();
            }
        }
    }
}