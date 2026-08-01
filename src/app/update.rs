use std::path::PathBuf;

//this needs to be splitted
use super::types::ViewerApp;
use crate::shortcuts::{handle_keyboard, handle_mouse};
use eframe::egui;
use image::GenericImageView;

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Initialize logo texture
        if self.logo_texture.is_none() {
            let image = image::load_from_memory(include_bytes!("../../assets/icon.ico"))
                .unwrap()
                .into_rgba8();

            let size = [image.width() as usize, image.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            self.logo_texture = Some(ctx.load_texture("logo", color_image, Default::default()));
        }

        // almighty esc key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

            if fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            } else {
                self.save_window_state(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            return;
        }

        // Standard keyboard shortcuts
        for command in handle_keyboard(ctx, &self.input_bindings) {
            self.handle_command(ctx, command);
        }

        // Delete key - trigger on key release
        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete));
        let delete_released = ctx.input(|i| i.key_released(egui::Key::Delete));

        if delete_pressed {
            self.delete_key_was_pressed = true;
        } else if delete_released && self.delete_key_was_pressed {
            self.delete_current_image();
            self.delete_key_was_pressed = false;
        }

        // mouse buttons
        for command in handle_mouse(
            ctx,
            &self.input_bindings,
            ctx.is_pointer_over_area(),
            self.b_ctrl_invert,
        ) {
            self.handle_command(ctx, command);
        }

        // drag & drop
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());

        if !dropped_files.is_empty() {
            let paths: Vec<PathBuf> = dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();
            if paths.len() == 1 {
                // If a single file is dropped, use default behavior (open the file)
                if let Some(path) = paths.get(0) {
                    self.open_path(path.clone());
                }
            } else if paths.len() > 1 {
                self.load_dropped_files(paths);
            }
        }
        // check for window resize
        let current_size = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size());
        if let (Some(prev), Some(curr)) = (self.last_window_size, current_size) {
            if prev != curr && !self.b_zoom_used {
                self.b_fit_to_window = true;
            }
        }
        self.last_window_size = current_size;

        // Process preload tasks first (background caching)
        self.process_preload_tasks(ctx);

        // Check for loaded image
        if let Some(rx) = &self.receiver {
            if let Ok(loaded_image) = rx.try_recv() {
                // Cache the loaded image (GIFs will be skipped)
                self.add_to_cache(ctx, self.current_index, loaded_image.clone());

                match loaded_image {
                    super::types::LoadedImage::Static(img) => {
                        let (width, height) = img.dimensions();
                        const MAX_TEXTURE_SIZE: u32 = 32768;

                        if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
                            // Show error message instead of crashing
                            self.b_is_loading = false;
                            self.texture = None;
                            self.image_error = Some(format!(
                                "Image too large: {}x{}\nMaximum supported size: {}x{}",
                                width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
                            ));
                            eprintln!("Failed to load image: too large ({}x{})", width, height);
                            return;
                        }

                        let rgba = img.to_rgba8();
                        let size = [width as usize, height as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        let options = self.get_texture_options();
                        self.texture = Some(ctx.load_texture("image", color, options));
                        self.gif_animation = None;
                        self.is_gif = false;
                        self.is_preview = false;
                        self.b_is_loading_full = false;
                        self.image_error = None;
                        let rgba = img.to_rgba8();
                        let width = rgba.width();
                        let height = rgba.height();
                        let size = [width as usize, height as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        let options = self.get_texture_options();
                        self.texture = Some(ctx.load_texture("image", color, options));
                        self.gif_animation = None;
                        self.is_gif = false;
                        self.is_preview = false;
                        self.b_is_loading_full = false;

                        // Check for extreme aspect ratio
                        if self.has_extreme_aspect_ratio(width, height) {
                            // Disable fit to window and use 1.0 zoom
                            self.b_fit_to_window = false;
                            self.zoom = 1.0;
                            self.pan = egui::Vec2::ZERO;
                            self.b_zoom_used = true; // Mark as zoom used so fit doesn't auto-apply
                        } else {
                            // Normal behavior: fit to window
                            self.b_fit_to_window = true;
                        }
                    }
                    super::types::LoadedImage::Animated(gif, is_preview) => {
                        self.gif_animation = Some(gif);
                        self.is_gif = true;
                        self.is_preview = is_preview;

                        // If it's a preview, we'll show the loading message
                        // The full GIF will be loaded in the background
                        if is_preview {
                            // Preview frame is loaded, show it immediately
                            // The full GIF loading will complete via full_gif_receiver
                        }
                    }
                }
                self.b_fit_to_window = true;
                self.b_is_loading = false;
                self.receiver = None;

                // After loading, cache the current image (GIFs will be skipped)
                self.cache_current_image();

                // Update window title with current filename
                self.update_window_title(ctx);

                // Trigger initial preload immediately after loading
                self.preload_adjacent_images(ctx);
            }
        }

        // Check for full image loaded (keep for compatibility)
        if let Some(rx) = &self.full_image_receiver {
            if let Ok(full_image) = rx.try_recv() {
                let rgba = full_image.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let options = self.get_texture_options();
                self.texture = Some(ctx.load_texture("image_full", color, options));
                self.b_is_loading_full = false;
                self.is_preview = false;
                self.b_fit_to_window = true;
                self.full_image_receiver = None;

                // Update cache with full quality
                self.cache_current_image();

                // Update window title
                self.update_window_title(ctx);
            }
        }

        // Check for full GIF upgrade (background loading complete)
        if let Some(rx) = &self.full_gif_receiver {
            if let Ok(loaded_image) = rx.try_recv() {
                if let super::types::LoadedImage::Animated(full_gif, _) = loaded_image {
                    if let Some(gif) = &mut self.gif_animation {
                        gif.upgrade_to_full(full_gif);
                        self.is_preview = false;

                        // Update cache (GIFs are not cached, but update texture)
                        self.full_gif_receiver = None;

                        // Update window title
                        self.update_window_title(ctx);

                        // Force texture update
                        self.update_gif_texture(ctx);
                    }
                }
            }
        }

        // Update GIF animation
        if self.is_gif {
            self.update_gif_texture(ctx);
        }

        // Preload adjacent images (non-blocking)
        if !self.image_entries.is_empty() && !self.b_is_loading {
            self.preload_adjacent_images(ctx);
        }

        // ========== UI ==========

        self.render_top_panel(ctx);
        self.render_central_panel(ctx);

        ctx.request_repaint();

        if ctx.input(|i| i.viewport().close_requested()) {
            self.stop_caching(); // Stop all caching
            self.save_window_state(ctx);
        }

        if self.show_delete_confirmation {
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Move this file to the Trash/Recycle Bin?");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            self.show_delete_confirmation = false;
                            self.delete_current_image();
                        }
                        if ui.button("No").clicked() {
                            self.show_delete_confirmation = false;
                        }
                    });
                });
        }
    }
}

impl Drop for ViewerApp {
    fn drop(&mut self) {
        if let Err(e) = self.settings_manager.save() {
            eprintln!("Failed to save settings: {}", e);
        }
    }
}

impl ViewerApp {
    pub fn update_gif_texture(&mut self, ctx: &egui::Context) {
        // Get options first (immutable borrow of self)
        let options = self.get_texture_options();

        if let Some(gif) = &mut self.gif_animation {
            if let Some(frame) = gif.get_current_frame() {
                let size = [frame.width() as usize, frame.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());

                self.texture = Some(ctx.load_texture("gif_frame", color_image, options));

                if gif.is_playing {
                    ctx.request_repaint();
                }
            }
        }
    }
}
