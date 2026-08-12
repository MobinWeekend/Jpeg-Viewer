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
                    let (total_tiles, prepared_tiles, progress_percent) = {
                        let total = vt.total_tiles();
                        let prepared = vt.prepared_tiles_count();
                        let progress = vt.preparation_progress();
                        (total, prepared, progress)
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
                let available_rect  = ui.available_rect_before_wrap();

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
                let response = ui.allocate_rect(available_rect, egui::Sense::drag());

                self.handle_image_mouse_input(ctx, &response);

                // Show image counter
                if !self.image_entries.is_empty() {
                    self.render_image_counter(ui);
                }

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

            let image_rect = self.get_image_rect(texture_size, center);
            let response = ui.allocate_rect(available_rect, egui::Sense::drag());

            let painter = ui.painter();
            painter.image(
                texture_id,
                image_rect,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                egui::Color32::WHITE,
            );

            // In render_central_panel(), after the image is rendered:
            if self.is_loading() {
                let painter = ui.painter();
                let message = match self.loading_state {
                    LoadingState::LoadingFullGif => "Loading full GIF...",
                    LoadingState::VirtualTextureLoading => "Loading large image...",
                    _ => "Loading...",
                };
                Self::draw_loading_overlay(&painter, available_rect, message);
            }

            self.handle_image_mouse_input(ctx, &response);

            if !self.image_entries.is_empty() {
                self.render_image_counter(ui);
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
        let center = ui.available_rect_before_wrap().center();
        let rect = Self::loading_content_rect(center);

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
        const SPINNER_SIZE: f32 = 48.0;
        const TEXT_HEIGHT: f32 = 24.0;
        const PROGRESS_HEIGHT: f32 = 40.0;

        let time = painter.ctx().input(|i| i.time);

        // -------------------------
        // Spinner
        // -------------------------

        let angle = (time * 3.0) as f32;
        let radius = SPINNER_SIZE * 0.35;
        let segments = 8;

        let spinner_center = egui::pos2(
            center.x,
            center.y
                - (TEXT_HEIGHT / 2.0 + 8.0)
                - if progress.is_some() {
                    PROGRESS_HEIGHT / 2.0
                } else {
                    0.0
                },
        );

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

        let font_id = egui::FontId::proportional(18.0);
        let text_color = style.visuals.text_color();

        let galley = painter.layout(message.to_owned(), font_id, text_color, f32::INFINITY);

        let text_y = if progress.is_some() {
            center.y + SPINNER_SIZE / 2.0 + 8.0 - PROGRESS_HEIGHT / 2.0
        } else {
            center.y + SPINNER_SIZE / 2.0 + 8.0
        };

        painter.galley(
            egui::pos2(center.x - galley.rect.width() / 2.0, text_y),
            galley,
            text_color,
        );

        // -------------------------
        // Progress bar
        // -------------------------

        if let Some(progress) = progress {
            if progress > 0.0 {
                let progress = progress.clamp(0.0, 1.0);

                let bar_rect = egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 60.0),
                    egui::vec2(200.0, 8.0),
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
                    egui::FontId::proportional(12.0),
                    text_color,
                    f32::INFINITY,
                );

                painter.galley(
                    egui::pos2(
                        center.x - percent_galley.rect.width() / 2.0,
                        bar_rect.max.y + 6.0,
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
                        egui::pos2(center.x - galley.rect.width() / 2.0, bar_rect.max.y + 24.0),
                        galley,
                        text_color,
                    );
                }
            }
        }
    }

    fn draw_loading_overlay(painter: &egui::Painter, rect: egui::Rect, message: &str) {
        let center = rect.center();
        let content_rect = Self::loading_content_rect(center);

        painter.rect_filled(content_rect, 12.0, painter.ctx().style().visuals.panel_fill);

        Self::draw_loading_content(
            painter,
            center,
            message,
            None,
            false,
            &painter.ctx().style(),
        );
    }

    fn loading_content_rect(center: egui::Pos2) -> egui::Rect {
        const CONTENT_WIDTH: f32 = 300.0;
        const CONTENT_HEIGHT: f32 = 220.0;

        egui::Rect::from_center_size(center, egui::vec2(CONTENT_WIDTH, CONTENT_HEIGHT))
    }

    // ... rest of helper functions (render_error_ui, render_welcome_ui, render_image_counter, etc.) ...
    fn render_error_ui(&mut self, ui: &mut egui::Ui, error: &str) {
        ui.centered_and_justified(|ui| {
            ui.add_space(40.0);
            ui.label(egui::RichText::new("🖼️").size(64.0));
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new("Failed to Load Image")
                    .size(28.0)
                    .strong(),
            );
            ui.add_space(8.0);
            ui.label(egui::RichText::new(error).size(16.0));
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("🔄 Retry").size(16.0))
                            .min_size(egui::vec2(100.0, 36.0)),
                    )
                    .clicked()
                {
                    self.image_error = None;
                    self.load_current_image_with_cache();
                }

                ui.add_space(8.0);

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("⏭️ Skip").size(16.0))
                            .min_size(egui::vec2(100.0, 36.0)),
                    )
                    .clicked()
                {
                    self.navigate_images(ui.ctx(), 1);
                }
            });
        });
    }

    // Note: render_welcome_ui is not shown; keep your existing implementation.

    fn render_image_counter(&self, ui: &mut egui::Ui) {
        let total = self.image_entries.len();
        if total == 0 {
            return;
        }

        let text = format!("{}/{}", self.current_index + 1, total);
        let font_id = egui::FontId::proportional(14.0);
        let text_color = ui.style().visuals.text_color();
        let galley = ui
            .painter()
            .layout(text, font_id, text_color, f32::INFINITY);

        let rect = ui.available_rect_before_wrap();
        let pos = egui::pos2(
            rect.right() - galley.rect.width() - 20.0,
            rect.bottom() - 40.0,
        );

        let bg_rect = galley
            .rect
            .translate(egui::Vec2::new(pos.x, pos.y))
            .expand(10.0);
        ui.painter()
            .rect_filled(bg_rect, 20.0, ui.style().visuals.panel_fill);
        ui.painter().galley(pos, galley, text_color);
    }
}
