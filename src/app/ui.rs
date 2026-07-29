//this needs to be splitted
use super::types::ViewerApp;
use crate::shortcuts::{ViewerCommand, handle_keyboard, handle_mouse};
use eframe::egui;

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

        for file in dropped_files {
            if let Some(path) = file.path {
                self.open_path(path);
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

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }

                ui.label(format!(
                    "Zoom: {}%",
                    (self.zoom * 100.0).round().max(1.0) as i32
                ));

                // Cache radius control
                ui.add_space(10.0);
                ui.label("Radius:");
                let mut radius = self.cache_radius;
                if ui
                    .add(
                        egui::DragValue::new(&mut radius)
                            .range(1..=100)
                            .speed(1)
                            .prefix(" "),
                    )
                    .changed()
                {
                    if radius != self.cache_radius {
                        self.update_cache_radius(radius);
                        // Save to settings
                        let _ = self.settings_manager.update(|settings| {
                            settings.cache_radius = radius;
                        });
                        // Trigger preload with new radius
                        if !self.image_entries.is_empty() && !self.b_is_loading {
                            self.preload_adjacent_images(ctx);
                        }
                    }
                }
                ui.label(" | ");

                // Show cache info with delta info
                let total_images = self.image_entries.len();
                let cached_count = self.image_cache.len();
                let cache_range = self.get_cache_range();
                let target_count = (cache_range * 2 + 1).min(total_images);
                ui.label(format!(
                    "Cache: {}/{} (r:{}, Δ:{})",
                    cached_count, target_count, self.cache_radius, self.delta_threshold
                ));

                // GIF controls
                if self.is_gif {
                    if let Some(gif) = &mut self.gif_animation {
                        // Check if it's an animated GIF (more than 1 frame)
                        if gif.is_animated() {
                            ui.add_space(10.0);
                            ui.label("GIF:");

                            if ui.button(if gif.is_playing { "⏸" } else { "▶" }).clicked() {
                                gif.toggle_play();
                            }

                            ui.label("Speed:");
                            let mut speed = gif.speed_multiplier;
                            let speed_slider = egui::Slider::new(&mut speed, 0.1..=10.0)
                                .logarithmic(true)
                                .text("x")
                                .smallest_positive(0.1)
                                .step_by(0.01);

                            if ui.add(speed_slider).changed() {
                                gif.set_speed(speed);
                            }

                            ui.label(format!(
                                "Frame {}/{}",
                                gif.get_current_frame_index() + 1,
                                gif.frame_count()
                            ));

                            // Show loading message for GIF preview
                            if self.is_preview {
                                ui.label("⏳ Loading GIF...");
                            }

                            ui.add_space(10.0);
                            ui.label("|");
                        } else {
                            // Single frame GIF or still loading
                            ui.add_space(10.0);
                            ui.label("GIF:");

                            // Show loading message for GIF preview
                            if self.is_preview {
                                ui.label("⏳ Loading GIF...");
                            } else {
                                ui.label("Static");
                            }

                            ui.add_space(10.0);
                            ui.label("|");
                        }
                    }
                }

                if ui
                    .checkbox(&mut self.b_ctrl_invert, "Scroll Zoom")
                    .changed()
                {
                    let _ = self.settings_manager.update(|settings| {
                        settings.b_ctrl_invert = self.b_ctrl_invert;
                    });
                }

                //My very first option! :)
                if !self.b_ctrl_invert {
                    ui.label(" | Scroll to navigate & ctrl + Scroll to Zoom | ");
                } else {
                    ui.label(" | ctrl + Scroll to navigate & Scroll to Zoom | ");
                }

                //ui for quality of filtering
                ui.add_space(10.0);
                ui.label("Filter:");

                // Get current filter value
                let mut filter = self.settings_manager.get().texture_filter.clone();

                // Show combo box
                egui::ComboBox::from_label("")
                    .selected_text(&filter)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut filter, "nearest".to_string(), "Nearest (fast)");
                        ui.selectable_value(&mut filter, "linear".to_string(), "Linear (smooth)");
                        ui.selectable_value(&mut filter, "mipmap".to_string(), "Mipmap (best)");
                    });

                // Check if filter changed by comparing with stored value
                let current_filter = self.settings_manager.get().texture_filter.clone();
                if filter != current_filter {
                    // Save the new setting
                    let _ = self.settings_manager.update(|settings| {
                        settings.texture_filter = filter.clone();
                    });
                    // Reload current image to apply the new filter
                    if !self.image_entries.is_empty() {
                        self.load_current_image_with_cache();
                    }
                }

                // Loading indicator for full resolution
                if self.b_is_loading_full {
                    ui.label("Loading...");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                let texture_size = texture.size_vec2();
                let available = ui.available_size();

                // Only apply fit if b_fit_to_window is true AND not an extreme aspect ratio
                if self.b_fit_to_window {
                    // Check if we should actually apply fit
                    let should_fit = if let Some(texture) = &self.texture {
                        let tex_size = texture.size_vec2();
                        // Check if current texture has extreme aspect ratio
                        // We store this info or check dynamically
                        let width = tex_size.x;
                        let height = tex_size.y;
                        let ratio = if width > height {
                            height / width
                        } else {
                            width / height
                        };
                        // Only apply fit if ratio is >= 0.1
                        ratio >= 0.1
                    } else {
                        true
                    };

                    if should_fit {
                        let zoom_x = available.x / texture_size.x;
                        let zoom_y = available.y / texture_size.y;
                        let fit_zoom = zoom_x.min(zoom_y).min(1.0);
                        self.zoom = fit_zoom;
                        self.pan = egui::Vec2::ZERO;
                        self.b_fit_to_window = false;
                    } else {
                        // For extreme ratios, ensure zoom is 1.0
                        self.zoom = 1.0;
                        self.pan = egui::Vec2::ZERO;
                        self.b_fit_to_window = false;
                    }
                }

                let display_size = texture_size * self.zoom;

                // Center the image in the available space
                let center = ui.available_rect_before_wrap().center();
                let image_rect =
                    egui::Rect::from_center_size(center + self.pan * self.zoom, display_size);

                // Allocate the full available space for interaction
                let response =
                    ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());

                // Paint the image centered in the allocated space using ui.painter()
                let painter = ui.painter();
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    egui::Color32::WHITE,
                );

                // Draw loading indicator overlay for GIFs
                if self.is_gif && self.is_preview {
                    let painter = ui.painter();
                    let text = "Loading GIF...".to_string();
                    let font_id = egui::FontId::proportional(20.0);
                    let galley = painter.layout(text, font_id, egui::Color32::WHITE, f32::INFINITY);
                    let rect = galley.rect;

                    let bg_rect = rect.expand(15.0);
                    painter.rect_filled(
                        bg_rect,
                        8.0,
                        egui::Color32::from_rgba_premultiplied(0, 0, 0, 200),
                    );

                    painter.galley(
                        egui::pos2(
                            response.rect.center().x - rect.width() / 2.0,
                            response.rect.center().y - rect.height() / 2.0,
                        ),
                        galley,
                        egui::Color32::WHITE,
                    );
                }

                // Handle mouse input on the response
                for command in handle_mouse(
                    ctx,
                    &self.input_bindings,
                    response.hovered(),
                    self.b_ctrl_invert,
                ) {
                    self.handle_command(ctx, command);
                }

                // Check for middle drag separately from other drags
                if response.hovered() {
                    let mid_dragging = ctx.input(|i| {
                        i.pointer.button_down(egui::PointerButton::Middle)
                            && i.pointer.delta().length() > 0.0
                    });

                    if mid_dragging {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }

                // Then handle left/right drags
                if response.dragged() {
                    let (left_down, right_down, delta) = ctx.input(|i| {
                        (
                            i.pointer.button_down(egui::PointerButton::Primary),
                            i.pointer.button_down(egui::PointerButton::Secondary),
                            i.pointer.delta(),
                        )
                    });

                    // Only handle left/right drags, skip middle
                    if left_down && !right_down {
                        self.pan += delta / self.zoom;
                        // ... pan logic
                    }

                    if right_down && !left_down {
                        self.zoom += delta.y * -0.005;
                        self.zoom = self.zoom.clamp(0.005, 50.0);
                        self.b_zoom_used = true;
                    }
                }
            } else if self.is_gif {
                // GIF is loading but texture not ready yet
                if self.b_is_loading {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading GIF...");
                    });
                } else if self.texture.is_none() && self.is_gif {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading GIF frame...");
                    });
                }
            } else {
                if self.b_is_loading {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading image...");
                    });
                } else {
                    let available = ui.available_height();
                    let content_height = 128.0 + 16.0 + 60.0;

                    ui.add_space((available - content_height).max(0.0) * 0.5);

                    ui.vertical_centered(|ui| {
                        if let Some(icon) = &self.logo_texture {
                            ui.image((icon.id(), egui::vec2(256.0, 256.0)));
                            ui.add_space(16.0);
                        }

                        ui.label(
                            egui::RichText::new(
                                "Press Ctrl+O or drag and drop a photo, folder\n\
                                 or a .zip, .7z, or .rar archive containing your photos.",
                            )
                            .size(24.0),
                        );
                    });
                }
            }
        });

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
