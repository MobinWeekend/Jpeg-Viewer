use super::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;
use std::time::Instant;

impl ViewerApp {
    pub fn handle_command(&mut self, ctx: &egui::Context, command: ViewerCommand) {
        match command {
            ViewerCommand::NextImage => {
                // Stop slideshow when manually navigating
                if self.slideshow_enabled {
                    self.slideshow_enabled = false;
                    let _ = self.settings_manager.update(|settings| {
                        settings.slideshow_enabled = false;
                    });
                    self.update_window_title(ctx);
                }
                self.navigate_images(ctx, 1);
            }
            ViewerCommand::PreviousImage => {
                // Stop slideshow when manually navigating
                if self.slideshow_enabled {
                    self.slideshow_enabled = false;
                    let _ = self.settings_manager.update(|settings| {
                        settings.slideshow_enabled = false;
                    });
                    self.update_window_title(ctx);
                }
                self.navigate_images(ctx, -1);
            }
            ViewerCommand::ZoomIn => {
                self.zoom *= 1.2;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }
            ViewerCommand::ZoomOut => {
                self.zoom /= 1.2;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }
            ViewerCommand::ResetZoom => {
                self.zoom = 1.0;
                self.b_zoom_used = true;
            }
            ViewerCommand::MakeFit => {
                self.b_fit_to_window = true;
                self.b_zoom_used = false;
            }
            ViewerCommand::OpenFile => {
                self.update_window_title(ctx);
                self.open_file_dialog();
            }
            ViewerCommand::OpenFolder => {
                self.update_window_title(ctx);
                self.open_folder_dialog();
            }
            ViewerCommand::ToggleFullscreen => {
                self.toggle_fullscreen(ctx);
                self.b_fit_to_window = false;
            }
            ViewerCommand::JumpToFirst => {
                if !self.image_entries.is_empty() {
                    // Stop slideshow when jumping
                    if self.slideshow_enabled {
                        self.slideshow_enabled = false;

                        let _ = self.settings_manager.update(|settings| {
                            settings.slideshow_enabled = false;
                        });
                    }

                    self.navigate_to_index(ctx, 0);
                }
            }
            ViewerCommand::JumpToLast => {
                if !self.image_entries.is_empty() {
                    // Stop slideshow when jumping
                    if self.slideshow_enabled {
                        self.slideshow_enabled = false;

                        let _ = self.settings_manager.update(|settings| {
                            settings.slideshow_enabled = false;
                        });
                    }

                    self.navigate_to_index(ctx, self.image_entries.len() - 1);
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
                crate::app::ui::render_settings_menu(self, ctx);
            }
            ViewerCommand::Help => {
                self.toggle_help_menu();
            }
            ViewerCommand::ToggleSlideshow => {
                self.toggle_slideshow();
                if self.slideshow_enabled {
                    // Reset timer when starting slideshow
                    self.slideshow_last_advance = Instant::now();
                }
                self.update_window_title(ctx);
            }
            ViewerCommand::SlideshowSpeedUp => {
                self.slideshow_speed_up();
            }
            ViewerCommand::SlideshowSpeedDown => {
                self.slideshow_speed_down();
            }
            ViewerCommand::CopyPath => {
                self.copy_path_to_clipboard();
            }
            ViewerCommand::CopyImage => {
                self.copy_image_to_clipboard();
            }
        }
    }
}
