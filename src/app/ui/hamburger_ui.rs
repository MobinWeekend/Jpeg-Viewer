use crate::app::types::ViewerApp;
use crate::app::ui::toolbar_frame;
use crate::app::ui::{SvgIcon, paint_svg_icon};
use crate::input::shortcuts::ViewerCommand;
use crate::ui_theme::scaled_text_size;
use eframe::egui;

const MENU_OFFSET: f32 = 8.0;
const HAMBURGER_SIZE: f32 = 36.0;

impl ViewerApp {
    /// Render the application menu button. The sidebar drawer is a separate
    /// window-edge affordance and is intentionally not repeated in this toolbar.
    pub fn render_hamburger_ui(&mut self, ui: &mut egui::Ui) {
        let platform_profile = crate::ui_theme::current_look_profile(ui.ctx());
        let win98 = platform_profile == "win98";
        let unframed_custom_titlebar = !self.settings_manager.get().show_titlebar;
        let button_fill = if win98 && !unframed_custom_titlebar {
            ui.visuals().widgets.inactive.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        let button_stroke = if win98 && !unframed_custom_titlebar {
            ui.visuals().widgets.inactive.bg_stroke
        } else {
            egui::Stroke::NONE
        };
        let button_width = if platform_profile == "cosmic" || platform_profile == "gnome" {
            76.0
        } else {
            HAMBURGER_SIZE
        };
        // A blank button provides the normal egui hit target and visual states;
        // its appearance comes from an editable, theme-tinted SVG asset.
        let button = ui
            .add(
                egui::Button::new(egui::RichText::new(""))
                    .min_size(egui::vec2(button_width, HAMBURGER_SIZE))
                    .fill(button_fill)
                    .stroke(button_stroke),
            )
            .on_hover_text("Menu");

        let icon_rect = egui::Rect::from_center_size(button.rect.center(), egui::vec2(24.0, 24.0));
        paint_svg_icon(ui, icon_rect, SvgIcon::Hamburger, ui.visuals().text_color());

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
                ui.set_min_width(220.0);
                ui.set_max_width(240.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                // ---- Toolbar row (horizontal cluster of icon actions) ----
                ui.horizontal(|ui| {
                    //self.hamburger_copy_button(ctx, ui);
                    self.hamburger_share_button(ui);
                    ui.separator();
                    self.hamburger_zoom_button(ctx, ui);
                    ui.add_space(2.0);
                    self.hamburger_fullscreen_button(ctx, ui);
                    ui.add_space(2.0);
                    self.hamburger_pin_button(ctx, ui);
                    ui.separator();
                    self.hamburger_home_button(ui);
                });

                ui.separator();
                self.hamburger_open_file_button(ctx, ui);
                self.hamburger_open_folder_button(ctx, ui);
                self.hamburger_open_folder_recursive_button(ctx, ui);
                self.hamburger_compare_button(ui);
                self.hamburger_open_with_button(ctx, ui);
                self.hamburger_locate_in_file_manager_button(ctx, ui);

                ui.separator();
                self.hamburger_masonry_buttons(ui);

                ui.separator();
                //self.hamburger_sidebar_toggle_button(ctx, ui);
                self.hamburger_inspect_button(ctx, ui);
                self.hamburger_wrap_window_button(ctx, ui);
                self.hamburger_edit_image_button(ctx, ui);
                self.hamburger_delete_image_button(ctx, ui);
                self.hamburger_raw_develop_button(ui);

                // ---- Loading indicator (only when loading) with it's own separator ----
                self.hamburger_loading_indicator(ctx, ui);
                ui.separator();

                // Keep these two frequently used destinations side-by-side.
                // They use half-width hit targets rather than the full-row helper,
                // so their hover regions cannot overlap or spill out of the menu.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let item_width = ((ui.available_width() - 4.0) * 0.5).max(1.0);
                    if Self::hamburger_menu_item_sized(
                        ui,
                        SvgIcon::Settings,
                        "Settings",
                        true,
                        "Application settings",
                        item_width,
                        28.0,
                    ) {
                        crate::app::ui::toggle_settings_menu(self, ctx);
                        self.hamburger_menu_open = false;
                    }
                    if Self::hamburger_menu_item_sized(
                        ui,
                        SvgIcon::Help,
                        "Help & About",
                        true,
                        "Keyboard shortcuts, features, and about",
                        item_width,
                        28.0,
                    ) {
                        self.toggle_help_menu(ctx);
                        self.hamburger_menu_open = false;
                    }
                });
                ui.separator();
                self.hamburger_exit_button(ctx, ui);
            });
        });

        // Close the menu when clicking outside of it.
        let menu_rect = ui.min_rect();

        ctx.input(|input| {
            if let Some(pointer_pos) = input.pointer.interact_pos()
                && input.pointer.any_pressed()
            {
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
        });
    }

    fn hamburger_menu_item(
        ui: &mut egui::Ui,
        icon: SvgIcon,
        label: &str,
        enabled: bool,
        tooltip: &str,
    ) -> bool {
        let width = ui.available_width().max(1.0).min(264.0);
        Self::hamburger_menu_item_sized(ui, icon, label, enabled, tooltip, width, 29.0)
    }

    fn hamburger_menu_item_sized(
        ui: &mut egui::Ui,
        icon: SvgIcon,
        label: &str,
        enabled: bool,
        tooltip: &str,
        width: f32,
        height: f32,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width.max(1.0), height.max(24.0)),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        // Draw the hover/pressed surface ourselves. The hitbox and row geometry
        // are identical in every state, and the hover border is painted inside
        // the allocated rectangle rather than changing widget dimensions.
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else if response.hovered() {
            &ui.visuals().widgets.hovered
        } else if enabled {
            &ui.visuals().widgets.inactive
        } else {
            &ui.visuals().widgets.noninteractive
        };
        if response.hovered() || response.is_pointer_button_down_on() {
            let fill = if visuals.bg_fill == egui::Color32::TRANSPARENT {
                crate::ui_theme::with_alpha(ui.visuals().selection.bg_fill, 72)
            } else {
                visuals.bg_fill
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(5), fill);
            let stroke = if visuals.bg_stroke.width > 0.0 {
                visuals.bg_stroke
            } else {
                ui.visuals().selection.stroke
            };
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(5),
                egui::Stroke::new(0.8, stroke.color),
                egui::StrokeKind::Inside,
            );
        }

        let icon_size = if height < 29.0 { 15.0 } else { 17.0 };
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 17.0, rect.center().y),
            egui::vec2(icon_size, icon_size),
        );
        let tint = if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        paint_svg_icon(ui, icon_rect, icon, tint);
        ui.painter().text(
            egui::pos2(icon_rect.right() + 9.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(scaled_text_size(
                ui.ctx(),
                if height < 29.0 { 12.0 } else { 13.5 },
            )),
            tint,
        );
        response.on_hover_text(tooltip).clicked()
    }

    /// Compact toolbar action buttons use the same explicit hover surface as the
    /// menu rows. The ordinary navigation buttons are intentionally transparent,
    /// which otherwise makes these three hamburger actions appear unresponsive.
    fn hamburger_icon_action_button(
        ui: &mut egui::Ui,
        icon: SvgIcon,
        enabled: bool,
        active: bool,
        tooltip: &str,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(34.0, 30.0),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        if active || response.hovered() || response.is_pointer_button_down_on() {
            let visuals = if response.is_pointer_button_down_on() {
                &ui.visuals().widgets.active
            } else if active {
                &ui.visuals().widgets.open
            } else {
                &ui.visuals().widgets.hovered
            };
            let fill = if visuals.bg_fill == egui::Color32::TRANSPARENT {
                crate::ui_theme::with_alpha(
                    ui.visuals().selection.bg_fill,
                    if active { 112 } else { 72 },
                )
            } else {
                visuals.bg_fill
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(5), fill);
            let stroke = if visuals.bg_stroke.width > 0.0 {
                visuals.bg_stroke
            } else {
                ui.visuals().selection.stroke
            };
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(5),
                egui::Stroke::new(0.8, stroke.color),
                egui::StrokeKind::Inside,
            );
        }

        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0));
        let tint = if enabled {
            if active {
                ui.visuals().selection.stroke.color
            } else {
                ui.visuals().text_color()
            }
        } else {
            ui.visuals().weak_text_color()
        };
        paint_svg_icon(ui, icon_rect, icon, tint);
        response.on_hover_text(tooltip)
    }

    fn hamburger_zoom_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let (icon, tooltip, command) = if self.b_zoom_used {
            (SvgIcon::Fit, "Fit to window", ViewerCommand::MakeFit)
        } else {
            (
                SvgIcon::OneToOne,
                "Make 1:1 ratio",
                ViewerCommand::ResetZoom,
            )
        };
        if Self::hamburger_icon_action_button(ui, icon, true, false, tooltip).clicked() {
            self.handle_command(ctx, command);
        }
    }

    fn hamburger_fullscreen_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if Self::hamburger_icon_action_button(
            ui,
            SvgIcon::Fullscreen,
            true,
            false,
            "Toggle fullscreen",
        )
        .clicked()
        {
            self.toggle_fullscreen(ctx);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_pin_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let (icon, tooltip) = if self.window_always_on_top {
            (SvgIcon::Unpin, "Unpin window from top")
        } else {
            (SvgIcon::Pin, "Pin window to top")
        };
        if Self::hamburger_icon_action_button(ui, icon, true, self.window_always_on_top, tooltip)
            .clicked()
        {
            self.window_always_on_top = !self.window_always_on_top;
            let level = if self.window_always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
    }

    /// Share control in the top toolbar row, using the SVG share symbol.
    fn hamburger_share_button(&mut self, ui: &mut egui::Ui) {
        let enabled =
            !self.is_home_surface_active() && self.image_entries.get(self.current_index).is_some();
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(34.0, 30.0),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        if response.hovered() || response.is_pointer_button_down_on() {
            let visuals = if response.is_pointer_button_down_on() {
                &ui.visuals().widgets.active
            } else {
                &ui.visuals().widgets.hovered
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(5), visuals.bg_fill);
        }
        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0));
        let tint = if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        paint_svg_icon(ui, icon_rect, SvgIcon::ContextShare, tint);
        if response
            .on_hover_text(if enabled {
                "Share current image"
            } else {
                "No image to share"
            })
            .clicked()
        {
            self.share_current_image();
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_open_file_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::OpenFile,
            "Open File…",
            true,
            "Load an image or archive",
        ) {
            self.handle_command(ctx, ViewerCommand::OpenFile);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_open_folder_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::OpenFolder,
            "Open Folder…",
            true,
            "Open a folder of images",
        ) {
            self.handle_command(ctx, ViewerCommand::OpenFolder);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_open_folder_recursive_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::FolderTree,
            "Open Folder + Subfolders…",
            true,
            "Include supported images from nested folders",
        ) {
            self.open_folder_recursively_dialog();
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_home_button(&mut self, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::Home,
            "",
            true,
            "Show Home in the primary Viewer",
        ) {
            self.open_home_page();
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_masonry_buttons(&mut self, ui: &mut egui::Ui) {
        let entries_enabled = !self.image_entries.is_empty();
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::Masonry,
            "Current entries",
            entries_enabled,
            "Open Masonry for current entries",
        ) {
            self.open_local_masonry();
            self.hamburger_menu_open = false;
        }
        let folder_enabled = self.current_directory.is_some();
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::MasonryFolder,
            "Current folder",
            folder_enabled,
            "Open Folder Masonry",
        ) {
            self.open_folder_masonry();
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_raw_develop_button(&mut self, ui: &mut egui::Ui) {
        let current_is_raw = self
            .image_entries
            .get(self.current_index)
            .is_some_and(|entry| {
                let name = match entry {
                    crate::imaging::entry::ImageEntry::File(path) => path.as_path(),
                    crate::imaging::entry::ImageEntry::Zip(item) => {
                        std::path::Path::new(&item.name)
                    }
                    crate::imaging::entry::ImageEntry::S7z(item) => {
                        std::path::Path::new(&item.name)
                    }
                    crate::imaging::entry::ImageEntry::Rar(item) => {
                        std::path::Path::new(&item.name)
                    }
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
            "RAW developed (full resolution)"
        } else {
            "View RAW (full quality)"
        };
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::Raw,
            label,
            !self.showing_raw_developed && !self.is_loading(),
            "Develop the original RAW sensor data",
        ) {
            self.load_current_image_as_raw();
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_compare_button(&mut self, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::ContextCompare,
            "Compare Images…",
            true,
            "Compare images side by side or in rows",
        ) {
            let entries = if self.is_home_surface_active() {
                Vec::new()
            } else {
                self.image_entries
                    .get(self.current_index)
                    .cloned()
                    .into_iter()
                    .collect()
            };
            self.open_compare_workspace(entries);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_open_with_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let path = self.current_image_path.clone();
        let enabled =
            !self.is_home_surface_active() && path.as_ref().is_some_and(|path| path.is_file());
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::ContextOpenWith,
            "Open With…",
            enabled,
            "Choose an application using the operating system",
        ) {
            if let Some(path) = path
                && let Err(error) = super::super::external_open::open_with(&path)
            {
                self.queue_status = Some(format!("Could not open external application: {error}"));
            }
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_locate_in_file_manager_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let path = self.current_image_path.clone();
        let enabled =
            !self.is_home_surface_active() && path.as_ref().is_some_and(|path| path.is_file());
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::ContextLocate,
            "Locate in File Manager",
            enabled,
            "Reveal the current image in the file manager",
        ) {
            if let Some(path) = path
                && let Err(error) = super::super::external_open::reveal_in_file_manager(&path)
            {
                self.queue_status = Some(format!("Could not locate file: {error}"));
            }
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_wrap_window_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let enabled = !self.is_home_surface_active() && !self.image_entries.is_empty();
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::Wrap,
            "Wrap Window Around Image",
            enabled,
            "Resize the window around the current image · Ctrl+Alt+W",
        ) {
            self.handle_command(ctx, ViewerCommand::WrapWindowToImage);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_edit_image_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let can_edit = !self.is_home_surface_active() && !self.image_entries.is_empty();
        let label = if self.show_edit_menu {
            "Exit Edit Mode"
        } else if self.edit_session.is_some() {
            "Resume Edit Mode"
        } else {
            "Edit Image"
        };
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::ContextEdit,
            label,
            can_edit,
            "Edit the current image",
        ) {
            self.start_editing(ctx);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_delete_image_button(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let can_delete = !self.is_home_surface_active()
            && matches!(
                self.image_entries.get(self.current_index),
                Some(crate::imaging::entry::ImageEntry::File(_))
            );
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::ContextDelete,
            "Delete Image…",
            can_delete,
            "Move the current image to the system Trash/Recycle Bin",
        ) {
            self.show_delete_confirmation = true;
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_inspect_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let enabled = !self.is_home_surface_active() && !self.image_entries.is_empty();
        if Self::hamburger_menu_item(
            ui,
            SvgIcon::Inspect,
            "Inspect Channels",
            enabled,
            "Show or hide individual color and alpha channels",
        ) {
            self.toggle_inspect_mode(ctx);
            self.hamburger_menu_open = false;
        }
    }

    fn hamburger_loading_indicator(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.is_loading() {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(egui::RichText::new("Loading…").size(scaled_text_size(ui.ctx(), 12.0)));
        });
    }

    fn hamburger_exit_button(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if Self::hamburger_menu_item(ui, SvgIcon::Exit, "Exit", true, "Close JPEG Viewer") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
