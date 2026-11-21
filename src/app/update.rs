use super::types::{LoadingState, ViewerApp};
use super::virtual_texture::VirtualTexture;
use crate::decoder::decode_bytes;
use eframe::egui;
use std::time::Instant;

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Check for a stable GitHub release at most once every seven days. The
        // request runs on a dedicated thread and never blocks the UI frame.
        if let Some(status) = self.updater.poll() {
            match status {
                super::updater::UpdateStatus::UpdateAvailable { release } => {
                    let cached_release_json = release.to_settings_json();
                    let _ = self.settings_manager.update(|settings| {
                        settings.update_available_release_json = cached_release_json;
                    });
                }
                super::updater::UpdateStatus::UpToDate { .. } => {
                    let _ = self.settings_manager.update(|settings| {
                        settings.update_available_release_json.clear();
                    });
                }
                super::updater::UpdateStatus::Idle
                | super::updater::UpdateStatus::Checking
                | super::updater::UpdateStatus::Failed { .. } => {}
            }
        }
        if let Some(status) = self.updater.poll_install()
            && matches!(status, super::updater::UpdateInstallStatus::Restarting)
        {
            // Portable AppImage/macOS/Linux/Windows builds use a helper process
            // that waits for this process to close before replacing the binary.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Deferred pop-outs own their rendering and animation callbacks. Keep
        // their viewport registrations alive on the minimize transition, while
        // avoiding the full Viewer/sidebar/layout/preload pass for a hidden root.
        // The child callbacks continue repainting independently while this root
        // sleeps; a low-frequency root wake re-registers them on older backends.
        if ctx.input(|input| input.viewport().minimized.unwrap_or(false)) {
            self.popouts.render_viewports(ctx);
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            return;
        }

        if let Some(attempted_at) = self.updater.maybe_start_automatic_check(
            self.settings_manager.get().update_check_last_attempt_unix,
            ctx,
        ) {
            let _ = self.settings_manager.update(|settings| {
                settings.update_check_last_attempt_unix = attempted_at;
            });
        }

        if let Some(Err(error)) = self.settings_manager.save_if_due() {
            eprintln!("Could not persist updated settings: {error}");
        }
        if let Some(delay) = self.settings_manager.time_until_save() {
            ctx.request_repaint_after(delay.max(std::time::Duration::from_millis(1)));
        }
        #[cfg(target_os = "linux")]
        crate::platform::linux_thumbnails::set_publish_enabled(
            self.settings_manager.get().publish_generated_thumbnails,
        );

        // Reset a shared upload allowance once per UI frame. Every current-image
        // and virtual-tile callback spends from this same budget.
        self.mipmap_upload_budget.store(
            super::texture_mipmap::MAX_MIP_UPLOAD_BYTES_PER_FRAME,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.cache_ready_current_mipmap();

        // Keep context-only layout helpers synchronized with the persisted setting.
        crate::app::ui::set_desktop_menu_bar_preference(
            ctx,
            &self.settings_manager.get().desktop_menu_bar_preference,
        );
        let appearance_settings = self.settings_manager.get().clone();
        self.update_window_title(ctx);
        crate::ui_theme::platform_backdrop::set_native_titlebar_color(
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
        // Restore a previous session before processing new external opens. Explicit
        // launch arguments were installed before the first frame and are detected by
        // check_workspace_session_restore, so they take precedence over restoration.
        self.check_workspace_session_restore();

        // A forwarded launch can include many selected files. Process its request
        // on the UI thread so it uses the same selected-files path as drag-and-drop.
        let forwarded_requests = self
            .instance_server
            .as_ref()
            .map(crate::platform::instance::InstanceServer::drain_requests)
            .unwrap_or_default();
        for paths in forwarded_requests {
            self.handle_forwarded_open_request(paths, ctx);
        }
        self.process_pending_open_paths(ctx);

        // Restore maximized state after the native window has been created. This
        // extra command is important for native decorations where the window manager
        // can report an initial unmaximized frame before applying ViewportBuilder.
        if self.startup_restore_maximize_pending {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            if maximized {
                self.startup_restore_maximize_pending = false;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
        } else if !ctx.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            if maximized != self.settings_manager.get().window_maximized {
                self.settings_manager.settings.window_maximized = maximized;
                if let Err(error) = self.settings_manager.save() {
                    eprintln!("Could not save maximize state: {error}");
                }
            }
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
        // Prefer the saved display selection; if it was disconnected, resolve to the primary
        // display and then the first remaining monitor before correcting the saved position.
        if !self.startup_window_placement_handled {
            let (reported_monitor_size, outer_rect, inner_size, maximized, fullscreen) =
                ctx.input(|input| {
                    let viewport = input.viewport();
                    (
                        viewport.monitor_size,
                        viewport.outer_rect,
                        viewport.inner_rect.map(|rect| rect.size()),
                        viewport.maximized.unwrap_or(false),
                        viewport.fullscreen.unwrap_or(false),
                    )
                });
            if let (Some(outer_rect), Some(inner_size)) = (outer_rect, inner_size) {
                let settings = self.settings_manager.get();
                let monitors = crate::platform::monitors::available_monitors();
                let selected_monitor = crate::platform::monitors::resolve_default_monitor_at(
                    &monitors,
                    &settings.window_default_monitor,
                    Some(outer_rect),
                );
                let monitor_padding = if settings.fit_window_limit == "monitor" {
                    settings.fit_window_monitor_offset.clamp(0.0, 4096.0)
                } else {
                    0.0
                };
                let selected_bounds =
                    selected_monitor.map(|monitor| monitor.inset_bounds(monitor_padding));
                let reported_usable_size = reported_monitor_size.map(|size| {
                    egui::vec2(
                        (size.x - monitor_padding * 2.0).max(100.0),
                        (size.y - monitor_padding * 2.0).max(100.0),
                    )
                });
                let monitor_size = selected_bounds
                    .map(|bounds| bounds.size())
                    .or(reported_usable_size);

                if let Some(monitor_size) = monitor_size {
                    // Do not invent a (0, 0) monitor origin when the platform cannot
                    // enumerate displays. In that case, keep the saved position and
                    // use egui's reported size only for the safety resize below.
                    if let Some(bounds) = selected_bounds {
                        let min_position = bounds.min;
                        let max_position = egui::pos2(
                            (bounds.max.x - outer_rect.width()).max(min_position.x),
                            (bounds.max.y - outer_rect.height()).max(min_position.y),
                        );

                        if settings.start_centered && !maximized {
                            // Center on the selected display, not necessarily the display
                            // that happened to host the initial native window.
                            let pos = bounds.min
                                + ((monitor_size - outer_rect.size()) * 0.5)
                                    .max(egui::Vec2::ZERO);
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                        } else if !maximized {
                            // A saved location on another monitor must not override the
                            // selected default display. If the chosen display disappeared,
                            // resolve_default_monitor_at has already selected the primary or
                            // first remaining display.
                            let center_on_selected = bounds.contains(outer_rect.center());
                            let out_of_bounds = outer_rect.min.x < bounds.min.x - 2.0
                                || outer_rect.min.y < bounds.min.y - 2.0
                                || outer_rect.max.x > bounds.max.x + 2.0
                                || outer_rect.max.y > bounds.max.y + 2.0;
                            if !center_on_selected || out_of_bounds {
                                let pos = if !center_on_selected {
                                    egui::pos2(
                                        bounds.min.x.clamp(min_position.x, max_position.x),
                                        bounds.min.y.clamp(min_position.y, max_position.y),
                                    )
                                } else {
                                    egui::pos2(
                                        outer_rect.min.x.clamp(min_position.x, max_position.x),
                                        outer_rect.min.y.clamp(min_position.y, max_position.y),
                                    )
                                };
                                if (pos - outer_rect.min).length_sq() > 4.0 {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                                }
                            }
                        }
                    }

                    // Only shrink when the actual client size exceeds the selected
                    // monitor. Never resize a window that already fits: this avoids
                    // accumulating native frame/DPI rounding across launches.
                    if !maximized
                        && !fullscreen
                        && (inner_size.x > monitor_size.x + 1.0
                            || inner_size.y > monitor_size.y + 1.0)
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
        // The decode worker sends its result through a channel but cannot request
        // an egui repaint itself. Keep polling while a load is pending so an
        // externally opened image appears as soon as decoding completes, even
        // when the window was otherwise idle.
        if self.receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(32));
        }
        if let Some(rx) = &self.receiver
            && let Ok(result) = rx.try_recv()
        {
            self.receiver = None;

            // Clear old file type detection before loading new image
            self.file_type_detection = None;

            match result {
                Ok(loaded_entry) => {
                    // The shared loader has already identified GIFs from the
                    // source bytes. Seed the look-ahead cache so preload
                    // scheduling does not reopen the same entry for its header.
                    if let Some(image_id) = self.get_image_id(self.current_index)
                        && let Ok(mut cache) = self.gif_detection_cache.lock()
                    {
                        cache.insert(
                            image_id,
                            matches!(
                                &loaded_entry.image,
                                super::types::LoadedImage::Animated(_, _)
                            ),
                        );
                    }
                    // Metadata, format detection, and pixels were all derived
                    // from the same source read by the background loader.
                    self.file_metadata = Some(loaded_entry.metadata);
                    self.set_file_type_detection(loaded_entry.file_type_detection);
                    self.full_gif_source = loaded_entry.full_gif_source;
                    let mut should_spawn_full_gif = false;
                    match loaded_entry.image {
                        super::types::LoadedImage::Static(img) => {
                            // Reuse a preloaded base texture when available. Otherwise
                            // create it once, without egui's synchronous generate_mipmap.
                            // The decoded RGBA allocation is shared with the background
                            // mip and backdrop workers instead of being cloned.
                            let prepared =
                                self.prepare_current_static_image(ctx, self.current_index, img);
                            self.texture = Some(prepared.texture);
                            self.texture_image_id = self.get_image_id(self.current_index);
                            self.cache_tricks_backdrop_from_decoded(
                                self.current_image_path.clone(),
                                prepared.width,
                                prepared.height,
                                prepared.rgba,
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
                        }
                        super::types::LoadedImage::Animated(gif, is_preview) => {
                            self.current_mipmap_state = None;
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
                                    std::sync::Arc::new(pixels),
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
                            self.current_mipmap_state = None;
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
                            self.texture_image_id = None;
                            self.current_mipmap_state = None;
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
                    if self.workspace.should_preload() {
                        self.preload_adjacent_images();
                    }
                    self.mark_interaction();

                    // Now spawn the full GIF loading if needed
                    if should_spawn_full_gif {
                        println!("Spawning full GIF load now...");
                        self.spawn_full_gif_loading();
                        self.set_loading_state(LoadingState::LoadingFullGif);
                    }
                }
                Err(error) => {
                    // The normal loader supplies detection from its shared
                    // byte buffer. Keep the legacy disk check only for the
                    // uncommon decode-failure path so rename suggestions
                    // remain available when a decoder rejects the file.
                    self.detect_from_disk();
                    self.current_mipmap_state = None;
                    self.texture = None;
                    self.texture_image_id = None;
                    self.image_error = Some(error);
                    self.gif_animation = None;
                    self.is_gif = false;
                    self.is_preview = false;
                    self.virtual_texture = None;
                    self.set_loading_state(LoadingState::Idle);

                    eprintln!(
                        "Error loading image: {}",
                        self.image_error.as_ref().unwrap()
                    );
                    ctx.request_repaint();
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
        if let Some(rx) = &self.full_image_receiver
            && let Ok(full_image) = rx.try_recv()
        {
            if !self.is_loading() && self.texture.is_some() {
                // full_image is DecodedImage; refresh the small backdrop source from
                // the full-resolution decoded buffer without reopening the file.
                let width = full_image.width;
                let height = full_image.height;
                let rgba = std::sync::Arc::new(full_image.data);
                let size = [width as usize, height as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_slice());
                self.texture =
                    Some(ctx.load_texture("image_full", color, self.get_texture_base_options()));
                self.texture_image_id = self.get_image_id(self.current_index);
                self.current_mipmap_state =
                    if self.settings_manager.get().texture_filter == "mipmap" {
                        Some(super::texture_mipmap::start_mipmap_upload(
                            width,
                            height,
                            std::sync::Arc::clone(&rgba),
                        ))
                    } else {
                        None
                    };
                self.cache_tricks_backdrop_from_decoded(
                    self.current_image_path.clone(),
                    width,
                    height,
                    rgba,
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
                                    if let Some(metadata) = self.file_metadata.as_mut() {
                                        metadata.frame_count = Some(frame_count);
                                        metadata.animated = Some(full_gif.is_animated());
                                    }
                                    self.full_gif_source = None;
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
                            self.full_gif_source = None;
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
                    self.full_gif_source = None;
                    eprintln!("Full GIF loading task was cancelled");
                }
            }
        }

        // Cancel loading if we navigated away from a GIF preview
        if (!self.is_gif || !self.is_preview) && self.full_gif_receiver.is_some() {
            self.full_gif_receiver = None;
            self.set_loading_state(LoadingState::Idle);
            println!("Cancelled full GIF loading - no longer viewing a GIF preview");
        }

        // ========== GIF ANIMATION ==========
        if self.is_gif {
            self.update_gif_texture(ctx);
        }

        // ========== PRELOAD ==========
        if self.workspace.should_preload()
            && !self.image_entries.is_empty()
            && !self.is_loading()
            && self.image_error.is_none()
        {
            self.preload_adjacent_images();
        }

        // Prepare mipmaps for one cached neighbour at a time after navigation
        // settles. Current-image mip work is prioritized by this scheduler.
        self.prepare_next_cached_mipmap(ctx);

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
        // File-operation confirmations are shared by Viewer, Compare, and Masonry
        // entry context menus, so render them outside the Masonry-only surface.
        self.render_delete_confirmation(ctx);
        self.render_file_conflict_dialog(ctx);
        self.render_workspace_session_prompt(ctx);
        crate::app::ui::render_overlay_ui(self, ctx);

        // Cached-neighbour mip uploads run after visible image/tile callbacks so
        // current-image rendering always gets first claim on the shared budget.
        // A tooltip-order layer ensures these callbacks are still submitted when
        // the central panel is showing a loading/error/virtual-texture branch.
        let background_mipmap_rect = self.main_view_rect(ctx);
        if background_mipmap_rect.is_positive() {
            let background_mipmap_painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("jpeg_viewer_background_mipmap_uploads"),
            ));
            for (texture_id, state) in self.cached_mipmap_uploads_for_frame() {
                super::texture_mipmap::append_mipmap_upload_callback(
                    &background_mipmap_painter,
                    background_mipmap_rect,
                    texture_id,
                    state,
                    std::sync::Arc::clone(&self.mipmap_upload_budget),
                    ctx.clone(),
                );
            }
        }

        // Keep the active Image tab's pop-out animation in the same playback
        // speed domain as its source. Each pop-out retains a private decoder state
        // so it continues looping while another workspace tab is active.
        if let Some(tab_id) = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .filter(|tab| tab.kind == super::workspace::WorkspaceTabKind::Image)
            .map(|tab| tab.id)
        {
            self.popouts.sync_active_animation(tab_id, self.gif_animation.as_ref());
        }

        // Workspace activation and tab creation happen during UI rendering;
        // update the native title after those interactions in the same frame.
        self.update_window_title(ctx);

        // Empty Viewer tabs deliberately remain empty; the launch/recent UI is
        // rendered only by an explicit Welcome workspace tab.

        // ========== FRAME LIMITER ==========
        self.schedule_repaint(ctx);
        if self.preload_working || self.is_loading() {
            ctx.request_repaint();
        }

        // ========== CLEANUP ==========
        if ctx.input(|i| i.viewport().close_requested()) {
            self.stop_caching();
            self.save_workspace_session();
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

        self.popouts.render_viewports(ctx);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // Pop-out viewports are transparent overlays. eframe uses this shared
        // clear color for its immediate child viewports too, so keep the clear
        // buffer transparent while any overlay exists. The main UI paints its
        // own panel surfaces over the clear buffer.
        if self.popouts.has_active_popouts()
            || (self.settings_manager.get().os_background_blur
            && crate::ui_theme::platform_backdrop::is_active())
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
        // Wayland backdrop proxies borrow eframe's display/surface, so release
        // them before the native window and its display connection are torn down.
        crate::ui_theme::platform_backdrop::shutdown();
        if let Some(handle) = self.virtual_texture_thread.take() {
            let _ = handle.join();
        }
        if let Err(e) = self.settings_manager.save_if_dirty() {
            eprintln!("Failed to save settings: {}", e);
        }
        crate::io_metrics::report();
    }
}

impl ViewerApp {
    pub fn update_gif_texture(&mut self, ctx: &egui::Context) {
        self.current_mipmap_state = None;
        let options = self.get_texture_base_options();
        // Resolve the identity before borrowing `gif_animation` mutably.
        let current_image_id = self.get_image_id(self.current_index);
        let mut texture_updated = false;

        // Keep the mutable borrow of the GIF animation scoped to frame extraction and
        // playback scheduling. Dial-up state needs `&mut self`, so update it only after
        // the GIF borrow has ended.
        if let Some(gif) = &mut self.gif_animation
            && let Some(frame) = gif.get_current_frame()
        {
            let size = [frame.width() as usize, frame.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());

            self.texture = Some(ctx.load_texture("gif_frame", color_image, options));
            self.texture_image_id = current_image_id;
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

        if texture_updated {
            self.mark_win98_dialup_texture_ready(false);
        }
    }
}
