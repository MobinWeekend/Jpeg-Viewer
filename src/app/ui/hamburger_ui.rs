use crate::app::types::ViewerApp;
use crate::app::ui::toolbar_frame;
use crate::input::shortcuts::ViewerCommand;
use crate::ui_theme::scaled_text_size;
use eframe::egui;

const MENU_OFFSET: f32 = 8.0;
const HAMBURGER_SIZE: f32 = 34.0;

impl ViewerApp {
    /// Render the application menu button. The sidebar drawer is a separate
    /// window-edge affordance and is intentionally not repeated in this toolbar.
    pub fn render_hamburger_ui(&mut self, ui: &mut egui::Ui) {
        let platform_profile = crate::ui_theme::current_look_profile(ui.ctx());
        let button_label = "☰";
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
                visuals.extreme_bg_color =
                    crate::ui_theme::with_alpha(visuals.extreme_bg_color, 250);
                visuals.text_edit_bg_color = Some(crate::ui_theme::with_alpha(
                    visuals.text_edit_bg_color(),
                    250,
                ));
                visuals.widgets.inactive.bg_fill =
                    crate::ui_theme::with_alpha(visuals.widgets.inactive.bg_fill, 246);
                visuals.widgets.hovered.bg_fill =
                    crate::ui_theme::with_alpha(visuals.widgets.hovered.bg_fill, 248);
                visuals.widgets.active.bg_fill =
                    crate::ui_theme::with_alpha(visuals.widgets.active.bg_fill, 250);
                visuals.widgets.open.bg_fill =
                    crate::ui_theme::with_alpha(visuals.widgets.open.bg_fill, 248);
            }

            ui.vertical(|ui| {
                // ---- Toolbar row (horizontal cluster of icon actions) ----
                ui.horizontal(|ui| {
                    self.hamburger_copy_button(ctx, ui);
                    ui.separator();
                    self.zoom_ui(ctx, ui);
                    ui.separator();
                    self.fullscreen_ui(ctx, ui);
                    ui.add_space(4.0);
                    self.pin_window_ui(ctx, ui);
                });

                ui.separator();

                self.hamburger_open_file_button(ctx, ui);
                self.hamburger_open_folder_button(ctx, ui);
                self.hamburger_open_folder_recursive_button(ctx, ui);
                self.hamburger_open_with_button(ctx, ui);
                self.hamburger_locate_in_file_manager_button(ctx, ui);

                ui.separator();

                self.hamburger_sidebar_toggle_button(ctx, ui);
                self.hamburger_inspect_button(ctx, ui);
                self.hamburger_wrap_window_button(ctx, ui);
                self.hamburger_raw_develop_button(ui);
                self.hamburger_edit_image_button(ctx, ui);
                self.hamburger_delete_image_button(ctx, ui);

                ui.separator();

                self.hamburger_settings_button(ctx, ui);
                self.hamburger_help_button(ctx, ui);

                // ---- Loading indicator (only when loading) ----
                self.hamburger_loading_indicator(ctx, ui);

                ui.separator();
                self.hamburger_exit_button(ctx, ui);
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

    /// "📋 Copy" — copies the current image to the clipboard.
    fn hamburger_copy_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📋 Copy").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(28.0, 28.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Shortcut: Ctrl+C")
            .clicked()
        {
            self.copy_image_to_clipboard();
        }
    }

    /// "📄 Open File" — opens the file picker.
    fn hamburger_open_file_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📄 Open File").size(scaled_text_size(ui.ctx(), 14.0)),
                )
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
    }

    /// "📷 View RAW" / "🖼 RAW developed" — only shown when the current
    /// image is a camera RAW file. Develops the full sensor data on demand.
    fn hamburger_raw_develop_button(&mut self, ui: &mut egui::Ui) {
        let current_is_raw = self
            .image_entries
            .get(self.current_index)
            .is_some_and(|entry| {
                let name = match entry {
                    crate::imaging::entry::ImageEntry::File(path) => path.as_path(),
                    crate::imaging::entry::ImageEntry::Zip(item) => std::path::Path::new(&item.name),
                    crate::imaging::entry::ImageEntry::S7z(item) => std::path::Path::new(&item.name),
                    crate::imaging::entry::ImageEntry::Rar(item) => std::path::Path::new(&item.name),
                };
                name.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(crate::imaging::core::ImageFormat::from_extension)
                    == Some(crate::imaging::core::ImageFormat::CameraRaw)
            });
        if !current_is_raw {
            return;
        }

        let label = if self.showing_raw_developed {
            "🖼  RAW developed (full resolution)"
        } else {
            "📷  View RAW (full quality)"
        };
        if ui
            .add_enabled(
                !self.showing_raw_developed && !self.is_loading(),
                egui::Button::new(
                    egui::RichText::new(label).size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text(
                "Develop the actual RAW sensor data instead of its embedded JPEG preview",
            )
            .clicked()
        {
            self.load_current_image_as_raw();
            self.hamburger_menu_open = false;
        }
    }

    /// "↗ Open With…" — lets the OS choose an application for the original
    /// file. Archive entries are skipped (no persistent standalone path).
    fn hamburger_open_with_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(path) = self.current_image_path.clone() else {
            return;
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("↗  Open With…").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Choose an application using the operating system's Open With dialog")
            .clicked()
        {
            if let Err(error) = super::super::external_open::open_with(&path) {
                self.queue_status = Some(format!("Could not open external application: {error}"));
            }
            self.hamburger_menu_open = false;
        }
    }

    /// "📂 Locate in File Manager" — reveals the current file in the OS file manager.
    fn hamburger_locate_in_file_manager_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(path) = self.current_image_path.clone() else {
            return;
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📂  Locate in File Manager")
                        .size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .clicked()
        {
            if let Err(error) = super::super::external_open::reveal_in_file_manager(&path) {
                self.queue_status = Some(format!("Could not locate file: {error}"));
            }
            self.hamburger_menu_open = false;
        }
    }

    /// "📐 Wrap Window Around Image" — resizes the window to fit the image.
    fn hamburger_wrap_window_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                !self.image_entries.is_empty(),
                egui::Button::new(
                    egui::RichText::new("📐  Wrap Window").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Resize the window around the current image · Ctrl+Alt+W")
            .clicked()
        {
            self.handle_command(ctx, ViewerCommand::WrapWindowToImage);
            self.hamburger_menu_open = false;
        }
    }

    /// "📁 Open Folder" — opens a folder of images.
    fn hamburger_open_folder_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📁  Open Folder").size(scaled_text_size(ui.ctx(), 14.0)),
                )
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
    }

    /// Open a folder and recursively include images from all nested folders.
    fn hamburger_open_folder_recursive_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📂  Open Folder + Subfolders")
                        .size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Open supported images from this folder and every nested folder")
            .clicked()
        {
            self.open_folder_recursively_dialog();
            self.hamburger_menu_open = false;
        }
    }

    /// "🗄️ Show/Hide Sidebar" — toggles the file sidebar visibility.
    fn hamburger_sidebar_toggle_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
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
    }

    /// "✂️ Edit Image" / "Resume Edit Mode" / "Exit Edit Mode" — toggles the
    /// separate image-editing mode. An existing working copy can be resumed
    /// without discarding unsaved changes.
    fn hamburger_edit_image_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.image_entries.is_empty() {
            return;
        }

        let edit_label = if self.show_edit_menu {
            "✂️  Exit Edit Mode"
        } else if self.edit_session.is_some() {
            "✂️  Resume Edit Mode"
        } else {
            "✂️  Edit Image"
        };
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
    }

    /// "🗑 Delete Image…" — moves the selected filesystem image to the
    /// system Trash/Recycle Bin. Disabled for archive entries.
    fn hamburger_delete_image_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let can_delete_current = matches!(
            self.image_entries.get(self.current_index),
            Some(crate::imaging::entry::ImageEntry::File(_))
        );
        if ui
            .add_enabled(
                can_delete_current,
                egui::Button::new(
                    egui::RichText::new("🗑  Delete Image…").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Move the current image to the system Trash/Recycle Bin")
            .clicked()
        {
            self.show_delete_confirmation = true;
            self.hamburger_menu_open = false;
        }
    }

    /// "🔬 Inspect" — toggles the per-channel inspect view (temporary display mode).
    fn hamburger_inspect_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("🔬  Inspect").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Show or hide individual color and alpha channels")
            .clicked()
        {
            self.toggle_inspect_mode(ctx);
            self.hamburger_menu_open = false;
        }
    }

    /// "⚙ Settings" — opens the settings menu.
    fn hamburger_settings_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("⚙  Settings").size(scaled_text_size(ui.ctx(), 14.0)),
                )
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
    }

    /// "❓ Help" — opens the help / shortcuts window.
    fn hamburger_help_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("❓  Help").size(scaled_text_size(ui.ctx(), 14.0)),
                )
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
    }

    /// Loading spinner row — only rendered while an image is loading.
    /// Includes its own separator so it can be moved anywhere without
    /// leaving a stray divider behind.
    fn hamburger_loading_indicator(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.is_loading() {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(egui::RichText::new("Loading...").size(scaled_text_size(ui.ctx(), 12.0)));
        });
    }

    /// "X Exit" — sends the viewport close command.
    fn hamburger_exit_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("X  Exit").size(scaled_text_size(ui.ctx(), 14.0)),
                )
                .min_size(egui::vec2(180.0, 30.0))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("Exit")
            .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
