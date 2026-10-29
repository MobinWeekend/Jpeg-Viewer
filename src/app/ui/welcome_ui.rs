/// Draw the welcome/placeholder UI when no image is loaded
use crate::app::types::ViewerApp;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

impl ViewerApp {
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        let available_rect = ui.available_rect_before_wrap();

        // Full welcome area interaction
        let response = ui.interact(
            available_rect,
            ui.id().with("welcome_area"),
            egui::Sense::click_and_drag(),
        );

        // Welcome content
        let available = available_rect.height();
        let content_height = 256.0 + 16.0 + 60.0;

        ui.add_space((available - content_height).max(0.0) * 0.5);

        ui.vertical_centered(|ui| {
            if let Some(icon) = &self.logo_texture {
                ui.image((icon.id(), egui::vec2(256.0, 256.0)));
                ui.add_space(16.0);
            }

            ui.allocate_ui_with_layout(
                egui::vec2(510.0, 32.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(egui::RichText::new("Press ").size(24.0));

                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Ctrl+O").size(24.0).strong(),
                                )
                                .min_size(egui::vec2(80.0, 36.0)),
                            )
                            .clicked()
                        {
                            self.handle_command(ctx, ViewerCommand::OpenFile);
                        }

                        ui.label(
                            egui::RichText::new(" or drag and drop a photo, folder ").size(24.0),
                        );
                    });
                },
            );

            ui.label(
                egui::RichText::new("or a .zip, .7z, or .rar archive containing your photos.")
                    .size(24.0),
            );
            ui.add_space(12.0);
            /*
            ui.label(
                egui::RichText::new("Press F1 for help · Press F3 for sidebar")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Slideshow effects are configurable in settings by pressing F5",
                )
                .size(14.0)
                .color(ui.visuals().weak_text_color()),
            );
             */
            //ui.label(egui::RichText::new("Emoji test: 😀 😂 ❤️ 🎨 🚀 🖥️").size(24.0));
            // For later work:
            //use super::settings_menu::general::general_settings;
            //general_settings(self, ui, ctx);
        });
        // Same mouse behavior as image view
        self.handle_image_mouse_input(ctx, &response);
        //use crate::app::ui::helpers::render_drag_area;
        //render_drag_area(ctx, ui);
    }

    /// Render keyboard shortcut help overlay inside the central image area, never
    /// over the docked file/settings/editor sidebars.
    pub fn render_shortcut_help(&self, ctx: &egui::Context) {
        let settings = self.settings_manager.get();
        let viewport_width = ctx.input(|input| {
            input
                .viewport()
                .inner_rect
                .map(|rect| rect.width())
                .unwrap_or(1280.0)
        });
        let left_width = if settings.show_file_sidebar {
            self.sidebar_actual_width.max(0.0)
        } else {
            0.0
        };
        let right_width = if self.show_edit_menu {
            settings.settings_sidebar_width.clamp(280.0, 560.0)
        } else if self.show_settings_menu {
            settings.settings_sidebar_width.clamp(260.0, 640.0)
        } else {
            0.0
        };
        let central_width = (viewport_width - left_width - right_width).max(0.0);

        // There is no useful room for the overlay on extremely narrow central views.
        // Hiding it is preferable to painting it over a sidebar.
        if central_width < 200.0 {
            return;
        }

        let max_width = (central_width - 16.0).max(180.0);
        let center_offset_x = (left_width - right_width) / 2.0;
        let placement = crate::ui_theme::resolve_main_ui_placement(
            &settings.main_ui_placement_preference,
            ctx,
        );
        // COSMIC's main toolbar sits at bottom center; lift the hints above it on the
        // welcome screen so the two overlays do not collide.
        let bottom_offset_y = if placement == "cosmic" { -84.0 } else { -20.0 };

        egui::Window::new("Keyboard Shortcuts")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_width(max_width)
            .max_width(max_width)
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::Vec2::new(center_offset_x, bottom_offset_y),
            )
            .frame(egui::Frame {
                fill: if settings.os_background_blur {
                    let base = ctx.style().visuals.window_fill();
                    let [r, g, b, _] = base.to_srgba_unmultiplied();
                    egui::Color32::from_rgba_unmultiplied(r, g, b, 196)
                } else {
                    ctx.style().visuals.window_fill()
                },
                corner_radius: if crate::ui_theme::current_look_profile(ctx) == "win98" { egui::CornerRadius::ZERO } else { egui::CornerRadius::same(8) },
                outer_margin: egui::Margin::ZERO,
                inner_margin: egui::Margin::symmetric(16, 12),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.style_mut().interaction.selectable_labels = false;
                ui.set_min_width(0.0);
                ui.set_max_width(max_width);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Keys").size(18.0));
                    ui.add_space(8.0);

                    let shortcuts = [
                        ("F1", "Help"),
                        ("F2", "Rename"),
                        ("F3", "Sidebar"),
                        ("F4", "Compact UI"),
                        ("F5", "Settings"),
                        ("F11", "Fullscreen"),
                    ];

                    for (i, (key, action)) in shortcuts.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("·").color(ui.visuals().weak_text_color()));
                            ui.add_space(4.0);
                        }
                        ui.label(egui::RichText::new(*key).strong().color(ui.visuals().hyperlink_color));
                        ui.label(egui::RichText::new(*action).color(ui.visuals().text_color()));
                    }
                });
            });
    }
}
