use super::types::ViewerApp;
use eframe::egui;
use image::GenericImageView;
use std::time::Instant;

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

        // Load frame limiter settings on first run
        if self.max_fps == 0.0 && self.idle_fps_limit == 0.0 {
            self.load_frame_limiter_settings();
        }

        // Load slideshow settings on first run
        if !self.slideshow_has_advanced {
            self.load_slideshow_settings();
        }

        // Handle startup fullscreen
        static mut STARTUP_FULLSCREEN_SET: bool = false;
        unsafe {
            if !STARTUP_FULLSCREEN_SET {
                STARTUP_FULLSCREEN_SET = true;
                let settings = self.settings_manager.get();
                if settings.start_fullscreen {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                }
            }
        }

        // ========== HARDCODED INPUT HANDLING ==========
        // Mark interaction if any input is processed
        let had_input = ctx.input(|i| {
            i.pointer.any_down()
                || i.pointer.delta().length() > 0.0
                || !i.keys_down.is_empty()
                || i.raw_scroll_delta != egui::Vec2::ZERO
        });

        // Check if window has focus (handle Option<bool>)
        let has_focus = ctx.input(|i| i.viewport().focused).unwrap_or(false);

        // Only mark interaction if window has focus OR it's a key press
        if had_input && (has_focus || ctx.input(|i| !i.keys_down.is_empty())) {
            self.mark_interaction();
        }

        self.handle_input(ctx);
        self.handle_window_resize(ctx);

        // ========== UPDATE ANIMATION STATE ==========
        // Track if we have an animated GIF playing
        let is_animating = if let Some(gif) = &self.gif_animation {
            gif.is_playing && gif.is_animated()
        } else {
            false
        };
        self.set_animating(is_animating);

        // ========== SLIDESHOW LOGIC ==========
        if self.slideshow_enabled && !self.image_entries.is_empty() && !self.b_is_loading {
            let elapsed = self.slideshow_last_advance.elapsed();
            if elapsed >= self.slideshow_interval {
                // Check if we should loop or stop
                if self.slideshow_loop || self.current_index < self.image_entries.len() - 1 {
                    self.advance_slideshow();
                    self.slideshow_last_advance = Instant::now();
                    self.slideshow_has_advanced = true;
                    // Repaint to show the new image
                    ctx.request_repaint();
                } else {
                    // Reached end and not looping - stop slideshow
                    self.slideshow_enabled = false;
                    let _ = self.settings_manager.update(|settings| {
                        settings.slideshow_enabled = false;
                    });
                    self.update_window_title(ctx);
                }
            }
        }

        // ========== PRELOAD TASKS ==========
        self.process_preload_tasks(ctx);

        // ========== IMAGE LOADING ==========
        // Check for loaded image - now handles Result
        if let Some(rx) = &self.receiver {
            if let Ok(result) = rx.try_recv() {
                // Clear the receiver immediately to prevent duplicate processing
                self.receiver = None;

                match result {
                    Ok(loaded_image) => {
                        // Success - process the image
                        self.add_to_cache(ctx, self.current_index, loaded_image.clone());

                        match loaded_image {
                            super::types::LoadedImage::Static(img) => {
                                let (width, height) = img.dimensions();
                                const MAX_TEXTURE_SIZE: u32 = 32768;

                                if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
                                    self.b_is_loading = false;
                                    self.texture = None;
                                    self.image_error = Some(format!(
                                        "Image too large: {}x{}\nMaximum supported size: {}x{}",
                                        width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
                                    ));
                                    eprintln!(
                                        "Failed to load image: too large ({}x{})",
                                        width, height
                                    );
                                    // Force repaint to show error
                                    ctx.request_repaint();
                                    return;
                                }

                                let rgba = img.to_rgba8();
                                let size = [width as usize, height as usize];
                                let color =
                                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                let options = self.get_texture_options();
                                self.texture = Some(ctx.load_texture("image", color, options));
                                self.gif_animation = None;
                                self.is_gif = false;
                                self.is_preview = false;
                                self.b_is_loading_full = false;
                                self.image_error = None;

                                // Check for extreme aspect ratio
                                if self.has_extreme_aspect_ratio(width, height) {
                                    self.b_fit_to_window = false;
                                    self.zoom = 1.0;
                                    self.pan = egui::Vec2::ZERO;
                                    self.b_zoom_used = true;
                                } else {
                                    self.b_fit_to_window = true;
                                }
                            }
                            super::types::LoadedImage::Animated(gif, is_preview) => {
                                self.gif_animation = Some(gif);
                                self.is_gif = true;
                                self.is_preview = is_preview;
                                self.image_error = None;
                            }
                        }

                        self.b_is_loading = false;

                        // After loading, cache the current image (GIFs will be skipped)
                        self.cache_current_image();

                        // Update window title with current filename
                        self.update_window_title(ctx);

                        // Trigger initial preload immediately after loading
                        self.preload_adjacent_images();

                        // Mark interaction so we show the loaded image immediately
                        self.mark_interaction();
                    }
                    Err(error) => {
                        // Error - show the error message and clean up
                        self.b_is_loading = false;
                        self.texture = None;
                        self.image_error = Some(error);
                        self.gif_animation = None;
                        self.is_gif = false;
                        self.is_preview = false;
                        eprintln!(
                            "Error loading image: {}",
                            self.image_error.as_ref().unwrap()
                        );
                        // Force repaint to show error
                        ctx.request_repaint();
                    }
                }
            }
        }

        // ========== FULL IMAGE LOADING ==========
        if let Some(rx) = &self.full_image_receiver {
            if let Ok(full_image) = rx.try_recv() {
                if !self.b_is_loading && self.texture.is_some() {
                    let rgba = full_image.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let options = self.get_texture_options();
                    self.texture = Some(ctx.load_texture("image_full", color, options));
                    self.b_is_loading_full = false;
                    self.is_preview = false;
                    self.b_fit_to_window = true;
                    self.full_image_receiver = None;
                    self.cache_current_image();
                    self.update_window_title(ctx);
                    self.mark_interaction();
                } else {
                    self.full_image_receiver = None;
                }
            }
        }

        // ========== FULL GIF UPGRADE ==========
        if let Some(rx) = &self.full_gif_receiver {
            if let Ok(result) = rx.try_recv() {
                self.full_gif_receiver = None;

                match result {
                    Ok(loaded_image) => {
                        if let Some(gif) = &mut self.gif_animation {
                            if let super::types::LoadedImage::Animated(full_gif, _) = loaded_image {
                                gif.upgrade_to_full(full_gif);
                                self.is_preview = false;
                                self.update_window_title(ctx);
                                self.update_gif_texture(ctx);
                                self.mark_interaction();
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to load full GIF: {}", error);
                    }
                }
            }
        }

        // ========== GIF ANIMATION ==========
        if self.is_gif {
            self.update_gif_texture(ctx);
        }

        // ========== PRELOAD ==========
        if !self.image_entries.is_empty() && !self.b_is_loading && self.image_error.is_none() {
            self.preload_adjacent_images();
        }

        // ========== UI RENDERING ==========
        self.render_top_panel(ctx);
        self.render_central_panel(ctx);

        // ========== KEYBOARD SHORTCUT HELP ==========
        // Show a small help overlay when no image is loaded
        if self.texture.is_none() && !self.b_is_loading && self.image_entries.is_empty() {
            self.render_shortcut_help(ctx);
        }

        // ========== SETTINGS MENU ==========
        if self.show_settings_menu {
            self.render_settings_menu(ctx);
        }

        // ========== FRAME LIMITER ==========
        // Only request repaint if the frame limiter allows it
        if self.should_request_repaint(ctx) {
            ctx.request_repaint();
        }

        // ========== CLEANUP ==========
        if ctx.input(|i| i.viewport().close_requested()) {
            self.stop_caching();
            self.save_window_state(ctx);
        }

        // ========== DELETE CONFIRMATION ==========
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
        let options = self.get_texture_options();

        if let Some(gif) = &mut self.gif_animation {
            if let Some(frame) = gif.get_current_frame() {
                let size = [frame.width() as usize, frame.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());

                self.texture = Some(ctx.load_texture("gif_frame", color_image, options));

                if gif.is_playing {
                    // GIF animations always need repaint
                    ctx.request_repaint();
                }
            }
        }
    }
}