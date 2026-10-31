use super::types::{LoadingState, ViewerApp};
use super::virtual_texture::VirtualTexture;
use crate::decoder::decode_bytes;
use eframe::egui;
use std::time::Instant;

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Keep context-only layout helpers synchronized with the persisted setting.
        crate::app::ui::set_desktop_menu_bar_preference(
            ctx,
            &self.settings_manager.get().desktop_menu_bar_preference,
        );
        let appearance_settings = self.settings_manager.get().clone();
        self.update_window_title(ctx);
        crate::platform_backdrop::set_native_titlebar_color(
            crate::ui_theme::native_title_bar_color(
                ctx,
                &appearance_settings.title_bar_color_mode,
                &appearance_settings.main_view_background,
            ),
        );
        let mut backdrop_preference_changed = false;
        if crate::ui_theme::os_background_blur_preference(ctx)
            != appearance_settings.os_background_blur
        {
            crate::ui_theme::set_os_background_blur_preference(
                ctx,
                appearance_settings.os_background_blur,
            );
            backdrop_preference_changed = true;
        }
        if crate::ui_theme::disable_os_background_blur_in_fullscreen_preference(ctx)
            != appearance_settings.disable_os_background_blur_in_fullscreen
        {
            crate::ui_theme::set_disable_os_background_blur_in_fullscreen(
                ctx,
                appearance_settings.disable_os_background_blur_in_fullscreen,
            );
            backdrop_preference_changed = true;
        }
        let backdrop_effective_changed = crate::ui_theme::sync_os_background_blur(ctx);
        if backdrop_preference_changed || backdrop_effective_changed {
            crate::ui_theme::apply_polished_style_with_options(
                ctx,
                &appearance_settings.ui_accent_color,
                appearance_settings.ui_scale,
                appearance_settings.ui_text_scale,
            );
        }
        // Indexing process so ui would load fast
        self.process_indexing();
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
            match image::load_from_memory(include_bytes!("../../assets/icons/icon.ico")) {
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
        if !self.slideshow_settings_loaded {
            self.load_slideshow_settings();
            self.slideshow_settings_loaded = true;
        }

        // ========== STARTUP WINDOW PLACEMENT ==========
        // Check the first reported native geometry once the platform has created the window.
        // egui reports the current monitor size; clamp saved positions back onto that monitor
        // when a monitor was removed or its resolution changed.
        if !self.startup_window_placement_handled {
            let (monitor_size, outer_rect, inner_size, maximized, fullscreen) =
                ctx.input(|input| {
                    let viewport = input.viewport();
                    (
                        viewport.monitor_size,
                        viewport.outer_rect,
                        viewport.inner_rect.map(|r| r.size()),
                        viewport.maximized.unwrap_or(false),
                        viewport.fullscreen.unwrap_or(false),
                    )
                });
            if let (Some(monitor_size), Some(outer_rect), Some(inner_size)) =
                (monitor_size, outer_rect, inner_size)
            {
                let settings = self.settings_manager.get();
                if settings.start_centered {
                    // Center once using the actual outer frame dimensions. The saved
                    // normal position is left untouched when start-centered is off.
                    let pos = egui::pos2(
                        ((monitor_size.x - outer_rect.width()) * 0.5).max(0.0),
                        ((monitor_size.y - outer_rect.height()) * 0.5).max(0.0),
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                } else {
                    // `with_position` and `OuterPosition` both use the outer frame's
                    // top-left. Clamp only when a frame edge is actually outside the
                    // current monitor, and ignore sub-point decoration/DPI rounding so
                    // a correctly restored window does not creep each time it opens.
                    let max_x = (monitor_size.x - outer_rect.width()).max(0.0);
                    let max_y = (monitor_size.y - outer_rect.height()).max(0.0);
                    let out_of_bounds = outer_rect.min.x < -2.0
                        || outer_rect.min.y < -2.0
                        || outer_rect.max.x > monitor_size.x + 2.0
                        || outer_rect.max.y > monitor_size.y + 2.0;
                    if out_of_bounds {
                        let pos = egui::pos2(
                            outer_rect.min.x.clamp(0.0, max_x),
                            outer_rect.min.y.clamp(0.0, max_y),
                        );
                        if (pos - outer_rect.min).length_sq() > 4.0 {
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                        }
                    }
                }

                // Only shrink when the actual client size exceeds the current monitor.
                // Never issue InnerSize for a size that already fits: the app persists its
                // own geometry, and repeated native resize commands can compound rounding.
                if !maximized
                    && !fullscreen
                    && (inner_size.x > monitor_size.x + 1.0 || inner_size.y > monitor_size.y + 1.0)
                {
                    let target_size = egui::vec2(
                        inner_size.x.min(monitor_size.x).max(100.0),
                        inner_size.y.min(monitor_size.y).max(100.0),
                    );
                    if (target_size - inner_size).length_sq() > 1.0 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size));
                    }
                }
                self.startup_window_placement_handled = true;
            }
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
                    self.update_window_title(ctx);
                }
            }
        }

        // Prevent system sleep only while slideshow playback is active and the option is enabled.
        let prevent_sleep =
            self.slideshow_enabled && self.settings_manager.get().prevent_sleep_during_slideshow;
        self.power_inhibitor.set_active(prevent_sleep);

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
                                let width = img.width;
                                let height = img.height;
                                let size = [width as usize, height as usize];
                                let color =
                                    egui::ColorImage::from_rgba_unmultiplied(size, img.data());
                                let options = self.get_texture_options();
                                self.texture = Some(ctx.load_texture("image", color, options));
                                self.cache_tricks_backdrop_from_decoded(
                                    self.current_image_path.clone(),
                                    width,
                                    height,
                                    img.data,
                                );
                                self.mark_win98_dialup_texture_ready(false);
                                if self.slideshow_previous_texture.is_some()
                                    && self.slideshow_transition_started.is_none()
                                    && self
                                        .slideshow_previous_texture
                                        .as_ref()
                                        .map(egui::TextureHandle::id)
                                        != self.texture.as_ref().map(egui::TextureHandle::id)
                                {
                                    self.slideshow_transition_started = Some(Instant::now());
                                }
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
                                // Update meta data
                                self.file_metadata = Some(self.build_file_metadata());
                                // Detect file type after successful load
                                self.detect_current_file_type();
                            }
                            super::types::LoadedImage::Animated(gif, is_preview) => {
                                self.gif_animation = Some(gif);
                                let backdrop_frame = self.gif_animation.as_ref().and_then(|gif| {
                                    gif.get_current_frame_ref().map(|frame| {
                                        (frame.width(), frame.height(), frame.as_raw().clone())
                                    })
                                });
                                if let (Some(path), Some((width, height, pixels))) =
                                    (self.current_image_path.clone(), backdrop_frame)
                                {
                                    self.cache_tricks_backdrop_from_decoded(
                                        Some(path),
                                        width,
                                        height,
                                        pixels,
                                    );
                                }
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
                                    let img = decode_bytes(&bytes).expect("Failed to decode image");
                                    let mut vt = VirtualTexture::new(img, tile_size); // VirtualTexture must accept DecodedImage
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
                ctx.request_repaint_after(std::time::Duration::from_millis(160));
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

                            ctx.request_repaint_after(std::time::Duration::from_millis(80));
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
                    ctx.request_repaint_after(std::time::Duration::from_millis(160));
                }
            }
        }

        // ========== FULL IMAGE LOADING (non-GIF) ==========
        if let Some(rx) = &self.full_image_receiver {
            if let Ok(full_image) = rx.try_recv() {
                if !self.is_loading() && self.texture.is_some() {
                    // full_image is DecodedImage; refresh the small backdrop source from
                    // the full-resolution decoded buffer without reopening the file.
                    let width = full_image.width;
                    let height = full_image.height;
                    let size = [width as usize, height as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, full_image.data());
                    let options = self.get_texture_options();
                    self.texture = Some(ctx.load_texture("image_full", color, options));
                    self.cache_tricks_backdrop_from_decoded(
                        self.current_image_path.clone(),
                        width,
                        height,
                        full_image.data,
                    );
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
                    ctx.request_repaint_after(std::time::Duration::from_millis(32));
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
        // The menu bar is a top panel and must be registered before sidebars and
        // the central canvas so it reserves layout space instead of covering them.
        self.hamburger_button_rect = None;
        self.right_sidebar_actual_width = 0.0;
        // Custom title-bar chrome is a real top-panel reservation, not just a
        // foreground overlay; it therefore pushes the menu and central UI downward.
        crate::app::ui::reserve_custom_titlebar_space(self, ctx);
        crate::app::ui::render_desktop_menu_bar(self, ctx);
        // Docked panels must be registered before the central panel so the image
        // canvas is laid out in the remaining space rather than underneath them.
        crate::app::queues::render_left_sidebar(self, ctx);
        // Dock the active right-side tool panel before the central canvas so
        // it receives a proper share of the available layout width.
        self.render_editing_ui(ctx);
        if self.show_settings_menu && !self.show_edit_menu {
            crate::app::ui::render_settings_menu(self, ctx);
        }
        self.render_central_panel(ctx);
        crate::app::ui::render_overlay_ui(self, ctx);

        if self.texture.is_none() && !self.is_loading() && self.image_entries.is_empty() {
            self.render_shortcut_help(ctx);
        }

        // ========== FRAME LIMITER ==========
        self.schedule_repaint(ctx);
        if self.preload_working || self.is_loading() {
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

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if cfg!(any(target_os = "windows", target_os = "macos"))
            && self.settings_manager.get().os_background_blur
            && crate::platform_backdrop::is_active()
        {
            egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
        } else {
            // If the OS cannot provide a backdrop, keep the native window opaque.
            // Translucent egui surfaces still compose over this solid fallback.
            let [r, g, b, _] = visuals.panel_fill.to_srgba_unmultiplied();
            egui::Color32::from_rgb(r, g, b).to_normalized_gamma_f32()
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
        let mut texture_updated = false;

        // Keep the mutable borrow of the GIF animation scoped to frame extraction and
        // playback scheduling. Dial-up state needs `&mut self`, so update it only after
        // the GIF borrow has ended.
        if let Some(gif) = &mut self.gif_animation {
            if let Some(frame) = gif.get_current_frame() {
                let size = [frame.width() as usize, frame.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());

                self.texture = Some(ctx.load_texture("gif_frame", color_image, options));
                texture_updated = true;

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

        if texture_updated {
            self.mark_win98_dialup_texture_ready(false);
        }
    }
}
