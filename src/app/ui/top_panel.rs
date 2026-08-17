use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            let has_image = !self.image_entries.is_empty();

            ui.horizontal(|ui| {
                // ========== LEFT SECTION ==========
                // Open button with icon
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("📂 Open File").size(14.0))
                            .min_size(egui::vec2(70.0, 28.0)),
                    )
                    .on_hover_text("Load an Image or Archive")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }
                ui.add_space(4.0);

                // Folder button
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("📁 Folder").size(14.0))
                            .min_size(egui::vec2(70.0, 28.0)),
                    )
                    .on_hover_text("Open a Folder of Images")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFolder);
                }
                ui.add_space(8.0);

                if has_image {
                    self.navigation_ui(ctx, ui);
                }
                // ========== SPACER TO PUSH RIGHT SECTION ==========
                ui.add_space(8.0);

                // ========== RIGHT SECTION ==========
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Settings button (far right)
                    let settings_button = ui
                        .add(
                            egui::Button::new(egui::RichText::new("⚙").size(16.0))
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
                    if self.is_loading() {
                        ui.add(egui::Spinner::new());
                        ui.add_space(4.0);
                    }

                    ui.add_space(8.0);

                    /*ui.label(
                        egui::RichText::new(
                            "Scroll: Navigate • Ctrl+Scroll: Zoom • L: Slideshow",
                        )
                        .size(11.0),
                    );*/
                });
            });

            // ========== RENAME SUGGESTION WARNING ==========
            // Show only when the detection belongs to the current image
            // and current navigation generation.
            let rename_suggestion = self
                .file_type_detection
                .as_ref()
                .filter(|detection| {
                    detection.mismatch
                        && detection.index == self.current_index
                        && detection.generation == self.preload_generation
                })
                .map(|detection| {
                    let current = detection
                        .current_extension
                        .as_deref()
                        .unwrap_or("(none)")
                        .to_string();

                    let suggested = detection.detected_extension.clone();

                    (current, suggested)
                });

            if let Some((current, suggested)) = rename_suggestion {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚠")
                            .color(egui::Color32::RED)
                            .size(24.0),
                    );

                    ui.label(format!("Detected .{} (current: .{})", suggested, current));

                    let rename_btn = ui.button(
                        egui::RichText::new(" Rename ")
                            .color(egui::Color32::LIGHT_GREEN)
                            .size(14.0),
                    );

                    if rename_btn.clicked() {
                        self.apply_rename_suggestion();
                    }
                });
            }
        });
    }
}
