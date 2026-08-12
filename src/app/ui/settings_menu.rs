use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use eframe::egui;
use std::time::Duration;

impl ViewerApp {
    pub fn render_settings_menu(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings_menu;
        let close_requested = false;

        egui::Window::new("Settings")
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 620.0])
            .min_size([350.0, 450.0])
            .max_size([600.0, 800.0])
            .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 42.0))
            .open(&mut open)
            .show(ctx, |ui| {
                // ========== SCROLLABLE CONTENT ==========
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            if ui
                                .button("📋 Copy Image")
                                .on_hover_text("Shortcut: Ctrl+C")
                                .clicked()
                            {
                                self.copy_image_to_clipboard();
                            }
                            ui.add_space(8.0);
                            if ui
                                .button("📋 Copy Path")
                                .on_hover_text("Shortcut: Ctrl+Shift+C")
                                .clicked()
                            {
                                self.copy_path_to_clipboard();
                            }
                        });

                        ui.add_space(4.0);
                        // ========== FILE INFO ==========
                        ui.collapsing(egui::RichText::new("📂 File Info").size(15.0), |ui| {
                            ui.add_space(4.0);

                            let Some(entry) = self.image_entries.get(self.current_index) else {
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("No image loaded").strong());
                                });
                                return;
                            };

                            // Get information based on the ImageEntry variant.
                            let (file_name, location, file_size) = match entry {
                                ImageEntry::File(path) => {
                                    let file_name = path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();

                                    let file_size = match std::fs::metadata(path) {
                                        Ok(metadata) => {
                                            let bytes = metadata.len();

                                            if bytes < 1024 {
                                                format!("{} B", bytes)
                                            } else if bytes < 1024 * 1024 {
                                                format!("{:.1} KB", bytes as f64 / 1024.0)
                                            } else if bytes < 1024 * 1024 * 1024 {
                                                format!(
                                                    "{:.1} MB",
                                                    bytes as f64 / (1024.0 * 1024.0)
                                                )
                                            } else {
                                                format!(
                                                    "{:.2} GB",
                                                    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                                                )
                                            }
                                        }
                                        Err(_) => "N/A".to_string(),
                                    };

                                    (file_name, path.clone(), file_size)
                                }

                                ImageEntry::Zip(zip) => {
                                    let file_size = std::fs::metadata(&zip.archive_path)
                                        .map(|metadata| {
                                            let bytes = metadata.len();

                                            if bytes < 1024 {
                                                format!("{} B", bytes)
                                            } else if bytes < 1024 * 1024 {
                                                format!("{:.1} KB", bytes as f64 / 1024.0)
                                            } else if bytes < 1024 * 1024 * 1024 {
                                                format!(
                                                    "{:.1} MB",
                                                    bytes as f64 / (1024.0 * 1024.0)
                                                )
                                            } else {
                                                format!(
                                                    "{:.2} GB",
                                                    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                                                )
                                            }
                                        })
                                        .unwrap_or_else(|_| "N/A".to_string());

                                    (zip.name.clone(), zip.archive_path.clone(), file_size)
                                }

                                ImageEntry::S7z(s7z) => {
                                    let file_size = std::fs::metadata(&s7z.archive_path)
                                        .map(|metadata| {
                                            let bytes = metadata.len();

                                            if bytes < 1024 {
                                                format!("{} B", bytes)
                                            } else if bytes < 1024 * 1024 {
                                                format!("{:.1} KB", bytes as f64 / 1024.0)
                                            } else if bytes < 1024 * 1024 * 1024 {
                                                format!(
                                                    "{:.1} MB",
                                                    bytes as f64 / (1024.0 * 1024.0)
                                                )
                                            } else {
                                                format!(
                                                    "{:.2} GB",
                                                    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                                                )
                                            }
                                        })
                                        .unwrap_or_else(|_| "N/A".to_string());

                                    (s7z.name.clone(), s7z.archive_path.clone(), file_size)
                                }

                                ImageEntry::Rar(rar) => {
                                    let file_size = std::fs::metadata(&rar.archive_path)
                                        .map(|metadata| {
                                            let bytes = metadata.len();

                                            if bytes < 1024 {
                                                format!("{} B", bytes)
                                            } else if bytes < 1024 * 1024 {
                                                format!("{:.1} KB", bytes as f64 / 1024.0)
                                            } else if bytes < 1024 * 1024 * 1024 {
                                                format!(
                                                    "{:.1} MB",
                                                    bytes as f64 / (1024.0 * 1024.0)
                                                )
                                            } else {
                                                format!(
                                                    "{:.2} GB",
                                                    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                                                )
                                            }
                                        })
                                        .unwrap_or_else(|_| "N/A".to_string());

                                    (rar.name.clone(), rar.archive_path.clone(), file_size)
                                }
                            };

                            // File name
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("File:").strong());
                                ui.add_space(8.0);
                                ui.label(&file_name);
                            });

                            ui.add_space(4.0);

                            // Size
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Size:").strong());
                                ui.add_space(8.0);
                                ui.label(&file_size);
                            });

                            ui.add_space(4.0);

                            // Path
                            let path_display = location.display().to_string();

                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Path:").strong());
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(&path_display)
                                        .color(egui::Color32::LIGHT_GRAY)
                                        .monospace(),
                                );
                            });

                            ui.add_space(4.0);
                        });

                        // ========== GENERAL SETTINGS ==========
                        ui.collapsing(egui::RichText::new("📋 General").size(15.0), |ui| {
                            ui.add_space(4.0);

                            // Scroll Zoom invert
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let response =
                                    ui.checkbox(&mut self.b_ctrl_invert, "Invert Scroll Zoom");
                                if response.changed() {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.b_ctrl_invert = self.b_ctrl_invert;
                                    });
                                }
                                ui.add_space(4.0);
                                ui.label("(Ctrl+Scroll to zoom)");
                            });

                            ui.add_space(4.0);

                            // Texture Filter
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Texture Filter:");
                                ui.add_space(8.0);
                                let mut filter = self.settings_manager.get().texture_filter.clone();
                                egui::ComboBox::from_label("")
                                    .selected_text(&filter)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut filter,
                                            "nearest".to_string(),
                                            "Nearest (fast)",
                                        );
                                        ui.selectable_value(
                                            &mut filter,
                                            "linear".to_string(),
                                            "Linear (smooth)",
                                        );
                                        ui.selectable_value(
                                            &mut filter,
                                            "mipmap".to_string(),
                                            "Mipmap (best)",
                                        );
                                    });

                                let current_filter =
                                    self.settings_manager.get().texture_filter.clone();
                                if filter != current_filter {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.texture_filter = filter.clone();
                                    });
                                    if !self.image_entries.is_empty() {
                                        // Force reload of virtual texture if present
                                        self.virtual_texture = None;
                                        self.vt_progress = None;
                                        self.vt_total_tiles = 0;
                                        self.virtual_texture_thread = None;
                                        self.load_current_image_with_cache();
                                    }
                                }
                            });

                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);

                            // Startup settings (moved here from separate section)
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let mut start_fs = self.settings_manager.get().start_fullscreen;
                                if ui
                                    .checkbox(&mut start_fs, "Start in Fullscreen Mode")
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.start_fullscreen = start_fs;
                                    });
                                }
                            });

                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(
                                    "Note: When started in fullscreen, Escape will close the app.",
                                );
                            });
                            ui.add_space(4.0);
                        });

                        ui.add_space(4.0);

                        // ========== SLIDESHOW SETTINGS ==========
                        ui.collapsing(egui::RichText::new("🎬 Slideshow").size(15.0), |ui| {
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let mut enabled = self.slideshow_enabled;
                                if ui.checkbox(&mut enabled, "Enable Slideshow").changed() {
                                    self.slideshow_enabled = enabled;
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.slideshow_enabled = enabled;
                                    });
                                    self.update_window_title(ctx);
                                }
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Interval (seconds):");
                                ui.add_space(8.0);
                                let mut interval_secs = self.slideshow_interval.as_secs_f32();
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut interval_secs)
                                            .range(0.5..=60.0)
                                            .speed(0.5)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let interval_ms = (interval_secs * 1000.0) as u64;
                                    self.slideshow_interval = Duration::from_millis(interval_ms);
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.slideshow_interval_ms = interval_ms;
                                    });
                                }
                                ui.add_space(4.0);
                                ui.label(format!("{:.1}s", interval_secs));
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let mut loop_enabled = self.slideshow_loop;
                                if ui.checkbox(&mut loop_enabled, "Loop").changed() {
                                    self.slideshow_loop = loop_enabled;
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.slideshow_loop = loop_enabled;
                                    });
                                }
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let mut random_enabled = self.slideshow_random;
                                if ui.checkbox(&mut random_enabled, "Random Order").changed() {
                                    self.slideshow_random = random_enabled;
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.slideshow_random = random_enabled;
                                    });
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Shortcuts:");
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::LIGHT_GREEN, "L");
                                ui.label("= Toggle, ");
                                ui.colored_label(egui::Color32::LIGHT_GREEN, ",");
                                ui.label("= Slower, ");
                                ui.colored_label(egui::Color32::LIGHT_GREEN, ".");
                                ui.label("= Faster");
                            });
                            ui.add_space(4.0);
                        });

                        ui.add_space(4.0);

                        // ========== ADVANCED SETTINGS ==========
                        ui.collapsing(egui::RichText::new("⚙ Advanced").size(15.0), |ui| {
                            ui.add_space(4.0);

                            // ===== VT SETTINGS =====
                            ui.label(
                                egui::RichText::new("⚙ Virtual texture Settings")
                                    .size(13.0)
                                    .strong(),
                            );

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Tile Size:");
                                ui.add_space(8.0);
                                let mut tile = self.settings_manager.get().tile_size;
                                egui::ComboBox::from_label("")
                                    .selected_text(format!("{}px", tile))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut tile, 128, "128px");
                                        ui.selectable_value(&mut tile, 256, "256px");
                                        ui.selectable_value(&mut tile, 512, "512px");
                                        ui.selectable_value(&mut tile, 1024, "1024px");
                                        ui.selectable_value(&mut tile, 2048, "2048px");
                                    });
                                if tile != self.settings_manager.get().tile_size {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.tile_size = tile;
                                    });
                                    // reload the current image if i
                                    if !self.image_entries.is_empty() {
                                        // Force reload of virtual texture
                                        self.virtual_texture = None;
                                        self.vt_progress = None;
                                        self.vt_total_tiles = 0;
                                        self.virtual_texture_thread = None;
                                        self.load_current_image_with_cache();
                                    }
                                }
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("VT Threshold:");
                                ui.add_space(8.0);
                                let mut threshold =
                                    self.settings_manager.get().virtual_texture_threshold;
                                if ui
                                    .add(egui::Slider::new(&mut threshold, 4096..=16384).text("px"))
                                    .changed()
                                {
                                    if threshold
                                        != self.settings_manager.get().virtual_texture_threshold
                                    {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.virtual_texture_threshold = threshold;
                                        });
                                        // Reload current image if virtual texturing is used
                                        if !self.image_entries.is_empty() {
                                            // Force reload of virtual texture
                                            self.virtual_texture = None;
                                            self.vt_progress = None;
                                            self.vt_total_tiles = 0;
                                            self.virtual_texture_thread = None;
                                            self.load_current_image_with_cache();
                                        }
                                    }
                                }
                            });

                            ui.add_space(4.0);

                            // ===== CACHE SETTINGS =====
                            ui.label(egui::RichText::new("💾 Cache Settings").size(13.0).strong());
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Cache Radius:");
                                ui.add_space(8.0);
                                let mut radius = self.cache_radius;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut radius)
                                            .range(1..=100)
                                            .speed(1)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    if radius != self.cache_radius {
                                        self.update_cache_radius(radius);
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.cache_radius = radius;
                                        });
                                        if !self.image_entries.is_empty() && !self.is_loading() {
                                            self.preload_adjacent_images();
                                        }
                                    }
                                }
                                ui.add_space(4.0);
                                ui.label(format!("({})", radius));
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Delta Factor:");
                                ui.add_space(8.0);
                                let mut factor = self.cache_delta_factor;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut factor)
                                            .range(0.1..=1.0)
                                            .speed(0.05)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    if factor != self.cache_delta_factor {
                                        self.cache_delta_factor = factor;
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.cache_delta_factor = factor;
                                        });
                                    }
                                }
                                ui.add_space(4.0);
                                ui.label(format!("({:.2})", factor));
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Max Tasks:");
                                ui.add_space(8.0);
                                let mut tasks = self.max_cache_task;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut tasks)
                                            .range(1..=10)
                                            .speed(1)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    if tasks != self.max_cache_task {
                                        self.max_cache_task = tasks;
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.max_cache_task = tasks;
                                        });
                                    }
                                }
                                ui.add_space(4.0);
                                ui.label(format!("({})", tasks));
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Preload Throttle:");
                                ui.add_space(8.0);
                                let mut throttle = self.settings_manager.get().preload_throttle_ms;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut throttle)
                                            .range(10..=1000)
                                            .speed(10)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    if throttle != self.settings_manager.get().preload_throttle_ms {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.preload_throttle_ms = throttle;
                                        });
                                    }
                                }
                                ui.add_space(4.0);
                                ui.label(format!("({} ms)", throttle));
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Navigation Pause:");
                                ui.add_space(8.0);
                                let mut pause = self.settings_manager.get().navigation_pause_ms;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut pause)
                                            .range(100..=5000)
                                            .speed(100)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    if pause != self.settings_manager.get().navigation_pause_ms {
                                        let _ = self.settings_manager.update(|settings| {
                                            settings.navigation_pause_ms = pause;
                                            self.navigation_pause_duration =
                                                std::time::Duration::from_millis(pause);
                                        });
                                    }
                                }
                                ui.add_space(4.0);
                                ui.label(format!("({} ms)", pause));
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Cache info with progress bar
                            let total_images = self.image_entries.len();
                            let cached_count = self.image_cache.len();
                            let cache_range = self.get_cache_range();
                            let target_count = (cache_range * 2 + 1).min(total_images);

                            let progress = if target_count > 0 {
                                cached_count as f32 / target_count as f32
                            } else {
                                0.0
                            };

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(format!(
                                    "📊 Cache: {}/{} images",
                                    cached_count, target_count
                                ));
                            });

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.add(egui::ProgressBar::new(progress).desired_width(200.0));
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("🗑️ Clear Cache").size(13.0),
                                        )
                                        .min_size(egui::vec2(100.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    self.image_cache.clear();
                                    self.preloading_indices.clear();
                                    self.preload_tasks.clear();
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // ===== FRAME LIMITER SETTINGS =====
                            ui.label(egui::RichText::new("🎮 Frame Limiter").size(13.0).strong());
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Controls how many frames per second the app renders.");
                            });
                            ui.add_space(6.0);

                            // Max FPS – bind directly to self.max_fps
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Max FPS:");
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.max_fps)
                                            .range(0.0..=120.0)
                                            .speed(1.0)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.max_fps = self.max_fps;
                                    });
                                    // Ensure the frame limiter picks up the new value (already set)
                                    self.load_frame_limiter_settings();
                                    ctx.request_repaint(); // force immediate effect
                                }
                                ui.add_space(4.0);
                                if self.max_fps == 0.0 {
                                    ui.label("Unlimited");
                                } else {
                                    ui.label(format!("{} FPS", self.max_fps));
                                }
                            });

                            ui.add_space(4.0);

                            // Idle FPS
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Idle FPS:");
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.idle_fps_limit)
                                            .range(0.0..=60.0)
                                            .speed(1.0)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.idle_fps_limit = self.idle_fps_limit;
                                    });
                                    self.load_frame_limiter_settings();
                                    ctx.request_repaint();
                                }
                                ui.add_space(4.0);
                                if self.idle_fps_limit == 0.0 {
                                    ui.label("Unlimited");
                                } else {
                                    ui.label(format!("{} FPS", self.idle_fps_limit));
                                }
                                ui.add_space(4.0);
                                ui.label("(when idle)");
                            });

                            ui.add_space(4.0);

                            // Idle Timeout
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Idle Timeout:");
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.idle_timeout_ms)
                                            .range(100..=10000)
                                            .speed(100)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.idle_timeout_ms = self.idle_timeout_ms;
                                    });
                                    self.load_frame_limiter_settings();
                                    ctx.request_repaint();
                                }
                                ui.add_space(4.0);
                                ui.label(format!("{} ms", self.idle_timeout_ms));
                                ui.add_space(4.0);
                                ui.label("(when focused)");
                            });

                            ui.add_space(4.0);

                            // Unfocused Timeout
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Unfocused Timeout:");
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.unfocused_idle_timeout_ms)
                                            .range(50..=5000)
                                            .speed(50)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.unfocused_idle_timeout_ms =
                                            self.unfocused_idle_timeout_ms;
                                    });
                                    self.load_frame_limiter_settings();
                                    ctx.request_repaint();
                                }
                                ui.add_space(4.0);
                                ui.label(format!("{} ms", self.unfocused_idle_timeout_ms));
                                ui.add_space(4.0);
                                ui.label("(when unfocused)");
                            });

                            ui.add_space(4.0);

                            // Unfocused Idle FPS
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label("Unfocused Idle FPS:");
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.unfocused_idle_fps_limit)
                                            .range(0.0..=60.0)
                                            .speed(1.0)
                                            .clamp_existing_to_range(true),
                                    )
                                    .changed()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.unfocused_idle_fps_limit =
                                            self.unfocused_idle_fps_limit;
                                    });
                                    self.load_frame_limiter_settings();
                                    ctx.request_repaint();
                                }
                                ui.add_space(4.0);
                                if self.unfocused_idle_fps_limit == 0.0 {
                                    ui.label("Unlimited");
                                } else {
                                    ui.label(format!("{} FPS", self.unfocused_idle_fps_limit));
                                }
                                ui.add_space(4.0);
                                ui.label("(when unfocused)");
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Show current state
                            let state_text = if self.is_animating {
                                "Animating"
                            } else if self.is_loading() {
                                "Loading"
                            } else if self.is_idle {
                                "Idle"
                            } else {
                                "Active"
                            };

                            let idle_text = if self.idle_fps_limit == 0.0 {
                                "Unlimited".to_string()
                            } else {
                                format!("{} FPS", self.idle_fps_limit)
                            };

                            let focus_text = if ctx.input(|i| i.viewport().focused).unwrap_or(false)
                            {
                                "✅ Focused"
                            } else {
                                "❌ Unfocused"
                            };

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(format!(
                                    "Current: {} | Idle: {} | {}",
                                    state_text, idle_text, focus_text
                                ));
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // ===== WINDOW SETTINGS =====
                            ui.label(egui::RichText::new("🪟 Window").size(13.0).strong());
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("🔄 Reset Window Position")
                                                .size(13.0),
                                        )
                                        .min_size(egui::vec2(160.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.window_pos = None;
                                    });
                                }
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("🔄 Reset Window Size").size(13.0),
                                        )
                                        .min_size(egui::vec2(160.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    let _ = self.settings_manager.update(|settings| {
                                        settings.window_size = None;
                                    });
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("⚠ Reset All Settings to Default")
                                                .size(13.0),
                                        )
                                        .min_size(egui::vec2(220.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    let default_settings = crate::settings::AppSettings::default();
                                    let _ = self.settings_manager.update(|settings| {
                                        *settings = default_settings;
                                    });
                                    // Reload all settings
                                    self.load_frame_limiter_settings();
                                    self.load_slideshow_settings();
                                    self.b_ctrl_invert = self.settings_manager.get().b_ctrl_invert;
                                    self.cache_radius = self.settings_manager.get().cache_radius;
                                    self.cache_delta_factor =
                                        self.settings_manager.get().cache_delta_factor;
                                    self.max_cache_task =
                                        self.settings_manager.get().max_cache_task;
                                    self.navigation_pause_duration =
                                        std::time::Duration::from_millis(
                                            self.settings_manager.get().navigation_pause_ms,
                                        );
                                    // Clear cache and reload
                                    self.image_cache.clear();
                                    self.preloading_indices.clear();
                                    self.preload_tasks.clear();
                                    if !self.image_entries.is_empty() {
                                        self.load_current_image_with_cache();
                                    }
                                    self.update_window_title(ctx);
                                }
                            });
                            ui.add_space(4.0);
                        });

                        ui.add_space(8.0);
                    });
            });

        // Update the actual state after the window closes
        if close_requested {
            self.show_settings_menu = false;
        } else {
            // The window might have been closed via X button, check the open state
            self.show_settings_menu = open;
        }
    }

    pub fn toggle_settings_menu(&mut self) {
        self.show_settings_menu = !self.show_settings_menu;
    }
}
