use crate::app::aspect_ratio::AspectRatio;
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    // Render all navigation-related controls.
    // Individual sections can also be called separately when needed.

    // NAVIGATION BUTTONS
    /// Previous and next image buttons.
    pub fn navigation_previous_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(egui::RichText::new("◀").size(16.0))
                    .min_size(egui::vec2(32.0, 28.0)),
            )
            .on_hover_text("Previous")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::PreviousImage);
        }
    }
    pub fn navigation_next_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(egui::RichText::new("▶").size(16.0))
                    .min_size(egui::vec2(32.0, 28.0)),
            )
            .on_hover_text("Next")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::NextImage);
        }
    }

    // SLIDESHOW

    /// Slideshow start/stop button and controls.
    pub fn slideshow_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let slideshow_icon = if self.slideshow_enabled {
            "⏸"
        } else {
            "🖼"
        };

        let slideshow_tooltip = if self.slideshow_enabled {
            "Stop Slideshow"
        } else {
            "Start Slideshow"
        };

        let button = ui
            .add(
                egui::Button::new(egui::RichText::new(slideshow_icon).size(24.0))
                    .min_size(egui::vec2(32.0, 28.0)),
            )
            .on_hover_text(slideshow_tooltip);

        if button.clicked() {
            self.handle_command(ctx, ViewerCommand::ToggleSlideshow);
        }

        if self.slideshow_enabled {
            self.slideshow_speed_ui(ctx, ui);
        }
    }

    /// Slideshow interval, speed and random indicator.
    pub fn slideshow_speed_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let interval_secs = self.slideshow_interval.as_secs_f32();
        let interval_mins = 60.0 / interval_secs;

        ui.label(egui::RichText::new(format!("{:1} Photo per min", interval_mins)).size(12.0));

        // Slower
        let slower_button = ui
            .add(
                egui::Button::new(egui::RichText::new("↘").size(24.0))
                    .min_size(egui::vec2(28.0, 24.0)),
            )
            .on_hover_text("Slower slideshow");

        if slower_button.clicked() {
            self.handle_command(ctx, ViewerCommand::SlideshowSpeedDown);
        }

        // Faster
        let faster_button = ui
            .add(
                egui::Button::new(egui::RichText::new("↗").size(24.0))
                    .min_size(egui::vec2(28.0, 24.0)),
            )
            .on_hover_text("Faster slideshow");

        if faster_button.clicked() {
            self.handle_command(ctx, ViewerCommand::SlideshowSpeedUp);
        }

        if self.slideshow_random {
            ui.label(egui::RichText::new("Random").size(12.0));
        }
    }

    // ZOOM
    /// Zoom percentage, zoom buttons and fit/reset button.
    pub fn zoom_text(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(52.0, 24.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(format!("{}%", (self.zoom * 100.0).round().max(1.0) as i32))
                    .size(14.0),
            )
            .halign(egui::Align::Center),
        );
    }
    pub fn zoom_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // Zoom in
        if ui
            .add(
                egui::Button::new(egui::RichText::new("+").size(20.0))
                    .min_size(egui::vec2(24.0, 24.0)),
            )
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ZoomIn);
        }

        // Zoom out
        if ui
            .add(
                egui::Button::new(egui::RichText::new("−").size(20.0))
                    .min_size(egui::vec2(24.0, 24.0)),
            )
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ZoomOut);
        }

        // Fit / 1:1
        if self.b_zoom_used {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("⊞").size(14.0))
                        .min_size(egui::vec2(28.0, 24.0)),
                )
                .on_hover_text("Fit")
                .clicked()
            {
                self.handle_command(ctx, ViewerCommand::MakeFit);
            }
        } else if ui
            .add(
                egui::Button::new(egui::RichText::new("1:1").size(14.0))
                    .min_size(egui::vec2(28.0, 24.0)),
            )
            .on_hover_text("Make 1:1 Ratio")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ResetZoom);
        }
    }

    // IMAGE INFORMATION
    /// Display resolution, file size and aspect ratio.
    pub fn image_info_ui(&self, ui: &mut egui::Ui) {
        let (width, height) = self.image_dimensions();

        if width == 0 || height == 0 {
            return;
        }

        let file_size = self.get_file_size_string();
        let aspect_ratio = AspectRatio::get_label(width, height);

        // Resolution
        ui.label(egui::RichText::new(format!("{}×{}", width, height)).size(12.0));

        ui.add_space(4.0);

        // File size
        if !file_size.is_empty() {
            ui.label(egui::RichText::new(file_size).size(12.0));

            ui.add_space(4.0);
        }

        // Aspect ratio
        if let Some(label) = aspect_ratio {
            ui.label(egui::RichText::new(label).size(14.0));
        }
    }

    /// Return the dimensions of the currently loaded image.
    pub fn image_dimensions(&self) -> (u32, u32) {
        if let Some(vt) = &self.virtual_texture {
            return vt.dimensions();
        }

        if let Some(texture) = &self.texture {
            let size = texture.size_vec2();

            return (size.x as u32, size.y as u32);
        }

        (0, 0)
    }

    // GIF CONTROLS
    /// Display GIF playback controls if the current image is an animated GIF.
    pub fn gif_controls_ui(&mut self, ui: &mut egui::Ui) {
        if !self.is_gif {
            return;
        }

        let Some(gif) = &mut self.gif_animation else {
            return;
        };

        if !gif.is_animated() {
            return;
        }

        // Play / Pause
        let play_text = if gif.is_playing {
            "⏸"
        } else if gif.speed_multiplier < 0.0 {
            "◀"
        } else {
            "▶"
        };

        if ui
            .add(
                egui::Button::new(egui::RichText::new(play_text).size(16.0))
                    .min_size(egui::vec2(32.0, 28.0)),
            )
            .clicked()
        {
            gif.toggle_play();
        }

        // Speed button
        let speed_text = if gif.speed_multiplier == 1.0 {
            "1×".to_string()
        } else if gif.speed_multiplier < 1.0 {
            format!("{:.1}×", gif.speed_multiplier)
        } else {
            format!("{}×", gif.speed_multiplier as i32)
        };

        if ui
            .add(
                egui::Button::new(egui::RichText::new(&speed_text).size(12.0))
                    .min_size(egui::vec2(40.0, 24.0)),
            )
            .clicked()
        {
            let current = gif.speed_multiplier;

            let next = if current == 1.0 {
                2.0
            } else if current == 2.0 {
                3.0
            } else if current == 3.0 {
                0.5
            } else {
                1.0
            };

            gif.set_speed(next);
        }

        // Speed slider
        ui.add(
            egui::Slider::new(&mut gif.speed_multiplier, 0.1..=10.0)
                .logarithmic(true)
                .show_value(false)
                .custom_formatter(|value, _| {
                    if (value - 1.0).abs() < 0.01 {
                        "1×".to_owned()
                    } else if value < 1.0 {
                        format!("{:.1}×", value)
                    } else {
                        format!("{:.0}×", value)
                    }
                }),
        );

        // Frame counter
        ui.label(
            egui::RichText::new(format!(
                "{}/{}",
                gif.get_current_frame_index() + 1,
                gif.frame_count()
            ))
            .size(12.0),
        );

        if self.is_preview {
            ui.label(egui::RichText::new("Loading...").size(12.0));
        }
    }

    // Fullscreen
    pub fn fullscreen_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(egui::RichText::new("⛶").size(14.0))
                    .min_size(egui::vec2(24.0, 24.0)),
            )
            .on_hover_text("Toggle Fullscreen")
            .clicked()
        {
            self.toggle_fullscreen(ctx);
            self.hamburger_menu_open = false;
        }
    }

    // Toggle whether the application window stays above all other windows.
    pub fn pin_window_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let (icon, tooltip, color) = if self.window_always_on_top {
            ("📌", "Unpin Window", egui::Color32::LIGHT_BLUE)
        } else {
            ("📌", "Pin Window to Top", egui::Color32::GRAY)
        };

        if ui
            .add(
                egui::Button::new(egui::RichText::new(icon).size(14.0).color(color))
                    .min_size(egui::vec2(24.0, 24.0)),
            )
            .on_hover_text(tooltip)
            .clicked()
        {
            self.window_always_on_top = !self.window_always_on_top;

            let window_level = if self.window_always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };

            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(window_level));
        }
    }
}
