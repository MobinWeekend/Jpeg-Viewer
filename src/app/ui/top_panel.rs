use crate::app::aspect_ratio::AspectRatio;
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ========== LEFT SECTION ==========
                // Open button with icon
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("📂 Open").size(14.0))
                            .min_size(egui::vec2(70.0, 28.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }
                ui.add_space(4.0);

                // Navigation buttons
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("◀").size(16.0))
                            .min_size(egui::vec2(32.0, 28.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::PreviousImage);
                }

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("▶").size(16.0))
                            .min_size(egui::vec2(32.0, 28.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::NextImage);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // ========== SLIDESHOW CONTROLS ==========
                if !self.image_entries.is_empty() {
                    let slideshow_icon = if self.slideshow_enabled {
                        "⏸"
                    } else {
                        "▶▶"
                    };
                    let slideshow_tooltip = if self.slideshow_enabled {
                        "Stop Slideshow"
                    } else {
                        "Start Slideshow"
                    };
                    let slideshow_button = ui
                        .add(
                            egui::Button::new(egui::RichText::new(slideshow_icon).size(14.0))
                                .min_size(egui::vec2(32.0, 28.0)),
                        )
                        .on_hover_text(slideshow_tooltip);
                    if slideshow_button.clicked() {
                        self.handle_command(ctx, ViewerCommand::ToggleSlideshow);
                    }

                    if self.slideshow_enabled {
                        // Show interval timer
                        let interval_secs = self.slideshow_interval.as_secs_f32();
                        ui.label(egui::RichText::new(format!("{:.1}s", interval_secs)).size(12.0));

                        // Speed controls - Slower
                        let slower_button = ui
                            .add(
                                egui::Button::new(egui::RichText::new("◀◀").size(12.0))
                                    .min_size(egui::vec2(28.0, 24.0)),
                            )
                            .on_hover_text("Slower slideshow");
                        if slower_button.clicked() {
                            self.handle_command(ctx, ViewerCommand::SlideshowSpeedDown);
                        }

                        // Speed controls - Faster
                        let faster_button = ui
                            .add(
                                egui::Button::new(egui::RichText::new("▶▶").size(12.0))
                                    .min_size(egui::vec2(28.0, 24.0)),
                            )
                            .on_hover_text("Faster slideshow");
                        if faster_button.clicked() {
                            self.handle_command(ctx, ViewerCommand::SlideshowSpeedUp);
                        }

                        // Random indicator
                        if self.slideshow_random {
                            ui.label(egui::RichText::new("🎲").size(12.0));
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                }

                // Zoom controls
                ui.label(
                    egui::RichText::new(format!(
                        "{}%",
                        (self.zoom * 100.0).round().max(1.0) as i32
                    ))
                    .size(13.0),
                );

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("+").size(16.0))
                            .min_size(egui::vec2(24.0, 24.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::ZoomIn);
                }

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("−").size(16.0))
                            .min_size(egui::vec2(24.0, 24.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::ZoomOut);
                }

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("⊡").size(14.0))
                            .min_size(egui::vec2(28.0, 24.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::MakeFit);
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("1:1").size(14.0))
                            .min_size(egui::vec2(28.0, 24.0)),
                    )
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::ResetZoom);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // ========== IMAGE INFO ==========
                if let Some(texture) = &self.texture {
                    let size = texture.size_vec2();
                    let width = size.x as u32;
                    let height = size.y as u32;

                    let file_size_str = self.get_file_size_string();
                    let aspect_ratio_str = AspectRatio::get_label(width, height);

                    // Resolution
                    ui.label(egui::RichText::new(format!("{}×{}", width, height)).size(12.0));
                    ui.add_space(4.0);

                    // File size
                    if !file_size_str.is_empty() {
                        ui.label(egui::RichText::new(file_size_str).size(12.0));
                        ui.add_space(4.0);
                    }

                    // Aspect ratio
                    if let Some(label) = aspect_ratio_str {
                        ui.label(egui::RichText::new(label).size(12.0));
                    } else {
                        let ratio_display = AspectRatio::format_as_ratio(width, height);
                        ui.label(
                            egui::RichText::new(format!("{} (Uncommon)", ratio_display)).size(12.0),
                        );
                    }
                }

                // ========== GIF CONTROLS ==========
                if self.is_gif {
                    if let Some(gif) = &mut self.gif_animation {
                        if gif.is_animated() {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Play/Pause button
                            let play_text = if gif.is_playing { "⏸" } else { "▶" };
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(play_text).size(16.0))
                                        .min_size(egui::vec2(32.0, 28.0)),
                                )
                                .clicked()
                            {
                                gif.toggle_play();
                            }

                            // Frame counter
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    gif.get_current_frame_index() + 1,
                                    gif.frame_count()
                                ))
                                .size(12.0),
                            );

                            // Speed controls
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
                                // Cycle speeds: 0.5x -> 1x -> 2x -> 3x -> 1x
                                let current = gif.speed_multiplier;
                                let next = if current == 0.5 {
                                    1.0
                                } else if current == 1.0 {
                                    2.0
                                } else if current == 2.0 {
                                    3.0
                                } else {
                                    0.5
                                };
                                gif.set_speed(next);
                            }

                            if self.is_preview {
                                ui.label(egui::RichText::new("⏳ Loading...").size(12.0));
                            }
                        }
                    }
                }

                // ========== SPACER TO PUSH RIGHT SECTION ==========
                ui.add_space(8.0);

                // ========== RIGHT SECTION ==========
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Settings button (far right)
                    let settings_button = ui
                        .add(
                            egui::Button::new(egui::RichText::new("⚙️").size(16.0))
                                .min_size(egui::vec2(36.0, 28.0)),
                        )
                        .on_hover_text("Settings");
                    if settings_button.clicked() {
                        self.toggle_settings_menu();
                    }
                    ui.add_space(4.0);

                    // Help button
                    let help_button = ui
                        .add(
                            egui::Button::new(egui::RichText::new("❓").size(16.0))
                                .min_size(egui::vec2(36.0, 28.0)),
                        )
                        .on_hover_text("Help - Keyboard shortcuts and features");
                    if help_button.clicked() {
                        self.toggle_help_menu();
                    }
                    ui.add_space(4.0);

                    // Fullscreen button
                    let _is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    let fs_button = ui
                        .add(
                            egui::Button::new(egui::RichText::new("⛶").size(16.0))
                                .min_size(egui::vec2(36.0, 28.0)),
                        )
                        .on_hover_text("Toggle Fullscreen");
                    if fs_button.clicked() {
                        self.toggle_fullscreen(ctx);
                    }
                    ui.add_space(4.0);

                    // Loading indicator
                    if self.b_is_loading_full || self.b_is_loading {
                        ui.add(egui::Spinner::new());
                        ui.add_space(4.0);
                    }

                    ui.add_space(8.0);

                    /*ui.label(
                        egui::RichText::new(
                            "⌨️ Scroll: Navigate • Ctrl+Scroll: Zoom • L: Slideshow",
                        )
                        .size(11.0),
                    );*/
                });
            });
        });
    }

    /// Get file size as a formatted string
    fn get_file_size_string(&self) -> String {
        if let Some(path) = &self.current_image_path {
            if let Ok(metadata) = std::fs::metadata(path) {
                return Self::format_file_size(metadata.len());
            }
        }

        // For archive images, try to get size from the entry
        if let Some(entry) = self.image_entries.get(self.current_index) {
            match entry {
                crate::image_entry::ImageEntry::Zip(zip) => {
                    if let Ok(file) = std::fs::File::open(&zip.archive_path) {
                        if let Ok(mut archive) = zip::ZipArchive::new(file) {
                            if let Ok(entry) = archive.by_index(zip.entry_index) {
                                return Self::format_file_size(entry.size());
                            }
                        }
                    }
                }
                crate::image_entry::ImageEntry::S7z(_) | crate::image_entry::ImageEntry::Rar(_) => {
                    // 7z and RAR don't provide easy size info without reading the file
                }
                _ => {}
            }
        }

        String::new()
    }

    /// Format file size in human-readable format
    fn format_file_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;

        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}