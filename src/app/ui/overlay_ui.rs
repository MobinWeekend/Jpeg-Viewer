use crate::app::constants::OVERLAY_HIDE_DELAY;
use crate::app::types::ViewerApp;
use eframe::egui;

const TOOLBAR_BG_ALPHA: u8 = 217;
const MENU_OFFSET: f32 = 8.0;
const TITLE_BAR_HEIGHT: f32 = 36.0;
const RESIZE_HANDLE_SIZE: f32 = 6.0;

impl ViewerApp {
    pub fn update_overlay_visibility(&mut self, ctx: &egui::Context) {
        if self.image_entries.is_empty() {
            return;
        }

        let (window_focused, mouse_over_window) = ctx.input(|i| {
            (
                i.viewport().focused.unwrap_or(false),
                i.pointer.hover_pos().is_some(),
            )
        });

        if !window_focused || !mouse_over_window {
            self.set_overlay_visible(ctx, false);
            ctx.set_cursor_icon(egui::CursorIcon::Default);
            return;
        }

        let mouse_over_ui = ctx.is_pointer_over_area();
        let elapsed = self.last_interaction_time.elapsed();

        let should_hide =
            !mouse_over_ui && !self.hamburger_menu_open && elapsed >= OVERLAY_HIDE_DELAY;

        if should_hide {
            self.set_overlay_visible(ctx, false);
            ctx.set_cursor_icon(egui::CursorIcon::None);
        } else {
            self.set_overlay_visible(ctx, true);
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }
    }

    fn set_overlay_visible(&mut self, ctx: &egui::Context, visible: bool) {
        if self.overlay_visible != visible {
            self.overlay_visible = visible;
            ctx.request_repaint();
        }
    }

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
    fn overlay_area(id: &'static str, anchor: egui::Align2, offset: egui::Vec2) -> egui::Area {
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

    // Titlebar
    fn title_bar_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let titlebar_visible = self.settings_manager.settings.show_titlebar;
        if titlebar_visible {
            return;
        }

        let window_width = ctx.available_rect().width();

        ui.set_min_width(window_width);
        ui.set_max_width(window_width);
        ui.set_min_height(TITLE_BAR_HEIGHT);

        let panel_color = ctx.style().visuals.panel_fill;
        let background = egui::Color32::from_rgba_unmultiplied(
            panel_color.r(),
            panel_color.g(),
            panel_color.b(),
            TOOLBAR_BG_ALPHA,
        );

        let frame = egui::Frame::new()
            .fill(background)
            .inner_margin(egui::Margin::symmetric(8, 4));

        frame.show(ui, |ui| {
            ui.set_min_width(window_width - 16.0);
            // Drag area
            let drag_response = ui.interact(
                ui.max_rect(),
                ui.id().with("window_drag"),
                egui::Sense::click_and_drag(),
            );

            if drag_response.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            } else if drag_response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.horizontal(|ui| {
                // Window title
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);

                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.window_title()).size(14.0).strong(),
                        )
                        .selectable(false),
                    );
                });

                // Push window controls to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    const BUTTON_WIDTH: f32 = 46.0;
                    const BUTTON_HEIGHT: f32 = 28.0;

                    let visuals = ui.style().visuals.clone();

                    // Close
                    let close_button = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("X")
                                    .size(14.0)
                                    .color(visuals.text_color()),
                            )
                            .frame(false)
                            .min_size(egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT)),
                        )
                        .on_hover_text("Close");

                    if close_button.hovered() {
                        ui.painter().rect_filled(
                            close_button.rect,
                            0.0,
                            egui::Color32::from_rgb(196, 43, 28),
                        );

                        ui.painter().text(
                            close_button.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "X",
                            egui::FontId::proportional(14.0),
                            egui::Color32::WHITE,
                        );
                    }

                    if close_button.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // Maximize / Restore
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

                    let icon = if maximized { "❐" } else { "□" };

                    let maximize_button = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(icon)
                                    .size(14.0)
                                    .color(visuals.text_color()),
                            )
                            .frame(false)
                            .min_size(egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT)),
                        )
                        .on_hover_text(if maximized { "Restore" } else { "Maximize" });

                    if maximize_button.hovered() {
                        ui.painter().rect_filled(
                            maximize_button.rect,
                            0.0,
                            visuals.widgets.hovered.bg_fill,
                        );
                    }

                    if maximize_button.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }

                    // Minimize
                    let minimize_button = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("—")
                                    .size(15.0)
                                    .color(visuals.text_color()),
                            )
                            .frame(false)
                            .min_size(egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT)),
                        )
                        .on_hover_text("Minimize");

                    if minimize_button.hovered() {
                        ui.painter().rect_filled(
                            minimize_button.rect,
                            0.0,
                            visuals.widgets.hovered.bg_fill,
                        );
                    }

                    if minimize_button.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                });
            });
        });
    }

    fn resize_handle(
        ctx: &egui::Context,
        id: &'static str,
        rect: egui::Rect,
        direction: egui::ResizeDirection,
        cursor: egui::CursorIcon,
    ) {
        egui::Area::new(egui::Id::new(id))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let size = rect.size();

                ui.set_min_size(size);
                ui.set_max_size(size);

                let (_id, response) = ui.allocate_space(size);

                let response = ui.interact(
                    response,
                    ui.id().with("resize_handle"),
                    egui::Sense::click_and_drag(),
                );

                if response.hovered() {
                    ctx.set_cursor_icon(cursor);
                }

                if response.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            });
    }

    fn render_resize_handles(&self, ctx: &egui::Context) {
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if is_fullscreen {
            return;
        }

        let rect = ctx.available_rect();
        let s = RESIZE_HANDLE_SIZE;

        // =========================
        // EDGES
        // =========================

        // Top
        Self::resize_handle(
            ctx,
            "resize_top",
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + s, rect.top()),
                egui::pos2(rect.right() - s, rect.top() + s),
            ),
            egui::ResizeDirection::North,
            egui::CursorIcon::ResizeVertical,
        );

        // Bottom
        Self::resize_handle(
            ctx,
            "resize_bottom",
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + s, rect.bottom() - s),
                egui::pos2(rect.right() - s, rect.bottom()),
            ),
            egui::ResizeDirection::South,
            egui::CursorIcon::ResizeVertical,
        );

        // Left
        Self::resize_handle(
            ctx,
            "resize_left",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + s),
                egui::pos2(rect.left() + s, rect.bottom() - s),
            ),
            egui::ResizeDirection::West,
            egui::CursorIcon::ResizeHorizontal,
        );

        // Right
        Self::resize_handle(
            ctx,
            "resize_right",
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - s, rect.top() + s),
                egui::pos2(rect.right(), rect.bottom() - s),
            ),
            egui::ResizeDirection::East,
            egui::CursorIcon::ResizeHorizontal,
        );

        // =========================
        // CORNERS
        // =========================

        // Top-left
        Self::resize_handle(
            ctx,
            "resize_top_left",
            egui::Rect::from_min_size(rect.min, egui::vec2(s, s)),
            egui::ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        );

        // Top-right
        Self::resize_handle(
            ctx,
            "resize_top_right",
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - s, rect.top()),
                rect.max.min(egui::pos2(rect.right(), rect.top() + s)),
            ),
            egui::ResizeDirection::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        );

        // Bottom-left
        Self::resize_handle(
            ctx,
            "resize_bottom_left",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - s),
                egui::pos2(rect.left() + s, rect.bottom()),
            ),
            egui::ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        );

        // Bottom-right
        Self::resize_handle(
            ctx,
            "resize_bottom_right",
            egui::Rect::from_min_max(egui::pos2(rect.right() - s, rect.bottom() - s), rect.max),
            egui::ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        );
    }

    pub fn render_overlay_ui(&mut self, ctx: &egui::Context) {
        if !self.overlay_visible {
            return;
        }

        // Title bar
        Self::overlay_area("title_bar", egui::Align2::CENTER_TOP, egui::vec2(0.0, 0.0)).show(
            ctx,
            |ui| {
                self.title_bar_ui(ctx, ui);
            },
        );

        let titlebar_visible = self.settings_manager.settings.show_titlebar;
        if !titlebar_visible {
            self.render_resize_handles(ctx);
        }
        let menu_offset_y = if titlebar_visible {
            MENU_OFFSET
        } else {
            (MENU_OFFSET / 2.0) + TITLE_BAR_HEIGHT
        };

        // HAMBURGER BUTTON
        Self::overlay_area(
            "hamburger_button",
            egui::Align2::LEFT_TOP,
            egui::vec2(MENU_OFFSET, menu_offset_y),
        )
        .show(ctx, |ui| {
            self.render_hamburger_ui(ui);
        });

        // HAMBURGER MENU
        Self::overlay_area(
            "hamburger_menu",
            egui::Align2::LEFT_TOP,
            egui::vec2(MENU_OFFSET, menu_offset_y + 28.0),
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
            egui::vec2(0.0, menu_offset_y),
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
                });
        });
    }

    fn separator_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(MENU_OFFSET);
        ui.separator();
        ui.add_space(MENU_OFFSET);
    }
}
