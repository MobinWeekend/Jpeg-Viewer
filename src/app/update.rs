use super::types::{LoadingState, ViewerApp};
use super::virtual_texture::VirtualTexture;
use eframe::egui;
use image::GenericImageView;
use std::time::Instant;

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // ========== UPDATE CURRENT FPS (throttled to 0.5s) ==========
        let now = std::time::Instant::now();
        // Use last_repaint_time to compute delta
        let delta = now - self.last_repaint_time;
        if delta.as_secs_f32() > 0.001 {
            let instant_fps = 1.0 / delta.as_secs_f32();
            let smoothed = self.current_fps * 0.9 + instant_fps * 0.1;
            if now - self.last_fps_update >= std::time::Duration::from_millis(500) {
                self.current_fps = smoothed;
                self.last_fps_update = now;
            }
        }

        // Initialize logo texture
        if self.logo_texture.is_none() {
            match image::load_from_memory(include_bytes!("../../assets/icon.ico")) {
                Ok(img) => {
                    let image = img.into_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    let color_image =
                        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                    self.logo_texture =
                        Some(ctx.load_texture("logo", color_image, Default::default()));
                }
                Err(e) => {
                    eprintln!("Failed to load logo icon: {}. Using fallback.", e);
                }
            }
        }

        // Load frame limiter settings on first run
        if self.max_fps == 0.0 && self.idle_fps_limit == 0.0 {
            self.load_frame_limiter_settings();
        }

        // Load slideshow settings on first run
        if !self.slideshow_has_advanced {
            self.load_slideshow_settings();
        }

        // ========== STARTUP FULLSCREEN ==========
        if !self.startup_fullscreen_handled {
            self.startup_fullscreen_handled = true;
            let settings = self.settings_manager.get();
            if settings.start_fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
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
        if self.slideshow_enabled && !self.image_entries.is_empty() && !self.is_loading() {
            let elapsed = self.slideshow_last_advance.elapsed();
            if elapsed >= self.slideshow_interval {
                if self.slideshow_loop || self.current_index < self.image_entries.len() - 1 {
                    self.advance_slideshow(ctx);
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

                // Clear old file type detection before loading new image
                self.file_type_detection = None;

                match result {
                    Ok(loaded_image) => {
                        self.detect_current_file_type();
                        self.add_to_cache(ctx, self.current_index, loaded_image.clone());
                        let mut should_spawn_full_gif = false;
                        match loaded_image {
                            super::types::LoadedImage::Static(img) => {
                                // This is a small image – upload directly
                                let (width, height) = img.dimensions();
                                let rgba = img.to_rgba8();
                                let size = [width as usize, height as usize];
                                let color =
                                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                let options = self.get_texture_options();
                                self.texture = Some(ctx.load_texture("image", color, options));
                                self.gif_animation = None;
                                self.is_gif = false;
                                self.is_preview = false;
                                self.image_error = None;
                                self.virtual_texture = None;
                                self.vt_progress = None;
                                self.vt_total_tiles = 0;
                                self.b_fit_to_window = true;
                                ctx.request_repaint();
                                self.set_loading_state(LoadingState::Idle);

                                // Detect file type after successful load
                                self.detect_current_file_type();
                            }
                            super::types::LoadedImage::Animated(gif, is_preview) => {
                                self.gif_animation = Some(gif);
                                self.is_gif = true;
                                self.is_preview = is_preview;
                                self.image_error = None;
                                self.virtual_texture = None;
                                self.b_fit_to_window = true;
                                self.set_loading_state(LoadingState::Idle);
                                // Detect file type after GIF load
                                self.detect_current_file_type();

                                // Spawn full GIF loading if this is a preview
                                if is_preview {
                                    println!(
                                        "GIF preview loaded, will spawn full GIF load after loading flag clears"
                                    );
                                    should_spawn_full_gif = true;
                                } else {
                                    println!(
                                        "Full GIF loaded directly ({} frames)",
                                        self.gif_animation
                                            .as_ref()
                                            .map(|g| g.frame_count())
                                            .unwrap_or(0)
                                    );
                                }
                            }
                            super::types::LoadedImage::VirtualPending(bytes, width, height) => {
                                // Large image – start virtual texture loading
                                println!(
                                    "VirtualPending: {}x{} ({} MP)",
                                    width,
                                    height,
                                    (width as u64 * height as u64) / 1_000_000
                                );

                                self.set_loading_state(LoadingState::VirtualTextureLoading);

                                // Clear old state
                                self.texture = None;
                                self.gif_animation = None;
                                self.is_gif = false;
                                self.is_preview = false;
                                self.image_error = None;

                                let tile_size = self.settings_manager.get().tile_size;

                                // Spawn thread to decode and prepare virtual texture
                                let handle = std::thread::spawn(move || {
                                    // Decode the bytes
                                    let img = crate::loader::load_image_from_bytes(&bytes, None)
                                        .expect("Failed to decode image"); // better error handling later
                                    // Create and prepare virtual texture
                                    let mut vt = VirtualTexture::new(img, tile_size);
                                    vt.prepare();
                                    vt
                                });
                                self.virtual_texture_thread = Some(handle);

                                // We don't have progress yet; will be set when thread finishes
                                self.vt_progress = None;
                                self.vt_total_tiles = 0;

                                self.detect_current_file_type();
                                ctx.request_repaint();
                            }
                        }
                        self.update_window_title(ctx);
                        self.preload_adjacent_images();
                        self.mark_interaction();

                        // Now spawn the full GIF loading if needed
                        if should_spawn_full_gif {
                            println!("Spawning full GIF load now...");
                            self.spawn_full_gif_loading();
                            self.set_loading_state(LoadingState::LoadingFullGif);
                        }
                    }
                    Err(error) => {
                        self.detect_current_file_type();
                        self.texture = None;
                        self.image_error = Some(error);
                        self.gif_animation = None;
                        self.is_gif = false;
                        self.is_preview = false;
                        self.virtual_texture = None;
                        self.set_loading_state(LoadingState::Idle);

                        // Detect file type even on error (so we can suggest rename)
                        self.detect_current_file_type();

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
        if self.is_loading_virtual() {
            // Update progress from the stored progress if available
            if let Some(vt) = &self.virtual_texture {
                let progress = vt.progress();
                self.vt_progress = Some(progress);
                self.vt_total_tiles = vt.total_tiles();
                ctx.request_repaint();
                self.mark_interaction();
            }

            if let Some(handle) = self.virtual_texture_thread.take() {
                if handle.is_finished() {
                    // Join the thread and get the prepared virtual texture
                    match handle.join() {
                        Ok(vt) => {
                            self.virtual_texture = Some(vt);
                            self.vt_progress = None;
                            self.b_fit_to_window = true;

                            // Detect file type when virtual texture becomes ready
                            self.detect_current_file_type();

                            ctx.request_repaint();
                            println!("Virtual texture ready!");
                            self.set_loading_state(LoadingState::Idle);
                        }
                        Err(e) => {
                            eprintln!("Failed to prepare virtual texture: {:?}", e);
                            self.vt_progress = None;
                            self.image_error = Some("Failed to prepare large image".to_string());
                            self.detect_current_file_type();
                            self.set_loading_state(LoadingState::Idle);
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
                if !self.is_loading() && self.texture.is_some() {
                    let rgba = full_image.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let options = self.get_texture_options();
                    self.texture = Some(ctx.load_texture("image_full", color, options));
                    self.is_preview = false;
                    self.b_fit_to_window = true;
                    self.full_image_receiver = None;
                    self.cache_current_image();

                    // Detect file type after full image upgrade
                    self.detect_current_file_type();

                    self.update_window_title(ctx);
                    self.mark_interaction();
                    // flag here
                    ctx.request_repaint();
                    self.set_loading_state(LoadingState::Idle);
                } else {
                    self.full_image_receiver = None;
                }
            }
        }

        // ========== FULL GIF UPGRADE ==========
        if let Some(rx) = &self.full_gif_receiver {
            match rx.try_recv() {
                Ok(result) => {
                    self.full_gif_receiver = None;
                    match result {
                        Ok(loaded_image) => {
                            self.detect_current_file_type();
                            if self.is_gif && self.is_preview {
                                if let super::types::LoadedImage::Animated(full_gif, _) =
                                    loaded_image
                                {
                                    let frame_count = full_gif.frame_count();
                                    println!(
                                        "Full GIF loaded with {} frames, upgrading...",
                                        frame_count
                                    );

                                    if let Some(mut gif) = self.gif_animation.take() {
                                        gif.upgrade_to_full(full_gif);
                                        gif.is_playing = true;
                                        gif.last_update = Instant::now();
                                        self.gif_animation = Some(gif);
                                    }

                                    self.is_preview = false;
                                    self.update_window_title(ctx);
                                    self.update_gif_texture(ctx);
                                    self.detect_current_file_type();
                                    self.mark_interaction();
                                    self.set_loading_state(LoadingState::Idle);

                                    if let Some(gif) = &self.gif_animation {
                                        println!(
                                            "GIF upgraded! {} frames, playing: {}",
                                            gif.frame_count(),
                                            gif.is_playing
                                        );
                                    }

                                    ctx.request_repaint();
                                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                                }
                            } else {
                                println!("Full GIF loaded but preview not showing");
                            }
                        }
                        Err(error) => {
                            self.detect_current_file_type();
                            eprintln!("Failed to load full GIF: {}", error);
                            self.is_preview = false;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.full_gif_receiver = None;
                    eprintln!("Full GIF loading task was cancelled");
                }
            }
        }

        // Cancel loading if we navigated away from a GIF preview
        if !self.is_gif || !self.is_preview {
            if self.full_gif_receiver.is_some() {
                self.full_gif_receiver = None;
                self.set_loading_state(LoadingState::Idle);
                println!("Cancelled full GIF loading - no longer viewing a GIF preview");
            }
        }

        // ========== GIF ANIMATION ==========
        if self.is_gif {
            self.update_gif_texture(ctx);
        }

        // ========== PRELOAD ==========
        if !self.image_entries.is_empty() && !self.is_loading() && self.image_error.is_none() {
            self.preload_adjacent_images();
        }

        // ========== UI RENDERING ==========
        crate::app::ui::render_overlay_ui(self, ctx);
        self.render_central_panel(ctx);

        if self.texture.is_none() && !self.is_loading() && self.image_entries.is_empty() {
            self.render_shortcut_help(ctx);
        }

        if self.show_settings_menu {
            crate::app::ui::render_settings_menu(self, ctx);
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

                if gif.is_playing && gif.is_animated() {
                    let current_delay = if gif.current_frame < gif.delays.len() {
                        gif.delays[gif.current_frame]
                    } else {
                        std::time::Duration::from_millis(100)
                    };
                    let adjusted_delay = if gif.speed_multiplier > 0.0 {
                        std::time::Duration::from_micros(
                            (current_delay.as_micros() as f32 / gif.speed_multiplier) as u64,
                        )
                    } else {
                        current_delay
                    };
                    ctx.request_repaint_after(adjusted_delay);
                }
            }
        }
    }
}
