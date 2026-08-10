use super::types::ViewerApp;
use super::virtual_texture::VirtualTexture;
use eframe::egui;
use image::GenericImageView;
use std::time::Instant;

// Constants for deciding when to use virtual texturing.
use super::virtual_texture::{LARGE_IMAGE_THRESHOLD, MAX_GPU_TEXTURE_SIZE};

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

        // ========== HELP MENU ==========
        if self.show_help_menu {
            self.render_help_window(ctx);
        }

        // ========== HARDCODED INPUT HANDLING ==========
        let had_input = ctx.input(|i| {
            i.pointer.any_down()
                || i.pointer.delta().length() > 0.0
                || !i.keys_down.is_empty()
                || i.raw_scroll_delta != egui::Vec2::ZERO
        });

        let has_focus = ctx.input(|i| i.viewport().focused).unwrap_or(false);

        if had_input && (has_focus || ctx.input(|i| !i.keys_down.is_empty())) {
            self.mark_interaction();
        }

        self.handle_input(ctx);
        self.handle_window_resize(ctx);

        // ========== UPDATE ANIMATION STATE ==========
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
                if self.slideshow_loop || self.current_index < self.image_entries.len() - 1 {
                    self.advance_slideshow();
                    self.slideshow_last_advance = Instant::now();
                    self.slideshow_has_advanced = true;
                    ctx.request_repaint();
                } else {
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
        if let Some(rx) = &self.receiver {
            if let Ok(result) = rx.try_recv() {
                self.receiver = None;

                match result {
                    Ok(loaded_image) => {
                        self.add_to_cache(ctx, self.current_index, loaded_image.clone());

                        match loaded_image {
                            super::types::LoadedImage::Static(img) => {
                                let (width, height) = img.dimensions();
                                let pixel_count = width as u64 * height as u64;

                                // Check if we should use virtual texturing
                                let use_virtual = pixel_count > LARGE_IMAGE_THRESHOLD
                                    || width > MAX_GPU_TEXTURE_SIZE
                                    || height > MAX_GPU_TEXTURE_SIZE
                                    || width > 16384
                                    || height > 16384;

                                if use_virtual {
                                    // Use virtual texture for large images
                                    println!(
                                        "Using virtual texture for {}x{} ({} MP)",
                                        width,
                                        height,
                                        pixel_count / 1_000_000
                                    );

                                    // Create virtual texture
                                    let vt = VirtualTexture::new(img);

                                    // Store progress info before moving vt
                                    let progress = vt.progress_ref().lock().unwrap().clone();
                                    self.vt_progress = Some(progress);
                                    self.vt_total_tiles = vt.total_tiles();

                                    // Set loading state
                                    self.virtual_texture_loading = true;

                                    // Spawn background thread to prepare tiles
                                    let handle = std::thread::spawn(move || {
                                        let mut vt_clone = vt;
                                        vt_clone.prepare();
                                        vt_clone
                                    });
                                    self.virtual_texture_thread = Some(handle);

                                    self.texture = None;
                                    self.gif_animation = None;
                                    self.is_gif = false;
                                    self.is_preview = false;
                                    self.b_is_loading_full = false;
                                    self.image_error = None;
                                    self.b_is_loading = false;

                                    ctx.request_repaint();
                                } else {
                                    // Normal upload for small images
                                    const MAX_TEXTURE_SIZE: u32 = 32768;
                                    if width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
                                        self.b_is_loading = false;
                                        self.texture = None;
                                        self.image_error = Some(format!(
                                            "Image too large: {}x{}\nMaximum supported size: {}x{}",
                                            width, height, MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE
                                        ));
                                        ctx.request_repaint();
                                        return;
                                    }

                                    let rgba = img.to_rgba8();
                                    let size = [width as usize, height as usize];
                                    let color = egui::ColorImage::from_rgba_unmultiplied(
                                        size,
                                        rgba.as_raw(),
                                    );
                                    let options = self.get_texture_options();
                                    self.texture = Some(ctx.load_texture("image", color, options));
                                    self.gif_animation = None;
                                    self.is_gif = false;
                                    self.is_preview = false;
                                    self.b_is_loading_full = false;
                                    self.image_error = None;
                                    self.virtual_texture = None;
                                    self.virtual_texture_loading = false;
                                    self.b_fit_to_window = true;
                                }
                            }
                            super::types::LoadedImage::Animated(gif, is_preview) => {
                                self.gif_animation = Some(gif);
                                self.is_gif = true;
                                self.is_preview = is_preview;
                                self.image_error = None;
                                self.virtual_texture = None;
                                self.virtual_texture_loading = false;
                                self.b_fit_to_window = true;
                            }
                        }

                        self.b_is_loading = false;
                        self.update_window_title(ctx);
                        self.preload_adjacent_images();
                        self.mark_interaction();
                    }
                    Err(error) => {
                        self.b_is_loading = false;
                        self.texture = None;
                        self.image_error = Some(error);
                        self.gif_animation = None;
                        self.is_gif = false;
                        self.is_preview = false;
                        self.virtual_texture = None;
                        self.virtual_texture_loading = false;
                        eprintln!(
                            "Error loading image: {}",
                            self.image_error.as_ref().unwrap()
                        );
                        ctx.request_repaint();
                    }
                }
            }
        }

        // ========== CHECK VIRTUAL TEXTURE BACKGROUND LOADING ==========
        if self.virtual_texture_loading {
            // Update progress from the stored progress if available
            if let Some(vt) = &self.virtual_texture {
                let progress = vt.progress_ref().lock().unwrap().clone();
                self.vt_progress = Some(progress);
                self.vt_total_tiles = vt.total_tiles();
            }

            if let Some(handle) = self.virtual_texture_thread.take() {
                if handle.is_finished() {
                    // Join the thread and get the prepared virtual texture
                    match handle.join() {
                        Ok(vt) => {
                            self.virtual_texture = Some(vt);
                            self.virtual_texture_loading = false;
                            self.vt_progress = None;
                            self.b_fit_to_window = true;
                            ctx.request_repaint();
                            println!("Virtual texture ready!");
                        }
                        Err(e) => {
                            eprintln!("Failed to prepare virtual texture: {:?}", e);
                            self.virtual_texture_loading = false;
                            self.vt_progress = None;
                            self.image_error = Some("Failed to prepare large image".to_string());
                        }
                    }
                } else {
                    // Put the handle back - still running
                    self.virtual_texture_thread = Some(handle);
                    // Request repaint to update progress bar
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }

        // ========== FULL IMAGE LOADING (non-GIF) ==========
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

        if self.texture.is_none() && !self.b_is_loading && self.image_entries.is_empty() {
            self.render_shortcut_help(ctx);
        }

        if self.show_settings_menu {
            self.render_settings_menu(ctx);
        }

        // ========== FRAME LIMITER ==========
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
        // Wait for virtual texture background thread to finish
        if let Some(handle) = self.virtual_texture_thread.take() {
            let _ = handle.join();
        }
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
                    ctx.request_repaint();
                }
            }
        }
    }
}
