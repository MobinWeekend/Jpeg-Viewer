use crate::app::types::ViewerApp;
use crate::app::ui::toolbar_frame;
use crate::shortcuts::ViewerCommand;
use crate::ui_theme::scaled_text_size;
use eframe::egui;

const MENU_OFFSET: f32 = 8.0;
const HAMBURGER_SIZE: f32 = 28.0;

impl ViewerApp {
    /// Render the application menu button. The sidebar drawer is a separate
    /// window-edge affordance and is intentionally not repeated in this toolbar.
    pub fn render_hamburger_ui(&mut self, ui: &mut egui::Ui) {
        let platform_profile = crate::ui_theme::current_look_profile(ui.ctx());
        let button_label = if platform_profile == "cosmic" || platform_profile == "gnome" {
            "☰ Menu"
        } else {
            "☰"
        };
        let win98 = platform_profile == "win98";
        let button_fill = if win98 {
            ui.visuals().widgets.inactive.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        let button_stroke = if win98 {
            ui.visuals().widgets.inactive.bg_stroke
        } else {
            egui::Stroke::NONE
        };
        let button = ui
            .add(
                egui::Button::new(
                    egui::RichText::new(button_label).size(scaled_text_size(ui.ctx(), 20.0)),
                )
                .min_size(egui::vec2(
                    if platform_profile == "cosmic" || platform_profile == "gnome" {
                        76.0
                    } else {
                        HAMBURGER_SIZE
                    },
                    HAMBURGER_SIZE,
                ))
                .fill(button_fill)
                .stroke(button_stroke),
            )
            .on_hover_text("Menu");

        self.hamburger_button_rect = Some(button.rect);
        if button.clicked() {
            self.hamburger_menu_open = !self.hamburger_menu_open;
        }
    }

    pub fn render_hamburger_menu_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.hamburger_menu_open {
            return;
        }

        toolbar_frame(ctx).show(ui, |ui| {
            // Hamburger/popup actions sit over arbitrary photographs, so their
            // surfaces are intentionally almost opaque when glass is enabled.
            // With glass disabled, preserve the selected theme's original opaque palette.
            if crate::ui_theme::os_background_blur_enabled(ctx) {
                let style = ui.style_mut();
                let visuals = &mut style.visuals;
                visuals.window_fill = crate::ui_theme::with_alpha(visuals.window_fill, 250);
                visuals.extreme_bg_color = crate::ui_theme::with_alpha(visuals.extreme_bg_color, 250);
                visuals.text_edit_bg_color = Some(crate::ui_theme::with_alpha(visuals.text_edit_bg_color(), 250));
                visuals.widgets.inactive.bg_fill = crate::ui_theme::with_alpha(visuals.widgets.inactive.bg_fill, 246);
                visuals.widgets.hovered.bg_fill = crate::ui_theme::with_alpha(visuals.widgets.hovered.bg_fill, 248);
                visuals.widgets.active.bg_fill = crate::ui_theme::with_alpha(visuals.widgets.active.bg_fill, 250);
                visuals.widgets.open.bg_fill = crate::ui_theme::with_alpha(visuals.widgets.open.bg_fill, 248);
            }
            ui.vertical(|ui| {
                //Controls
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("📋 Copy").size(scaled_text_size(ui.ctx(), 14.0)))
                                .min_size(egui::vec2(28.0, 28.0))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text("Shortcut: Ctrl+C")
                        .clicked()
                    {
                        self.copy_image_to_clipboard();
                    }
                    ui.separator();
                    self.zoom_ui(ctx, ui);
                    ui.separator();
                    self.fullscreen_ui(ctx, ui);
                    ui.add_space(4.0);
                    self.pin_window_ui(ctx, ui);
                });

                ui.separator();
                // Open File
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("📄 Open File").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text("Load an Image or Archive")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                    self.hamburger_menu_open = false;
                }

                // RAW files open quickly using their largest embedded JPEG preview.
                // Offer full sensor development as an explicit user action.
                let current_is_raw = self.image_entries.get(self.current_index).is_some_and(|entry| {
                    let name = match entry {
                        crate::image_entry::ImageEntry::File(path) => path.as_path(),
                        crate::image_entry::ImageEntry::Zip(item) => std::path::Path::new(&item.name),
                        crate::image_entry::ImageEntry::S7z(item) => std::path::Path::new(&item.name),
                        crate::image_entry::ImageEntry::Rar(item) => std::path::Path::new(&item.name),
                    };
                    name.extension()
                        .and_then(|extension| extension.to_str())
                        .and_then(crate::image_core::ImageFormat::from_extension)
                        == Some(crate::image_core::ImageFormat::CameraRaw)
                });
                if current_is_raw {
                    let label = if self.showing_raw_developed {
                        "🖼  RAW developed (full resolution)"
                    } else {
                        "📷  View RAW (full quality)"
                    };
                    if ui.add_enabled(
                        !self.showing_raw_developed && !self.is_loading(),
                        egui::Button::new(egui::RichText::new(label).size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    ).on_hover_text("Develop the actual RAW sensor data instead of its embedded JPEG preview").clicked() {
                        self.load_current_image_as_raw();
                        self.hamburger_menu_open = false;
                    }
                }

                // Let the OS choose an application for the original file. Archive entries
                // remain in the viewer because they do not have a persistent standalone path.
                if let Some(path) = self.current_image_path.clone() {
                    if ui.add(
                        egui::Button::new(egui::RichText::new("↗  Open With…").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    ).on_hover_text("Choose an application using the operating system's Open With dialog").clicked() {
                        if let Err(error) = super::super::external_open::open_with(&path) {
                            self.queue_status = Some(format!("Could not open external application: {error}"));
                        }
                        self.hamburger_menu_open = false;
                    }
                }

                if let Some(path) = self.current_image_path.clone() {
                    if ui.add(
                        egui::Button::new(egui::RichText::new("📂  Locate in File Manager").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    ).clicked() {
                        if let Err(error) = super::super::external_open::reveal_in_file_manager(&path) {
                            self.queue_status = Some(format!("Could not locate file: {error}"));
                        }
                        self.hamburger_menu_open = false;
                    }
                }

                // Open Folder
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("📁  Open Folder").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text("Open a Folder of Images")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFolder);
                    self.hamburger_menu_open = false;
                }

                ui.separator();

                // File sidebar
                let sidebar_visible = self.settings_manager.get().show_file_sidebar;
                if ui
                    .add(
                        egui::Button::new(if sidebar_visible {
                            "🗄️  Hide Sidebar"
                        } else {
                            "🗄️  Show Sidebar"
                        })
                        .min_size(egui::vec2(180.0, 30.0))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                    )
                    .clicked()
                {
                    let _ = self
                        .settings_manager
                        .update(|settings| settings.show_file_sidebar = !sidebar_visible);
                    self.hamburger_menu_open = false;
                }

                // Toggle the separate image-editing mode. An existing working
                // copy can be resumed without discarding unsaved changes.
                let edit_label = if self.show_edit_menu {
                    "✂️  Exit Edit Mode"
                } else if self.edit_session.is_some() {
                    "✂️  Resume Edit Mode"
                } else {
                    "✂️  Edit Image"
                };
                if !self.image_entries.is_empty() {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(edit_label).size(scaled_text_size(ui.ctx(), 14.0)))
                                .min_size(egui::vec2(180.0, 30.0))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text("Toggle editing mode. Unsaved work stays in a working copy until saved or discarded.")
                        .clicked()
                    {
                        self.start_editing(ctx);
                        self.hamburger_menu_open = false;
                    }
                };
                // Settings
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("⚙  Settings").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text("Settings")
                    .clicked()
                {
                    crate::app::ui::toggle_settings_menu(self);
                    self.hamburger_menu_open = false;
                }

                // Help
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("❓  Help").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text("Keyboard shortcuts and features")
                    .clicked()
                {
                    self.toggle_help_menu(ctx);
                    self.hamburger_menu_open = false;
                }

                // Loading indicator
                if self.is_loading() {
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(egui::RichText::new("Loading...").size(scaled_text_size(ui.ctx(), 12.0)));
                    });
                }

                // Exit
                ui.separator();

                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("X  Exit").size(scaled_text_size(ui.ctx(), 14.0)))
                            .min_size(egui::vec2(180.0, 30.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text("Exit")
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // Close the menu when clicking outside of it.
        let menu_rect = ui.min_rect();

        ctx.input(|input| {
            if let Some(pointer_pos) = input.pointer.interact_pos() {
                if input.pointer.any_pressed() {
                    let hamburger_rect = self.hamburger_button_rect.unwrap_or_else(|| {
                        egui::Rect::from_min_size(
                            egui::pos2(MENU_OFFSET, MENU_OFFSET),
                            egui::vec2(HAMBURGER_SIZE, HAMBURGER_SIZE),
                        )
                    });

                    if !menu_rect.contains(pointer_pos) && !hamburger_rect.contains(pointer_pos) {
                        self.hamburger_menu_open = false;
                    }
                }
            }
        });
    }
}
