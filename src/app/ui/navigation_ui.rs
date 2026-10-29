//use crate::app::aspect_ratio::AspectRatio;
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use crate::ui_theme::scaled_text_size;
use eframe::egui;

/// Render toolbar buttons with the classic raised/sunken bevel in the Windows 98
/// profile, while delegating the actual interaction and state handling to egui.
trait ProfileToolbarButtonExt {
    fn add_profile_button<'a>(&mut self, button: egui::Button<'a>) -> egui::Response;
}

impl ProfileToolbarButtonExt for egui::Ui {
    fn add_profile_button<'a>(&mut self, button: egui::Button<'a>) -> egui::Response {
        let response = self.add(button);
        if crate::ui_theme::current_look_profile(self.ctx()) == "win98" {
            crate::ui_theme::paint_win98_bevel(self, response.rect, response.is_pointer_button_down_on());
        }
        response
    }
}

impl ViewerApp {
    // Render all navigation-related controls.
    // Individual sections can also be called separately when needed.

    // NAVIGATION BUTTONS
    /// Previous and next image buttons.
    pub fn navigation_previous_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("◀").size(scaled_text_size(ui.ctx(), 22.0)))
                    .min_size(self.btn_size())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("Previous")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::PreviousImage);
        }
    }
    pub fn navigation_next_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("▶").size(scaled_text_size(ui.ctx(), 22.0)))
                    .min_size(self.btn_size())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
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
            .add_profile_button(
                egui::Button::new(
                    egui::RichText::new(slideshow_icon).size(scaled_text_size(ui.ctx(), 22.0)),
                )
                .min_size(self.btn_size())
                .fill(toolbar_button_fill(ui))
                .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text(slideshow_tooltip);

        if button.clicked() {
            self.handle_command(ctx, ViewerCommand::ToggleSlideshow);
        }

    }

    /// Slideshow options row, rendered beside the navigation bar while playback is active.
    /// The second row uses the same switches and persisted controls as Settings > Slideshow.
    pub fn slideshow_speed_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        use crate::sort::SortMethod;

        let interval_ms = self.slideshow_interval.as_millis().max(500) as u64;
        let mut photos_per_min = ((60_000.0 / interval_ms as f64).round() as i32).clamp(1, 120);

        ui.label(egui::RichText::new("Photos/min").size(scaled_text_size(ui.ctx(), 12.0)));
        let speed_edit = ui.add_sized(
            egui::vec2(58.0, ui.spacing().interact_size.y),
            egui::DragValue::new(&mut photos_per_min)
                .range(1..=120)
                .speed(1),
        );
        if speed_edit.changed() {
            let interval_ms = (60_000_u64 / photos_per_min.clamp(1, 120) as u64).clamp(500, 60_000);
            self.slideshow_interval = std::time::Duration::from_millis(interval_ms);
            self.slideshow_last_advance = std::time::Instant::now();
            let _ = self.settings_manager.update(|settings| {
                settings.slideshow_interval_ms = interval_ms;
            });
        }

        let slower_button = ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("−").size(scaled_text_size(ui.ctx(), 16.0)))
                    .min_size(self.btn_size_medium())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("Fewer photos per minute");
        if slower_button.clicked() {
            self.handle_command(ctx, ViewerCommand::SlideshowSpeedDown);
        }

        let faster_button = ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("+").size(scaled_text_size(ui.ctx(), 16.0)))
                    .min_size(self.btn_size_medium())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("More photos per minute");
        if faster_button.clicked() {
            self.handle_command(ctx, ViewerCommand::SlideshowSpeedUp);
        }

        ui.separator();
        let mut loop_enabled = self.slideshow_loop;
        if crate::ui_theme::switch(ui, &mut loop_enabled, "Loop") {
            self.slideshow_loop = loop_enabled;
            let _ = self.settings_manager.update(|settings| settings.slideshow_loop = loop_enabled);
        }

        ui.separator();
        let mut selected_sort = self.sort_method;
        let sort_label = match selected_sort {
            SortMethod::Natural => "Natural",
            SortMethod::DateNewest => "Newest",
            SortMethod::DateOldest => "Oldest",
            SortMethod::Shuffle => "Shuffle",
        };
        ui.label("Sort:");
        egui::ComboBox::from_id_salt("slideshow_sort_method")
            .selected_text(sort_label)
            .width(88.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected_sort, SortMethod::Natural, "Natural");
                ui.selectable_value(&mut selected_sort, SortMethod::DateNewest, "Newest");
                ui.selectable_value(&mut selected_sort, SortMethod::DateOldest, "Oldest");
                ui.selectable_value(&mut selected_sort, SortMethod::Shuffle, "Shuffle");
            });
        if selected_sort != self.sort_method {
            self.sort_method = selected_sort;
            // The selected sort controls the actual collection order, including when
            // viewing a queue or a local list. Keep the current image selected.
            let selected_id = self
                .image_entries
                .get(self.current_index)
                .map(|entry| entry.get_id());
            if !self.image_entries.is_empty() {
                crate::sort::sort_images(&mut self.image_entries, self.sort_method);
                if let Some(id) = selected_id {
                    if let Some(index) = self.image_entries.iter().position(|entry| entry.get_id() == id) {
                        self.current_index = index;
                    }
                }
            }
            // Shuffle now means shuffle the collection once, not a separate random
            // jump on every slideshow tick.
            self.slideshow_random = false;
            let _ = self.settings_manager.update(|settings| settings.slideshow_random = false);
            self.image_cache.clear();
            self.preloading_indices.clear();
            self.preload_tasks.clear();
            self.preload_generation = self.preload_generation.wrapping_add(1);
            self.sidebar_last_current_index = None;
            if !self.image_entries.is_empty() {
                self.load_current_image_with_cache();
            }
            ctx.request_repaint();
        }

        ui.separator();
        let mut transition = self.slideshow_transition.clone();
        ui.label("Transition:");
        egui::ComboBox::from_id_salt("slideshow_toolbar_transition")
            .selected_text(match transition.as_str() {
                "none" => "Instant",
                "slide" => "Slide",
                "random" => "Random",
                _ => "Fade",
            })
            .width(78.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut transition, "none".to_owned(), "Instant");
                ui.selectable_value(&mut transition, "fade".to_owned(), "Fade");
                ui.selectable_value(&mut transition, "slide".to_owned(), "Slide");
                ui.selectable_value(&mut transition, "random".to_owned(), "Random");
            });
        if transition != self.slideshow_transition {
            self.slideshow_transition = transition.clone();
            let _ = self.settings_manager.update(|settings| settings.slideshow_transition = transition);
        }

        let mut motion = self.slideshow_motion.clone();
        ui.label("Motion:");
        egui::ComboBox::from_id_salt("slideshow_toolbar_motion")
            .selected_text(match motion.as_str() {
                "none" => "Off",
                "pan-horizontal" => "Pan wide",
                "pan-vertical" => "Pan tall",
                "zoom-in" => "Zoom in",
                "zoom-out" => "Zoom out",
                "random" => "Random",
                _ => "Smart",
            })
            .width(82.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut motion, "none".to_owned(), "Off");
                ui.selectable_value(&mut motion, "smart".to_owned(), "Smart");
                ui.selectable_value(&mut motion, "pan-horizontal".to_owned(), "Pan wide images");
                ui.selectable_value(&mut motion, "pan-vertical".to_owned(), "Pan tall images");
                ui.selectable_value(&mut motion, "zoom-in".to_owned(), "Zoom in");
                ui.selectable_value(&mut motion, "zoom-out".to_owned(), "Zoom out");
                ui.selectable_value(&mut motion, "random".to_owned(), "Random");
            });
        if motion != self.slideshow_motion {
            self.slideshow_motion = motion.clone();
            let _ = self.settings_manager.update(|settings| settings.slideshow_motion = motion);
        }

        ui.separator();
        ui.label("Strength:");
        let mut strength = self.slideshow_motion_strength;
        let strength_slider = egui::Slider::new(&mut strength, 5.0..=40.0)
            .suffix("%")
            .show_value(true);
        if ui.add(crate::ui_theme::BorderedSlider::new(strength_slider)).changed() {
            self.slideshow_motion_strength = strength;
            let _ = self.settings_manager.update(|settings| settings.slideshow_motion_strength = strength);
        }
    }

    // ZOOM
    /// Zoom percentage, zoom buttons and fit/reset button.
    pub fn zoom_text(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let text = format!("{}%", (self.zoom * 100.0).round().max(1.0) as i32);
        let tooltip = if self.b_zoom_used {
            "Click to Fit"
        } else {
            "Click for 100%"
        };

        // Keep the layout slot fixed-width even for very large percentages. The
        // percentage is painted independently so its text may visually overflow the
        // slot instead of forcing neighboring toolbar controls to shift.
        let (zoom_rect, response) = ui.allocate_exact_size(
            egui::vec2(50.0, 24.0),
            egui::Sense::click(),
        );
        let font_id = egui::FontId::proportional(scaled_text_size(ui.ctx(), 14.0));
        ui.painter().text(
            zoom_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            ui.visuals().text_color(),
        );
        let response = response.on_hover_text(tooltip);

        if response.clicked() {
            if self.b_zoom_used {
                self.handle_command(ctx, ViewerCommand::MakeFit);
            } else {
                self.handle_command(ctx, ViewerCommand::ResetZoom);
            }
        }
    }

    // Zoom in
    pub fn zoom_in_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("+").size(scaled_text_size(ui.ctx(), 22.0)))
                    .min_size(self.btn_size_small())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("Zoom in")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ZoomIn);
        }
    }
    // Zoom out
    pub fn zoom_out_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("−").size(scaled_text_size(ui.ctx(), 22.0)))
                    .min_size(self.btn_size_small())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("Zoom out")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ZoomOut);
        }
    }

    pub fn zoom_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // Fit / 1:1
        if self.b_zoom_used {
            if ui
                .add_profile_button(
                    egui::Button::new(
                        egui::RichText::new("⊞").size(scaled_text_size(ui.ctx(), 14.0)),
                    )
                    .min_size(self.btn_size_medium())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
                )
                .on_hover_text("Fit")
                .clicked()
            {
                self.handle_command(ctx, ViewerCommand::MakeFit);
            }
        } else if ui
            .add_profile_button(
                egui::Button::new(
                    egui::RichText::new("1:1").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(self.btn_size_medium())
                .fill(toolbar_button_fill(ui))
                .stroke(toolbar_button_stroke(ui)),
            )
            .on_hover_text("Make 1:1 Ratio")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::ResetZoom);
        }
    }

    // GIF CONTROLS
    /// Display GIF playback controls if the current image is an animated GIF.
    pub fn gif_controls_ui(&mut self, ui: &mut egui::Ui) {
        if !self.is_gif {
            return;
        }
        // Read size BEFORE the mutable borrow
        let btn = self.btn_size();

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
            .add_profile_button(
                egui::Button::new(
                    egui::RichText::new(play_text).size(scaled_text_size(ui.ctx(), 22.0)),
                )
                .min_size(btn)
                .fill(toolbar_button_fill(ui))
                .stroke(toolbar_button_stroke(ui)),
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
            .add_profile_button(
                egui::Button::new(
                    egui::RichText::new(&speed_text).size(scaled_text_size(ui.ctx(), 12.0)),
                )
                .min_size(egui::vec2(40.0, 24.0))
                .fill(toolbar_button_fill(ui))
                .stroke(toolbar_button_stroke(ui)),
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
            crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut gif.speed_multiplier, 0.1..=10.0)
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
        ));

        // Frame counter
        ui.label(
            egui::RichText::new(format!(
                "{}/{}",
                gif.get_current_frame_index() + 1,
                gif.frame_count()
            ))
            .size(scaled_text_size(ui.ctx(), 12.0)),
        );

        if self.is_preview {
            ui.label(egui::RichText::new("Loading...").size(scaled_text_size(ui.ctx(), 12.0)));
        }
    }

    // Fullscreen
    pub fn fullscreen_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new("⛶").size(scaled_text_size(ui.ctx(), 14.0)))
                    .min_size(self.btn_size_small())
                    .fill(toolbar_button_fill(ui))
                    .stroke(toolbar_button_stroke(ui)),
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
        let (icon, tooltip, fill) = if self.window_always_on_top {
            ("📌", "Unpin Window", egui::Color32::RED)
        } else {
            ("📌", "Pin Window to Top", egui::Color32::TRANSPARENT)
        };

        if ui
            .add_profile_button(
                egui::Button::new(egui::RichText::new(icon).size(scaled_text_size(ui.ctx(), 14.0)))
                    .min_size(self.btn_size_small())
                    .fill(fill)
                    .stroke(toolbar_button_stroke(ui)),
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

    /// Standard toolbar button size.
    fn btn_size(&self) -> egui::Vec2 {
        if self.compactui {
            egui::vec2(24.0, 22.0) // compact
        } else {
            egui::vec2(32.0, 28.0) // normal
        }
    }

    /// Small toolbar button size (used for +/−, 1:1, fit, pin, etc.)
    fn btn_size_small(&self) -> egui::Vec2 {
        if self.compactui {
            egui::vec2(20.0, 22.0)
        } else {
            egui::vec2(24.0, 28.0)
        }
    }

    /// Medium button size (slower/faster, zoom text)
    fn btn_size_medium(&self) -> egui::Vec2 {
        if self.compactui {
            egui::vec2(22.0, 22.0)
        } else {
            egui::vec2(28.0, 28.0)
        }
    }
}


fn toolbar_button_fill(ui: &egui::Ui) -> egui::Color32 {
    if crate::ui_theme::current_look_profile(ui.ctx()) == "win98" {
        ui.visuals().widgets.inactive.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    }
}

fn toolbar_button_stroke(_ui: &egui::Ui) -> egui::Stroke {
    // A raised/sunken bevel is painted by ProfileToolbarButtonExt for Win98.
    egui::Stroke::NONE
}
