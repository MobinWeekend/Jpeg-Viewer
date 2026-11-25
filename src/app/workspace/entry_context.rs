//! Single-entry actions shared by the main image view's right-click menu.

use super::helpers::*;
use super::*;
use eframe::egui;
use crate::app::ui::{paint_svg_icon, SvgIcon};

fn entry_context_menu_item(ui: &mut egui::Ui, icon: SvgIcon, label: &str) -> bool {
    let text_color = ui.visuals().text_color();
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font_id, text_color);
    const MENU_MAX_WIDTH: f32 = 360.0;
    ui.set_max_width(MENU_MAX_WIDTH);
    let glyph_width = 19.0;
    // Let the menu grow only to the width required by its longest label.
    // No fixed minimum is applied, so short actions produce a compact menu.
    let desired_width = (galley.size().x + 12.0 + glyph_width + 14.0)
        .min(MENU_MAX_WIDTH);
    let height = ui.spacing().interact_size.y.max(25.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(desired_width, height), egui::Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else {
            &ui.visuals().widgets.hovered
        };
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), visuals.bg_fill);
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    paint_svg_icon(ui, icon_rect, icon, text_color);
    ui.painter().galley(
        egui::pos2(icon_rect.right() + 9.0, rect.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );
    response.clicked()
}

impl crate::app::types::ViewerApp {
    /// Attach the same single-entry operations available from Browse and Masonry
    /// to a plain right-click on the currently displayed image.
    pub(crate) fn show_main_entry_context_menu(
        &mut self,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        let Some(entry) = self.image_entries.get(self.current_index).cloned() else {
            return;
        };
        let path = match &entry {
            ImageEntry::File(path) => Some(path.clone()),
            _ => None,
        };
        let entry_name = entry_display_name(&entry);
        let favorite_style = self.settings_manager.get().favorite_icon_style.clone();

        response.clone().context_menu(|menu| {
            menu.set_min_width(0.0);
            menu.set_max_width(360.0);
            if entry_context_menu_item(menu, SvgIcon::ContextNewTab, "Open in New Tab") {
                self.open_entry_in_image_tab(entry.clone(), true);
                menu.close();
            }
            if path.is_some()
                && entry_context_menu_item(menu, SvgIcon::ContextEdit, "Edit Image…")
            {
                self.start_editing(ctx);
                menu.close();
            }
            if entry_context_menu_item(menu, SvgIcon::ContextShare, "Share…") {
                self.share_entry(&entry);
                menu.close();
            }
            if entry_context_menu_item(menu, SvgIcon::ContextCompare, "Compare…") {
                self.open_compare_workspace(vec![entry.clone()]);
                menu.close();
            }

            menu.separator();
            if entry_context_menu_item(menu, SvgIcon::ContextCopy, "Copy / Extract…") {
                self.copy_or_move_selected(vec![entry.clone()], false);
                menu.close();
            }
            if let Some(path) = &path {
                if entry_context_menu_item(menu, SvgIcon::ContextMove, "Move…") {
                    self.copy_or_move_selected(vec![entry.clone()], true);
                    menu.close();
                }
                if entry_context_menu_item(menu, SvgIcon::ContextDelete, "Delete…") {
                    let tab_id = self
                        .workspace
                        .tabs
                        .get(self.workspace.active_index)
                        .map(|tab| tab.id)
                        .unwrap_or(1);
                    self.workspace.delete_confirmation = Some((tab_id, vec![entry.clone()], 0));
                    menu.close();
                }
                menu.separator();
                if entry_context_menu_item(menu, SvgIcon::ContextOpenWith, "Open With…") {
                    if let Err(error) = crate::app::external_open::open_with(path) {
                        self.queue_status = Some(format!("Could not open external application: {error}"));
                    }
                    menu.close();
                }
                if entry_context_menu_item(menu, SvgIcon::ContextLocate, "Locate in File Manager") {
                    if let Err(error) = crate::app::external_open::reveal_in_file_manager(path) {
                        self.queue_status = Some(format!("Could not locate file: {error}"));
                    }
                    menu.close();
                }
                if entry_context_menu_item(menu, SvgIcon::ContextRename, "Rename… (F2)") {
                    self.begin_rename(path);
                    menu.close();
                }

                menu.separator();
                let in_queue = self
                    .queue_paths
                    .iter()
                    .any(|item| crate::app::queues::paths_equal(item, path));
                if in_queue {
                    if entry_context_menu_item(menu, SvgIcon::ContextQueue, "Remove from Queue") {
                        self.queue_paths
                            .retain(|item| !crate::app::queues::paths_equal(item, path));
                        self.queue_status = Some("Removed image from Queue.".into());
                        menu.close();
                    }
                } else if entry_context_menu_item(menu, SvgIcon::ContextQueue, "Add to Queue") {
                    let added = self.add_queue_path(path.clone());
                    self.queue_status = Some(if added {
                        "Added image to Queue.".into()
                    } else {
                        "Image is already in Queue.".into()
                    });
                    menu.close();
                }

                let in_favorites = self
                    .favorite_paths
                    .iter()
                    .any(|item| crate::app::queues::paths_equal(item, path));
                if in_favorites {
                    if entry_context_menu_item(
                        menu,
                        crate::app::ui::favorite_svg_icon(&favorite_style, true),
                        "Remove from Favorites",
                    ) {
                        self.remove_favorite_path(path);
                        menu.close();
                    }
                } else if entry_context_menu_item(
                    menu,
                    crate::app::ui::favorite_svg_icon(&favorite_style, false),
                    "Add to Favorites",
                ) {
                    self.add_favorite_path(path.clone());
                    menu.close();
                }

                if entry_context_menu_item(menu, SvgIcon::ContextReorder, "Move to position…") {
                    self.sidebar_number_edit = Some((
                        0,
                        self.current_index,
                        (self.current_index + 1).to_string(),
                    ));
                    menu.close();
                }
                if entry_context_menu_item(menu, SvgIcon::ContextCopyPath, "Copy File Path") {
                    self.queue_status = Some(
                        match arboard::Clipboard::new()
                            .and_then(|mut clipboard| clipboard.set_text(path.display().to_string()))
                        {
                            Ok(()) => "Copied file path to clipboard.".into(),
                            Err(error) => format!("Could not copy file path: {error}"),
                        },
                    );
                    menu.close();
                }
            } else {
                match &entry {
                    ImageEntry::Zip(item) => {
                        if entry_context_menu_item(
                            menu,
                            SvgIcon::ContextArchive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, SvgIcon::ContextCopyName, "Copy Entry Name") {
                            self.queue_status = Some(
                                match arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.set_text(item.name.clone()))
                                {
                                    Ok(()) => "Copied archive entry name.".into(),
                                    Err(error) => format!("Could not copy entry name: {error}"),
                                },
                            );
                            menu.close();
                        }
                    }
                    ImageEntry::S7z(item) => {
                        if entry_context_menu_item(
                            menu,
                            SvgIcon::ContextArchive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, SvgIcon::ContextCopyName, "Copy Entry Name") {
                            self.queue_status = Some(
                                match arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.set_text(item.name.clone()))
                                {
                                    Ok(()) => "Copied archive entry name.".into(),
                                    Err(error) => format!("Could not copy entry name: {error}"),
                                },
                            );
                            menu.close();
                        }
                    }
                    ImageEntry::Rar(item) => {
                        if entry_context_menu_item(
                            menu,
                            SvgIcon::ContextArchive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, SvgIcon::ContextCopyName, "Copy Entry Name") {
                            self.queue_status = Some(
                                match arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.set_text(item.name.clone()))
                                {
                                    Ok(()) => "Copied archive entry name.".into(),
                                    Err(error) => format!("Could not copy entry name: {error}"),
                                },
                            );
                            menu.close();
                        }
                    }
                    ImageEntry::File(_) => {}
                }
            }

            menu.separator();
            let compact_name = if entry_name.chars().count() > 34 {
                format!("{}…", entry_name.chars().take(33).collect::<String>())
            } else {
                entry_name.clone()
            };
            menu.label(egui::RichText::new(compact_name).weak().small())
                .on_hover_text(entry_name);
        });
    }
}
