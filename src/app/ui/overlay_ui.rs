use crate::app::types::ViewerApp;
use eframe::egui;

const TOOLBAR_BG_ALPHA: u8 = 217;
const MENU_OFFSET: f32 = 8.0;

impl ViewerApp {
    pub fn toolbar_frame(ctx: &egui::Context) -> egui::Frame {
        let panel_color = ctx.style().visuals.panel_fill;

        let background = egui::Color32::from_rgba_unmultiplied(
            panel_color.r(),
            panel_color.g(),
            panel_color.b(),
            TOOLBAR_BG_ALPHA,
        );

        egui::Frame::new()
            .fill(background)
            .inner_margin(egui::Margin::symmetric(6, 6))
            .corner_radius(egui::CornerRadius::same(12))
    }

    /// Create a foreground overlay area at the given anchor and offset.
    fn overlay_area(
        id: &'static str,
        anchor: egui::Align2,
        offset: egui::Vec2,
    ) -> egui::Area {
        egui::Area::new(egui::Id::new(id))
            .order(egui::Order::Foreground)
            .anchor(anchor, offset)
    }

    /// Show a toolbar frame with a horizontal layout.
    fn toolbar_ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        Self::toolbar_frame(ctx).show(ui, |ui| {
            ui.horizontal(|ui| {
                content(self, ui);
            });
        });
    }

    pub fn render_overlay_ui(&mut self, ctx: &egui::Context) {
        // HAMBURGER BUTTON
        Self::overlay_area(
            "hamburger_button",
            egui::Align2::LEFT_TOP,
            egui::vec2(MENU_OFFSET, MENU_OFFSET),
        )
        .show(ctx, |ui| {
            self.render_hamburger_ui(ui);
        });

        // HAMBURGER MENU
        Self::overlay_area(
            "hamburger_menu",
            egui::Align2::LEFT_TOP,
            egui::vec2(MENU_OFFSET, (2.0 * MENU_OFFSET) + 28.0),
        )
        .show(ctx, |ui| {
            self.render_hamburger_menu_ui(ctx, ui);
        });

        // Stop other UI if there is no image.
        if self.image_entries.is_empty() {
            return;
        }

        // TOP CENTER TOOLBAR
        Self::overlay_area(
            "toolbar_top_center",
            egui::Align2::CENTER_TOP,
            egui::vec2(0.0, MENU_OFFSET),
        )
        .show(ctx, |ui| {
            self.toolbar_ui(ctx, ui, |this, ui| {
                this.zoom_ui(ctx, ui);
                this.fullscreen_ui(ctx, ui);
                this.separator_ui(ui);

                this.image_info_ui(ui);

                if this.is_gif {
                    this.separator_ui(ui);
                    this.gif_controls_ui(ui);
                }
            });
        });

        // BOTTOM CENTER TOOLBAR
        Self::overlay_area(
            "toolbar_bot_center",
            egui::Align2::CENTER_BOTTOM,
            egui::vec2(0.0, -MENU_OFFSET),
        )
        .show(ctx, |ui| {
            self.toolbar_ui(ctx, ui, |this, ui| {
                this.navigation_previous_ui(ctx, ui);
                this.slideshow_ui(ctx, ui);
                this.navigation_next_ui(ctx, ui);
            });
        });

        // BOTTOM RIGHT
        Self::overlay_area(
            "toolbar_bot_right",
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-MENU_OFFSET, -MENU_OFFSET),
        )
        .show(ctx, |ui| {
            self.toolbar_ui(ctx, ui, |this, ui| {
                this.render_image_counter(ui);
            });
        });

        self.render_rename_warning(ctx);
    }

    fn render_image_counter(&self, ui: &mut egui::Ui) {
        let total = self.image_entries.len();
        let text = format!("{}/{}", self.current_index + 1, total);

        ui.label(
            egui::RichText::new(text)
                .size(14.0)
                .color(ui.style().visuals.text_color()),
        );
    }

    fn render_rename_warning(&mut self, ctx: &egui::Context) {
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

        let Some((current, suggested)) = rename_suggestion else {
            return;
        };

        Self::overlay_area(
            "rename_warning",
            egui::Align2::CENTER_TOP,
            egui::vec2(0.0, 36.0),
        )
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(egui::CornerRadius::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⚠")
                                .color(egui::Color32::RED)
                                .size(24.0),
                        );

                        ui.label(format!(
                            "Detected .{} (current: .{})",
                            suggested, current
                        ));

                        let rename_btn = ui.button(
                            egui::RichText::new(" Rename ")
                                .color(egui::Color32::LIGHT_GREEN)
                                .size(14.0),
                        );

                        if rename_btn.clicked() {
                            self.apply_rename_suggestion();
                        }
                    });
                });
        });
    }

    fn separator_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(MENU_OFFSET);
        ui.separator();
        ui.add_space(MENU_OFFSET);
    }
}