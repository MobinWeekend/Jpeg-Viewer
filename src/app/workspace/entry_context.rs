//! Single-entry actions shared by the main image view's right-click menu.

use super::helpers::*;
use super::*;

impl crate::app::types::ViewerApp {
    /// Attach the same single-entry operations available from Browse and Masonry
    /// to a plain right-click on the currently displayed image.
    pub(crate) fn show_main_entry_context_menu(
        &mut self,
        response: &egui::Response,
        _ctx: &egui::Context,
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
            if menu.button("Open in New Tab").clicked() {
                self.open_entry_in_image_tab(entry.clone(), true);
                menu.close();
            }
            if menu.button("Share…").clicked() {
                self.share_entry(&entry);
                menu.close();
            }
            if menu.button("Compare…").clicked() {
                self.open_compare_workspace(vec![entry.clone()]);
                menu.close();
            }

            menu.separator();
            if menu.button("Copy / Extract…").clicked() {
                self.copy_or_move_selected(vec![entry.clone()], false);
                menu.close();
            }
            if let Some(path) = &path {
                if menu.button("Move…").clicked() {
                    self.copy_or_move_selected(vec![entry.clone()], true);
                    menu.close();
                }
                if menu.button("Delete…").clicked() {
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
                if menu.button("Open With…").clicked() {
                    if let Err(error) = crate::app::external_open::open_with(path) {
                        self.queue_status = Some(format!("Could not open external application: {error}"));
                    }
                    menu.close();
                }
                if menu.button("Locate in File Manager").clicked() {
                    if let Err(error) = crate::app::external_open::reveal_in_file_manager(path) {
                        self.queue_status = Some(format!("Could not locate file: {error}"));
                    }
                    menu.close();
                }
                if menu.button("Rename… (F2)").clicked() {
                    self.begin_rename(path);
                    menu.close();
                }

                menu.separator();
                let in_queue = self.queue_paths.iter().any(|item| {
                    crate::app::queues::paths_equal(item, path)
                });
                if in_queue {
                    if menu.button("Remove from Queue").clicked() {
                        self.queue_paths.retain(|item| {
                            !crate::app::queues::paths_equal(item, path)
                        });
                        self.queue_status = Some("Removed image from Queue.".into());
                        menu.close();
                    }
                } else if menu.button("Add to Queue").clicked() {
                    let added = self.add_queue_path(path.clone());
                    self.queue_status = Some(if added {
                        "Added image to Queue.".into()
                    } else {
                        "Image is already in Queue.".into()
                    });
                    menu.close();
                }

                let in_favorites = self.favorite_paths.iter().any(|item| {
                    crate::app::queues::paths_equal(item, path)
                });
                if in_favorites {
                    if menu.button("Remove from Favorites").clicked() {
                        self.remove_favorite_path(path);
                        menu.close();
                    }
                } else if menu.button("Add to Favorites").clicked() {
                    self.add_favorite_path(path.clone());
                    menu.close();
                }

                if menu.button("Move to position…").clicked() {
                    self.sidebar_number_edit = Some((
                        0,
                        self.current_index,
                        (self.current_index + 1).to_string(),
                    ));
                    menu.close();
                }
                if menu.button("Copy File Path").clicked() {
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
                        if menu.button("Open Containing Archive").clicked() {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
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
                        if menu.button("Open Containing Archive").clicked() {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
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
                        if menu.button("Open Containing Archive").clicked() {
                            if let Err(error) = open::that(&item.archive_path) {
                                self.queue_status = Some(format!("Could not open archive: {error}"));
                            }
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
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
