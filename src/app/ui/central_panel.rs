use crate::app::types::{LoadingState, ViewerApp};
use crate::ui_theme::scaled_text_size;
use eframe::egui;

impl ViewerApp {
    pub fn render_central_panel(&mut self, ctx: &egui::Context) {
        let style = ctx.style();
        let settings = self.settings_manager.get().clone();
        let mut view_bg =
            parse_view_background(&settings.main_view_background, style.visuals.dark_mode);
        if crate::ui_theme::os_background_blur_enabled(ctx) {
            let [r, g, b, _] = view_bg.to_srgba_unmultiplied();
            let opacity = (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0)
                .round() as u8;
            view_bg = egui::Color32::from_rgba_unmultiplied(r, g, b, opacity);
        }
        let frame = egui::Frame::central_panel(&style)
            .inner_margin(0.0)
            .fill(view_bg);
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

            // Get the available rect. Platform-style fit mode reserves a little space
            // around the image and for the top/bottom centered toolbar.
            let available_rect = ui.available_rect_before_wrap();
            let image_viewport_rect = fitted_image_viewport_rect(
                available_rect,
                ctx,
                &settings,
                !self.b_zoom_used,
            );

            if self.texture.is_none() && self.virtual_texture.is_none() {
                // Handle loading states when no texture and no virtual texture
                if self.is_loading() {
                    if !crate::app::ui::uses_desktop_menu_bar(ctx) {
                        let painter = ui.painter();
                        let message = match self.loading_state {
                            LoadingState::Indexing => "Indexing...",
                            LoadingState::LoadingFullGif => "Loading full GIF...",
                            LoadingState::VirtualTextureLoading => "Loading large image...",
                            _ => "Loading...",
                        };
                        Self::draw_loading_overlay(&painter, available_rect, message);
                    }
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
                    self.calculate_fit_zoom(texture_size, image_viewport_rect.size());
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
                        image_viewport_rect,
                        texture_options,
                    );
                }

                // Allocate the same rect for interaction (drag/pan)
                let response = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());

                self.handle_image_mouse_input(ctx, &response);

                // Show loading indicator overlay if still loading
                if self.is_loading() && !crate::app::ui::uses_desktop_menu_bar(ctx) {
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
                self.calculate_fit_zoom(texture_size, image_viewport_rect.size());
                self.b_fit_to_window = false;
            }

            let base_image_rect = self.get_image_rect(texture_size, image_viewport_rect.center());
            let mut image_rect = base_image_rect;
            let mut image_tint = egui::Color32::WHITE;
            let mut outgoing_frame: Option<(egui::TextureHandle, egui::Rect, egui::Color32)> = None;

            if self.slideshow_enabled {
                // Image motion is a per-image presentation effect. It is not
                // used as the transition itself; transitions happen only after
                // the next image has actually become available.
                let interval = self.slideshow_interval.as_secs_f32().max(0.5);
                let motion_progress = (self.slideshow_last_advance.elapsed().as_secs_f32()
                    / interval)
                    .clamp(0.0, 1.0);
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
                    image_viewport_rect,
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
                const TRANSITION_DURATION: std::time::Duration =
                    std::time::Duration::from_millis(450);
                if let (Some(previous), Some(started)) = (
                    self.slideshow_previous_texture.clone(),
                    self.slideshow_transition_started,
                ) {
                    if previous.id() != texture_id {
                        let elapsed = started.elapsed();
                        if elapsed < TRANSITION_DURATION {
                            let t = (elapsed.as_secs_f32() / TRANSITION_DURATION.as_secs_f32())
                                .clamp(0.0, 1.0);
                            let transition = match self.slideshow_transition.as_str() {
                                "slide" => "slide",
                                "random" if self.current_index % 2 == 1 => "slide",
                                "none" => "none",
                                _ => "fade",
                            };
                            if transition == "slide" {
                                let offset = image_viewport_rect.width() * t;
                                let old_size = previous.size_vec2();
                                let old_scale = (image_viewport_rect.width() / old_size.x.max(1.0))
                                    .min(image_viewport_rect.height() / old_size.y.max(1.0));
                                // Prefer the outgoing image's exact last-rendered
                                // rectangle so its zoom and pan don't snap to the
                                // incoming image's view settings.
                                let old_rect = self
                                    .slideshow_previous_rect
                                    .unwrap_or_else(|| {
                                        egui::Rect::from_center_size(
                                            image_viewport_rect.center(),
                                            old_size * old_scale,
                                        )
                                    })
                                    .translate(egui::vec2(-offset, 0.0));
                                image_rect = frame
                                    .rect
                                    .translate(egui::vec2(image_viewport_rect.width() * (1.0 - t), 0.0));
                                outgoing_frame = Some((previous, old_rect, egui::Color32::WHITE));
                            } else if transition == "fade" {
                                let old_size = previous.size_vec2();
                                let old_scale = (image_viewport_rect.width() / old_size.x.max(1.0))
                                    .min(image_viewport_rect.height() / old_size.y.max(1.0));
                                let old_rect = self.slideshow_previous_rect.unwrap_or_else(|| {
                                    egui::Rect::from_center_size(
                                        image_viewport_rect.center(),
                                        old_size * old_scale,
                                    )
                                });
                                outgoing_frame = Some((
                                    previous,
                                    old_rect,
                                    egui::Color32::from_white_alpha(
                                        ((1.0 - t) * 255.0).round() as u8
                                    ),
                                ));
                                image_tint =
                                    egui::Color32::from_white_alpha((t * 255.0).round() as u8);
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
            let dialup_settings = self.settings_manager.get().clone();
            let current_profile = crate::ui_theme::current_look_profile(ctx);
            let dialup_active = current_profile == "win98" && dialup_settings.win98_dialup_enabled;
            // Dial-up replaces the checkerboard with the progressive-download effect;
            // keep the user's checkerboard preference intact for when Dial-up is disabled.
            if settings.show_alpha_checkerboard && !self.slideshow_enabled && !dialup_active {
                paint_checkerboard(
                    painter,
                    image_rect,
                    settings.alpha_checkerboard_size,
                    settings.alpha_checkerboard_brightness,
                    style.visuals.dark_mode,
                );
            }
            if let Some((previous_texture, previous_rect, previous_tint)) = outgoing_frame {
                painter.image(
                    previous_texture.id(),
                    previous_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    previous_tint,
                );
            }
            if dialup_active && self.win98_dialup_reveal_started.is_some() {
                let elapsed = self.win98_dialup_reveal_started.unwrap().elapsed().as_secs_f32();
                let kbps = dialup_settings.win98_dialup_connection_kbps.clamp(1, 56) as f32;
                // 28.8 kbps takes about 2.5 seconds. Every strip starts only after
                // the prior strip's scheduled start, with bounded per-row timing jitter.
                let duration = (2.5 * 28.8 / kbps).clamp(1.25, 18.0);
                // Bound the scanline count so each complete strip is visible before
                // the next begins, even at 60 Hz. Very thin rows would otherwise be
                // scheduled only a few milliseconds apart and appear in batches.
                let band_height = (image_rect.height() / 100.0).max(6.0_f32);
                let full_uv = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE);
                let visible_rect = image_rect.intersect(painter.clip_rect());
                if !visible_rect.is_negative() {
                    let row_count = (image_rect.height() / band_height).ceil().max(1.0) as usize;
                    let first_row = (((visible_rect.top() - image_rect.top()) / band_height).floor().max(0.0) as usize).min(row_count);
                    let last_row = (((visible_rect.bottom() - image_rect.top()) / band_height).ceil().max(0.0) as usize).min(row_count);
                    let total_weight = (0..row_count).map(dialup_row_weight).sum::<f32>().max(1.0);
                    let mut elapsed_weight = 0.0_f32;
                    // Iterate from the first row even when the image is scrolled: earlier
                    // rows still contribute to the schedule, so reveal order never reverses.
                    for row in 0..last_row {
                        let reveal_at = duration * (elapsed_weight / total_weight);
                        if row >= first_row && elapsed >= reveal_at {
                            let y0 = image_rect.top() + row as f32 * band_height;
                            let y1 = (y0 + band_height).min(image_rect.bottom());
                            let strip = egui::Rect::from_min_max(
                                egui::pos2(image_rect.left(), y0),
                                egui::pos2(image_rect.right(), y1),
                            );
                            painter.with_clip_rect(strip).image(texture_id, image_rect, full_uv, image_tint);
                        }
                        elapsed_weight += dialup_row_weight(row);
                    }
                }
                if elapsed < duration + 0.07 {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                } else {
                    self.win98_dialup_reveal_started = None;
                    self.win98_dialup_previous_texture_id = None;
                }
            } else {
                // Turning the effect off cancels any in-progress reveal.
                if !dialup_settings.win98_dialup_enabled || current_profile != "win98" {
                    self.win98_dialup_reveal_started = None;
                    self.win98_dialup_reveal_pending = false;
                    self.win98_dialup_previous_texture_id = None;
                }
                painter.image(
                    texture_id,
                    image_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    image_tint,
                );
            }

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

            if self.is_loading() && !crate::app::ui::uses_desktop_menu_bar(ctx) {
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
        // In desktop-menu-bar mode the single loading indicator lives on the
        // far-right side of the bar instead of covering the image viewport.
        if crate::app::ui::uses_desktop_menu_bar(ui.ctx()) {
            return;
        }
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

        let font_id = egui::FontId::proportional(scaled_text_size(painter.ctx(), 12.0));
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
                    egui::FontId::proportional(scaled_text_size(painter.ctx(), 10.0)),
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
                        egui::FontId::proportional(scaled_text_size(painter.ctx(), 12.0)),
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

/// Deterministic high-variance reveal timing for a single Dial-up scanline.
/// Every weight remains positive, so later rows never start before prior rows;
/// the broad 0.25x–2.0x range makes simulated modem fluctuations much more visible.
fn dialup_row_weight(row: usize) -> f32 {
    let mut noise = (row as u32)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    noise ^= noise >> 13;
    noise = noise.wrapping_mul(1_274_126_177);
    noise ^= noise >> 16;
    let jitter_unit = (noise & 0xFFFF) as f32 / 65_535.0;
    0.25 + jitter_unit * 1.75
}

fn parse_view_background(value: &str, dark: bool) -> egui::Color32 {
    if value == "light" {
        return egui::Color32::from_rgb(245, 245, 245);
    }
    if value == "theme" {
        return if dark {
            egui::Color32::from_rgb(18, 18, 18)
        } else {
            egui::Color32::from_rgb(238, 238, 238)
        };
    }
    if let Some(hex) = value.strip_prefix("custom:#") {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return egui::Color32::from_rgb(r, g, b);
            }
        }
    }
    if dark {
        egui::Color32::from_rgb(18, 18, 18)
    } else {
        egui::Color32::from_rgb(38, 38, 38)
    }
}

fn paint_checkerboard(
    painter: &egui::Painter,
    rect: egui::Rect,
    cell: f32,
    brightness: f32,
    dark: bool,
) {
    // Zooming can make the image rect much larger than the visible viewport.
    // Clip before iterating so we only paint checker tiles the user can see.
    let rect = rect.intersect(painter.clip_rect());
    if !rect.is_positive() {
        return;
    }
    let cell = cell.clamp(8.0, 64.0);
    let brightness = brightness.clamp(0.25, 1.5);
    let tone = |base: f32| (base * brightness).clamp(0.0, 255.0) as u8;
    let a = if dark {
        egui::Color32::from_gray(tone(45.0))
    } else {
        egui::Color32::from_gray(tone(210.0))
    };
    let b = if dark {
        egui::Color32::from_gray(tone(65.0))
    } else {
        egui::Color32::from_gray(tone(240.0))
    };
    let x0 = (rect.left() / cell).floor() as i32;
    let y0 = (rect.top() / cell).floor() as i32;
    let x1 = (rect.right() / cell).ceil() as i32;
    let y1 = (rect.bottom() / cell).ceil() as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            let tile = egui::Rect::from_min_max(
                egui::pos2(x as f32 * cell, y as f32 * cell).max(rect.min),
                egui::pos2((x + 1) as f32 * cell, (y + 1) as f32 * cell).min(rect.max),
            );
            if tile.is_positive() {
                painter.rect_filled(tile, 0.0, if (x + y) % 2 == 0 { a } else { b });
            }
        }
    }
}


/// Reserve non-overlapping room for centered platform toolbars only while the
/// image is in fit-to-window mode. The original JPEG Viewer layout is unchanged.
fn fitted_image_viewport_rect(
    available: egui::Rect,
    ctx: &egui::Context,
    settings: &crate::settings::AppSettings,
    fit_mode: bool,
) -> egui::Rect {
    if !fit_mode {
        return available;
    }

    let profile = crate::ui_theme::current_look_profile(ctx);
    if profile != "cosmic" && profile != "gnome" && profile != "win98" {
        return available;
    }

    const EDGE_PADDING: f32 = 12.0;
    const TOOLBAR_CLEARANCE: f32 = 58.0;
    let placement = crate::ui_theme::resolve_main_ui_placement(
        &settings.main_ui_placement_preference,
        ctx,
    );
    let mut top = EDGE_PADDING;
    let mut bottom = EDGE_PADDING;
    match placement {
        "cosmic" => bottom += TOOLBAR_CLEARANCE,
        "gnome" | "win98" => top += TOOLBAR_CLEARANCE,
        // With the original split toolbar, leave room at both ends so neither
        // the zoom controls nor the bottom navigation overlap a fitted image.
        _ => {
            top += 40.0;
            bottom += 40.0;
        }
    }

    let max_vertical_inset = (available.height() - 1.0).max(0.0);
    let vertical_total = top + bottom;
    if vertical_total > max_vertical_inset && vertical_total > 0.0 {
        let factor = max_vertical_inset / vertical_total;
        top *= factor;
        bottom *= factor;
    }

    let horizontal = EDGE_PADDING.min((available.width() - 1.0).max(0.0) * 0.5);
    egui::Rect::from_min_max(
        egui::pos2(available.left() + horizontal, available.top() + top),
        egui::pos2(available.right() - horizontal, available.bottom() - bottom),
    )
}
