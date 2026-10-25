use crate::app::types::{LoadingState, ViewerApp};
use eframe::egui;

impl ViewerApp {
    pub fn render_central_panel(&mut self, ctx: &egui::Context) {
        let style = ctx.style();
        let frame = egui::Frame::central_panel(&style).inner_margin(0.0);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Check for error first
            let error = self.image_error.clone();
            if let Some(error) = error {
                self.render_error_ui(ui, &error);
                return;
            }

            // ---------- VIRTUAL TEXTURE LOADING STATE ----------
            // Check if virtual texture is loading in background
            if self.is_loading_virtual() {
                let (total_tiles, prepared_tiles, progress_percent, is_extreme) = {
                    if let Some(ref progress) = self.vt_progress {
                        let total = self.vt_total_tiles;
                        let prepared = progress.prepared_tiles;
                        let progress_val = if total > 0 {
                            prepared as f32 / total as f32
                        } else {
                            0.0
                        };

                        // Check if this is an extreme aspect ratio
                        // b_fit_to_window will be false and b_zoom_used true for extreme ratios
                        let extreme = !self.b_fit_to_window && self.b_zoom_used;
                        (total, prepared, progress_val, extreme)
                    } else {
                        (0, 0, 0.0, false)
                    }
                };

                let message = if total_tiles > 0 && prepared_tiles > 0 {
                    format!(
                        "Loading large image... ({}/{})",
                        prepared_tiles, total_tiles
                    )
                } else {
                    "Loading large image...".to_string()
                };

                self.render_loading_ui(ui, &message, Some(progress_percent), is_extreme);
                return;
            }

            // Get the available rect
            let available_rect = ui.available_rect_before_wrap();
            let center = available_rect.center();

            if self.texture.is_none() && self.virtual_texture.is_none() {
                // Handle loading states when no texture and no virtual texture
                if self.is_loading() {
                    let painter = ui.painter();
                    let message = match self.loading_state {
                        LoadingState::Indexing => "Indexing...",
                        LoadingState::LoadingFullGif => "Loading full GIF...",
                        LoadingState::VirtualTextureLoading => "Loading large image...",
                        _ => "Loading...",
                    };
                    Self::draw_loading_overlay(&painter, available_rect, message);
                    return;
                }
                let error = self.image_error.clone();
                if let Some(error) = error {
                    self.render_error_ui(ui, &error);
                    return;
                }
                if self.image_entries.is_empty() {
                    self.render_welcome_ui(ctx, ui);
                    return;
                } else if self.is_gif {
                    self.render_loading_ui(ui, "Loading GIF frame...", None, false);
                    return;
                }
                return;
            }

            // ---------- VIRTUAL TEXTURE RENDERING ----------
            if self.virtual_texture.is_some() {
                let vt = self.virtual_texture.as_mut().unwrap();

                // Check if virtual texture is ready
                if !vt.is_ready() {
                    let total_tiles = vt.total_tiles();
                    let prepared_tiles = vt.prepared_tiles_count();
                    let progress_percent = if total_tiles > 0 {
                        prepared_tiles as f32 / total_tiles as f32
                    } else {
                        0.0
                    };

                    // Check if this is an extreme aspect ratio
                    let is_extreme = !self.b_fit_to_window && self.b_zoom_used;

                    let message = if total_tiles > 0 && prepared_tiles > 0 {
                        format!(
                            "Loading large image... ({}/{})",
                            prepared_tiles, total_tiles
                        )
                    } else {
                        "Loading large image...".to_string()
                    };

                    self.render_loading_ui(ui, &message, Some(progress_percent), is_extreme);
                    return;
                }

                // Get dimensions from virtual texture
                let (vt_width, vt_height) = vt.dimensions();
                let texture_size = egui::vec2(vt_width as f32, vt_height as f32);
                // Update fit-to-window if needed for virtual texture
                if self.b_fit_to_window {
                    let available = ui.available_size();
                    self.calculate_fit_zoom(texture_size, available);
                    self.b_fit_to_window = false;
                }

                // Render using virtual texture
                let available_rect = ui.available_rect_before_wrap();

                // Create the painter and immediately use it in a limited scope
                let texture_options = self.get_texture_options();

                if let Some(vt) = &mut self.virtual_texture {
                    let painter = ui.painter();

                    vt.render(
                        ctx,
                        &painter,
                        self.zoom,
                        self.pan,
                        available_rect,
                        texture_options,
                    );
                }

                // Allocate the same rect for interaction (drag/pan)
                let response = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());

                self.handle_image_mouse_input(ctx, &response);

                // Show loading indicator overlay if still loading
                if self.is_loading() {
                    let painter = ui.painter();
                    Self::draw_loading_overlay(&painter, available_rect, "Loading...");
                }
                return;
            }

            // ---------- NORMAL TEXTURE RENDERING ----------
            // Use a block to limit the borrow of texture
            let (texture_size, texture_id) = {
                if let Some(texture) = &self.texture {
                    (texture.size_vec2(), texture.id())
                } else {
                    // No texture available
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new("No image loaded").size(24.0));
                    });
                    return;
                }
            };

            if self.b_fit_to_window {
                let available = ui.available_size();
                self.calculate_fit_zoom(texture_size, available);
                self.b_fit_to_window = false;
            }

            let base_image_rect = self.get_image_rect(texture_size, center);
            let mut image_rect = base_image_rect;
            let mut image_tint = egui::Color32::WHITE;
            let mut outgoing_frame: Option<(egui::TextureHandle, egui::Rect, egui::Color32)> = None;

            if self.slideshow_enabled {
                // Image motion is a per-image presentation effect. It is not
                // used as the transition itself; transitions happen only after
                // the next image has actually become available.
                let interval = self.slideshow_interval.as_secs_f32().max(0.5);
                let motion_progress = (self.slideshow_last_advance.elapsed().as_secs_f32() / interval).clamp(0.0, 1.0);
                let effect = if self.slideshow_motion == "random" {
                    match self.current_index % 5 {
                        0 => crate::slideshow::MotionEffect::PanHorizontal,
                        1 => crate::slideshow::MotionEffect::PanVertical,
                        2 => crate::slideshow::MotionEffect::ZoomIn,
                        3 => crate::slideshow::MotionEffect::ZoomOut,
                        _ => crate::slideshow::MotionEffect::Smart,
                    }
                } else {
                    crate::slideshow::MotionEffect::from_setting(&self.slideshow_motion)
                };
                let frame = crate::slideshow::transform(
                    base_image_rect,
                    available_rect,
                    motion_progress,
                    effect,
                    self.slideshow_motion_strength,
                    false,
                );
                image_rect = frame.rect;

                if motion_progress < 1.0 {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }

                // Cross-image transition lasts a short, fixed period after the
                // incoming image is ready, rather than over its whole display interval.
                const TRANSITION_DURATION: std::time::Duration = std::time::Duration::from_millis(450);
                if let (Some(previous), Some(started)) = (
                    self.slideshow_previous_texture.clone(),
                    self.slideshow_transition_started,
                ) {
                    if previous.id() != texture_id {
                        let elapsed = started.elapsed();
                        if elapsed < TRANSITION_DURATION {
                            let t = (elapsed.as_secs_f32() / TRANSITION_DURATION.as_secs_f32()).clamp(0.0, 1.0);
                            let transition = match self.slideshow_transition.as_str() {
                                "slide" => "slide",
                                "random" if self.current_index % 2 == 1 => "slide",
                                "none" => "none",
                                _ => "fade",
                            };
                            if transition == "slide" {
                                let offset = available_rect.width() * t;
                                let old_size = previous.size_vec2();
                                let old_scale = (available_rect.width() / old_size.x.max(1.0))
                                    .min(available_rect.height() / old_size.y.max(1.0));
                                // Prefer the outgoing image's exact last-rendered
                                // rectangle so its zoom and pan don't snap to the
                                // incoming image's view settings.
                                let old_rect = self.slideshow_previous_rect.unwrap_or_else(|| {
                                    egui::Rect::from_center_size(
                                        available_rect.center(),
                                        old_size * old_scale,
                                    )
                                }).translate(egui::vec2(-offset, 0.0));
                                image_rect = frame.rect.translate(egui::vec2(available_rect.width() * (1.0 - t), 0.0));
                                outgoing_frame = Some((previous, old_rect, egui::Color32::WHITE));
                            } else if transition == "fade" {
                                let old_size = previous.size_vec2();
                                let old_scale = (available_rect.width() / old_size.x.max(1.0))
                                    .min(available_rect.height() / old_size.y.max(1.0));
                                let old_rect = self.slideshow_previous_rect.unwrap_or_else(|| {
                                    egui::Rect::from_center_size(
                                        available_rect.center(),
                                        old_size * old_scale,
                                    )
                                });
                                outgoing_frame = Some((
                                    previous,
                                    old_rect,
                                    egui::Color32::from_white_alpha(((1.0 - t) * 255.0).round() as u8),
                                ));
                                image_tint = egui::Color32::from_white_alpha((t * 255.0).round() as u8);
                            }
                            ctx.request_repaint_after(std::time::Duration::from_millis(16));
                        } else {
                            self.slideshow_previous_texture = None;
                            self.slideshow_previous_rect = None;
                            self.slideshow_transition_started = None;
                        }
                    }
                }
            } else {
                self.slideshow_previous_texture = None;
                self.slideshow_previous_rect = None;
                self.slideshow_transition_started = None;
            }
            // Remember the incoming image's actual rendered rectangle for the
            // next navigation event. During a slide this may include the
            // temporary offset; once the transition completes, the normal
            // motion rectangle is stored again.
            self.image_rect = Some(image_rect);
            let response = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());

            let painter = ui.painter();
            if let Some((previous_texture, previous_rect, previous_tint)) = outgoing_frame {
                painter.image(
                    previous_texture.id(),
                    previous_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    previous_tint,
                );
            }
            painter.image(
                texture_id,
                image_rect,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                image_tint,
            );

            // Crop selection owns primary-button drags while editing. Other
            // mouse bindings remain available when the crop tool is inactive.
            if self.show_edit_menu && self.edit_session.is_some() {
                if self.crop_tool_active {
                    self.handle_crop_mouse_input(ctx, &response, image_rect);
                } else {
                    self.handle_image_mouse_input(ctx, &response);
                }
                if self.crop_tool_active || self.crop_selection.is_some() {
                    self.paint_crop_selection(ui.painter(), image_rect);
                }
            } else {
                self.handle_image_mouse_input(ctx, &response);
            }

            if self.is_loading() {
                let painter = ui.painter();
                let message = match self.loading_state {
                    LoadingState::LoadingFullGif => "Loading full GIF...",
                    LoadingState::VirtualTextureLoading => "Loading large image...",
                    _ => "Loading...",
                };
                Self::draw_loading_overlay(&painter, available_rect, message);
            }
        });
    }

    // New unified loading UI with optional progress and extreme ratio indicator
    fn render_loading_ui(
        &self,
        ui: &mut egui::Ui,
        message: &str,
        progress: Option<f32>,
        show_extreme_ratio: bool,
    ) {
        let available = ui.available_rect_before_wrap();
        let rect = Self::loading_content_rect(available);

        ui.allocate_rect(rect, egui::Sense::hover());

        let painter = ui.painter();

        painter.rect_filled(rect, 12.0, ui.style().visuals.panel_fill);

        Self::draw_loading_content(
            &painter,
            rect.center(),
            message,
            progress,
            show_extreme_ratio,
            ui.style(),
        );
    }

    fn draw_loading_content(
        painter: &egui::Painter,
        center: egui::Pos2,
        message: &str,
        progress: Option<f32>,
        show_extreme_ratio: bool,
        style: &egui::Style,
    ) {
        const SPINNER_SIZE: f32 = 22.0;
        //const TEXT_HEIGHT: f32 = 16.0;
        //const PROGRESS_HEIGHT: f32 = 16.0;

        let time = painter.ctx().input(|i| i.time);

        // -------------------------
        // Spinner
        // -------------------------

        let angle = (time * 3.0) as f32;
        let radius = SPINNER_SIZE * 0.28;
        let segments = 8;

        let spinner_center = egui::pos2(center.x - 76.0, center.y);

        for i in 0..segments {
            let angle_offset = (i as f32 / segments as f32) * std::f32::consts::TAU;

            let alpha = ((0.3 + 0.7 * ((time as f32 * 2.0 + angle_offset).sin() * 0.5 + 0.5))
                * 255.0) as u8;

            let x = spinner_center.x + radius * (angle + angle_offset).cos();

            let y = spinner_center.y + radius * (angle + angle_offset).sin();

            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(6.0, 6.0)),
                3.0,
                egui::Color32::from_rgba_premultiplied(100, 150, 255, alpha),
            );
        }

        // -------------------------
        // Message
        // -------------------------

        let font_id = egui::FontId::proportional(12.0);
        let text_color = style.visuals.text_color();

        let galley = painter.layout(message.to_owned(), font_id, text_color, f32::INFINITY);

        let text_y = center.y - galley.rect.height() / 2.0;

        painter.galley(egui::pos2(center.x - 60.0, text_y), galley, text_color);

        // -------------------------
        // Progress bar
        // -------------------------

        if let Some(progress) = progress {
            if progress > 0.0 {
                let progress = progress.clamp(0.0, 1.0);

                let bar_rect = egui::Rect::from_center_size(
                    egui::pos2(center.x + 24.0, center.y + 13.0),
                    egui::vec2(112.0, 4.0),
                );

                painter.rect_filled(bar_rect, 4.0, style.visuals.widgets.noninteractive.bg_fill);

                let fill_rect = egui::Rect::from_min_size(
                    bar_rect.min,
                    egui::vec2(bar_rect.width() * progress, bar_rect.height()),
                );

                painter.rect_filled(fill_rect, 4.0, style.visuals.widgets.active.bg_fill);

                // Percentage

                let percent = (progress * 100.0) as u32;

                let percent_galley = painter.layout(
                    format!("{}%", percent),
                    egui::FontId::proportional(10.0),
                    text_color,
                    f32::INFINITY,
                );

                painter.galley(
                    egui::pos2(
                        center.x + 24.0 - percent_galley.rect.width() / 2.0,
                        bar_rect.max.y + 2.0,
                    ),
                    percent_galley,
                    text_color,
                );

                // Extreme aspect ratio message

                if show_extreme_ratio {
                    let galley = painter.layout(
                        "Extreme aspect ratio - showing at 1:1".to_string(),
                        egui::FontId::proportional(12.0),
                        text_color,
                        f32::INFINITY,
                    );

                    painter.galley(
                        egui::pos2(center.x - galley.rect.width() / 2.0, bar_rect.max.y + 12.0),
                        galley,
                        text_color,
                    );
                }
            }
        }
    }

    // loading...
    fn draw_loading_overlay(painter: &egui::Painter, rect: egui::Rect, message: &str) {
        let content_rect = Self::loading_content_rect(rect);
        let center = content_rect.center();

        let panel_fill = painter.ctx().style().visuals.panel_fill;
        let semi_transparent = panel_fill.linear_multiply(0.85);

        painter.rect_filled(content_rect, 12.0, semi_transparent);

        Self::draw_loading_content(
            painter,
            center,
            message,
            None,
            false,
            &painter.ctx().style(),
        );
    }

    fn loading_content_rect(rect: egui::Rect) -> egui::Rect {
        const CONTENT_WIDTH: f32 = 190.0;
        const CONTENT_HEIGHT: f32 = 48.0;
        let margin = egui::vec2(12.0, 12.0);
        let pos = egui::pos2(
            rect.right() - margin.x - CONTENT_WIDTH / 2.0,
            rect.top() + margin.y + CONTENT_HEIGHT / 2.0,
        );
        egui::Rect::from_center_size(pos, egui::vec2(CONTENT_WIDTH, CONTENT_HEIGHT))
    }
}
