//! Single-entry actions shared by the main image view's right-click menu.

use super::helpers::*;
use super::*;
use eframe::egui;
use crate::app::ui::{paint_svg_icon, SvgIcon};

#[derive(Clone, Copy)]
enum EntryContextIcon {
    NewTab,
    Share,
    Compare,
    Copy,
    Move,
    Delete,
    Edit,
    OpenExternal,
    Locate,
    Rename,
    Queue,
    Favorite,
    Reorder,
    CopyPath,
    Archive,
    CopyName,
}

/// A compact menu row with font glyphs for common actions and theme-tinted SVG
/// assets for secondary actions.
fn entry_context_menu_item(ui: &mut egui::Ui, icon: EntryContextIcon, label: &str) -> bool {
    let text_color = ui.visuals().text_color();
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let glyph = entry_context_glyph(icon);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, text_color);
    // Shrink-wrap menu rows to their content. Stretching each custom row to
    // `available_width` made the context menu consume almost the whole viewer.
    // The menu itself is capped at one third of the active viewport width.
    let viewport_width = ui.ctx().input(|input| input.viewport_rect().width());
    let menu_width_limit = if viewport_width.is_finite() {
        (viewport_width / 3.0).max(180.0)
    } else {
        360.0
    };
    let glyph_width = if glyph.is_some() { 19.0 } else { 15.0 };
    let desired_width = (galley.size().x + 12.0 + glyph_width + 14.0)
        .max(156.0)
        .min(menu_width_limit);
    ui.set_max_width(menu_width_limit);
    let width = desired_width;
    let height = ui.spacing().interact_size.y.max(24.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else {
            &ui.visuals().widgets.hovered
        };
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(3),
            visuals.bg_fill,
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 15.0, rect.center().y),
        egui::vec2(15.0, 15.0),
    );
    if let Some(glyph) = glyph {
        ui.painter().text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(15.0),
            text_color,
        );
    } else {
        paint_entry_context_icon(ui, icon_rect, icon, text_color);
    }
    ui.painter().galley(
        egui::pos2(
            icon_rect.right() + 9.0,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );
    response.clicked()
}

/// Use ordinary font glyphs for common file actions; the remaining actions
/// are mapped to their editable SVG assets below.
fn entry_context_glyph(icon: EntryContextIcon) -> Option<&'static str> {
    match icon {
        EntryContextIcon::NewTab => Some("⊞"),
        EntryContextIcon::Compare => Some("⇄"),
        EntryContextIcon::Copy => Some("⧉"),
        EntryContextIcon::Move => Some("↔"),
        EntryContextIcon::Delete => Some("🗑"),
        EntryContextIcon::Edit => Some("✎"),
        EntryContextIcon::OpenExternal => Some("↗"),
        EntryContextIcon::Rename => Some("✎"),
        EntryContextIcon::CopyPath => Some("🔗"),
        _ => None,
    }
}

fn paint_entry_context_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: EntryContextIcon,
    color: egui::Color32,
) {
    let svg_icon = match icon {
        EntryContextIcon::Share => Some(SvgIcon::ContextShare),
        EntryContextIcon::Locate => Some(SvgIcon::ContextLocate),
        EntryContextIcon::Queue => Some(SvgIcon::ContextQueue),
        EntryContextIcon::Favorite => Some(SvgIcon::ContextFavorite),
        EntryContextIcon::Reorder => Some(SvgIcon::ContextReorder),
        EntryContextIcon::Archive => Some(SvgIcon::ContextArchive),
        EntryContextIcon::CopyName => Some(SvgIcon::ContextCopyName),
        // Common file actions deliberately remain ordinary font glyphs/emojis.
        EntryContextIcon::NewTab
        | EntryContextIcon::Compare
        | EntryContextIcon::Copy
        | EntryContextIcon::Move
        | EntryContextIcon::Delete
        | EntryContextIcon::Edit
        | EntryContextIcon::OpenExternal
        | EntryContextIcon::Rename
        | EntryContextIcon::CopyPath => None,
    };

    if let Some(svg_icon) = svg_icon {
        paint_svg_icon(ui, rect, svg_icon, color);
    }
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

        response.clone().context_menu(|menu| {
            if entry_context_menu_item(menu, EntryContextIcon::NewTab, "Open in New Tab") {
                self.open_entry_in_image_tab(entry.clone(), true);
                menu.close();
            }
            if path.is_some()
                && entry_context_menu_item(menu, EntryContextIcon::Edit, "Edit Image…")
            {
                self.start_editing(ctx);
                menu.close();
            }
            if entry_context_menu_item(menu, EntryContextIcon::Share, "Share…") {
                self.share_entry(&entry);
                menu.close();
            }
            if entry_context_menu_item(menu, EntryContextIcon::Compare, "Compare…") {
                self.open_compare_workspace(vec![entry.clone()]);
                menu.close();
            }

            menu.separator();
            if entry_context_menu_item(menu, EntryContextIcon::Copy, "Copy / Extract…") {
                self.copy_or_move_selected(vec![entry.clone()], false);
                menu.close();
            }
            if let Some(path) = &path {
                if entry_context_menu_item(menu, EntryContextIcon::Move, "Move…") {
                    self.copy_or_move_selected(vec![entry.clone()], true);
                    menu.close();
                }
                if entry_context_menu_item(menu, EntryContextIcon::Delete, "Delete…") {
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
                if entry_context_menu_item(menu, EntryContextIcon::OpenExternal, "Open With…") {
                    if let Err(error) = crate::app::external_open::open_with(path) {
                        self.queue_status = Some(format!("Could not open external application: {error}"));
                    }
                    menu.close();
                }
                if entry_context_menu_item(menu, EntryContextIcon::Locate, "Locate in File Manager") {
                    if let Err(error) = crate::app::external_open::reveal_in_file_manager(path) {
                        self.queue_status = Some(format!("Could not locate file: {error}"));
                    }
                    menu.close();
                }
                if entry_context_menu_item(menu, EntryContextIcon::Rename, "Rename… (F2)") {
                    self.begin_rename(path);
                    menu.close();
                }

                menu.separator();
                let in_queue = self
                    .queue_paths
                    .iter()
                    .any(|item| crate::app::queues::paths_equal(item, path));
                if in_queue {
                    if entry_context_menu_item(menu, EntryContextIcon::Queue, "Remove from Queue") {
                        self.queue_paths
                            .retain(|item| !crate::app::queues::paths_equal(item, path));
                        self.queue_status = Some("Removed image from Queue.".into());
                        menu.close();
                    }
                } else if entry_context_menu_item(menu, EntryContextIcon::Queue, "Add to Queue") {
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
                        EntryContextIcon::Favorite,
                        "Remove from Favorites",
                    ) {
                        self.remove_favorite_path(path);
                        menu.close();
                    }
                } else if entry_context_menu_item(
                    menu,
                    EntryContextIcon::Favorite,
                    "Add to Favorites",
                ) {
                    self.add_favorite_path(path.clone());
                    menu.close();
                }

                if entry_context_menu_item(menu, EntryContextIcon::Reorder, "Move to position…") {
                    self.sidebar_number_edit = Some((
                        0,
                        self.current_index,
                        (self.current_index + 1).to_string(),
                    ));
                    menu.close();
                }
                if entry_context_menu_item(menu, EntryContextIcon::CopyPath, "Copy File Path") {
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
                            EntryContextIcon::Archive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, EntryContextIcon::CopyName, "Copy Entry Name") {
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
                            EntryContextIcon::Archive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, EntryContextIcon::CopyName, "Copy Entry Name") {
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
                            EntryContextIcon::Archive,
                            "Open Containing Archive",
                        ) {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if entry_context_menu_item(menu, EntryContextIcon::CopyName, "Copy Entry Name") {
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
            menu.label(egui::RichText::new(entry_name).weak().small());
        });
    }
}
