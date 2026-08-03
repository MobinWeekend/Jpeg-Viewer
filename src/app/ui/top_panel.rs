use crate::app::types::ViewerApp;
use crate::app::aspect_ratio::AspectRatio;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            // Use a horizontal scrollable area to prevent overflow
            egui::ScrollArea::horizontal()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // ========== LEFT SECTION ==========
                        // Open button
                        if ui.button("📂 Open").clicked() {
                            self.handle_command(ctx, ViewerCommand::OpenFile);
                        }
                        ui.add_space(8.0);

                        // Zoom display
                        ui.label(format!(
                            "Zoom: {}%",
                            (self.zoom * 100.0).round().max(1.0) as i32
                        ));
                        ui.add_space(8.0);

                        // Cache info
                        let total_images = self.image_entries.len();
                        let cached_count = self.image_cache.len();
                        let cache_range = self.get_cache_range();
                        let target_count = (cache_range * 2 + 1).min(total_images);
                        ui.label(format!("📦 {}/{}", cached_count, target_count));
                        ui.add_space(8.0);

                        // ========== IMAGE INFO ==========
                        if let Some(texture) = &self.texture {
                            let size = texture.size_vec2();
                            let width = size.x as u32;
                            let height = size.y as u32;
                            
                            let file_size_str = self.get_file_size_string();
                            let aspect_ratio_str = AspectRatio::get_label(width, height);
                            
                            ui.separator();
                            ui.add_space(6.0);
                            
                            // Resolution
                            ui.label(format!("{}×{}", width, height));
                            
                            // File size if available
                            if !file_size_str.is_empty() {
                                ui.label(format!("({})", file_size_str));
                            }
                            
                            ui.add_space(4.0);
                            
                            // Aspect ratio
                            if let Some(label) = aspect_ratio_str {
                                ui.colored_label(egui::Color32::LIGHT_BLUE, label);
                            } else {
                                let ratio_display = AspectRatio::format_as_ratio(width, height);
                                ui.colored_label(egui::Color32::LIGHT_YELLOW, format!("{} (Uncommon)", ratio_display));
                            }
                            
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);
                        }

                        // ========== GIF CONTROLS ==========
                        if self.is_gif {
                            if let Some(gif) = &mut self.gif_animation {
                                if gif.is_animated() {
                                    if ui.button(if gif.is_playing { "⏸" } else { "▶" }).clicked() {
                                        gif.toggle_play();
                                    }
                                    ui.label(format!(
                                        " {}/{}",
                                        gif.get_current_frame_index() + 1,
                                        gif.frame_count()
                                    ));
                                    if self.is_preview {
                                        ui.label("⏳");
                                    }
                                    ui.add_space(6.0);
                                    ui.separator();
                                    ui.add_space(6.0);
                                }
                            }
                        }

                        // ========== MIDDLE SECTION - Spacer pushes settings to the right ==========
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Settings button (always visible, on the right)
                            if ui.button("⚙️ Settings").clicked() {
                                self.toggle_settings_menu();
                            }
                            ui.add_space(8.0);

                            // Loading indicator (right side)
                            if self.b_is_loading_full {
                                ui.label("⏳ Loading...");
                                ui.add_space(4.0);
                            }

                            // Scroll zoom indicator
                            if !self.b_ctrl_invert {
                                ui.label("Scroll: ↕ | Ctrl+Scroll: Zoom");
                            } else {
                                ui.label("Scroll: Zoom | Ctrl+Scroll: ↕");
                            }
                            ui.add_space(4.0);
                        });
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