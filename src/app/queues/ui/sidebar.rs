//! Queue and favorites tab content.
use super::layout::{fit_cards, CELL_MIN, GAP};
use crate::app::queues::actions;
use crate::app::queues::storage::{absolute_path, paths_equal};
use crate::app::types::ViewerApp;
use crate::helpers::is_supported_image;
use eframe::egui;
use std::path::PathBuf;

impl ViewerApp {
    pub fn render_queue_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Remember the active queue pane's bounds so external drops can be routed
        // to the queue without changing drops elsewhere in the application.
        self.queue_drop_rect = Some(ui.available_rect_before_wrap());

        ui.heading("Queue");
        ui.label(format!("{} items", self.queue_paths.len()));
        let mut entry_size = self.settings_manager.get().queue_entry_size;
        ui.horizontal(|ui| {
            ui.label("Entry size");
            if ui
                .add(
                    crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut entry_size, 64.0..=240.0)
                        .show_value(false)
                        .step_by(8.0),
                ))
                .changed()
            {
                let _ = self
                    .settings_manager
                    .update(|settings| settings.queue_entry_size = entry_size);
            }
            ui.label(if entry_size < 104.0 {
                "Compact"
            } else if entry_size < 176.0 {
                "Medium"
            } else {
                "Large"
            });
        });

        ui.horizontal(|ui| {
            if ui.button("＋ Add files").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    let mut added = 0;
                    for path in paths {
                        added += usize::from(self.add_queue_path(path));
                    }
                    self.queue_status = Some(format!("Added {added} file(s)."));
                }
            }
            if ui.button("＋ Current image").clicked() {
                self.add_current_image_to_queue();
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("＋ Current view")
                .on_hover_text("Add filesystem images currently shown in Browse")
                .clicked()
            {
                self.add_current_folder_images();
            }
            if ui
                .button("Use Browse list if empty")
                .on_hover_text("Only fills the queue when it is empty")
                .clicked()
            {
                if self.queue_paths.is_empty() {
                    self.add_current_folder_images();
                } else {
                    self.queue_status =
                        Some("Queue is not empty; existing items were preserved.".into());
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("＋ Folder…")
                .on_hover_text("Add images from a folder and its subfolders")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    let added = self.add_folder_recursively(&path);
                    self.queue_status = Some(format!("Added {added} image(s) from folder tree."));
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("＋ Add folder as entry…")
                .on_hover_text(
                    "Keep the folder as one queue item; its contents are read when opened",
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    let added = self.add_queue_path(path);
                    self.queue_status = Some(if added {
                        "Added folder as one queue entry.".into()
                    } else {
                        "Folder is already in the queue.".into()
                    });
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Open queue").clicked() {
                self.open_queue_in_viewer(ctx);
            }
            if ui
                .button("Return")
                .on_hover_text("Return to the previous browsing view")
                .clicked()
            {
                self.return_to_previous_view(ctx);
            }
            if ui.button("Save…").clicked() {
                self.save_queue_dialog();
            }
            if ui.button("Load…").clicked() {
                self.load_queue_dialog();
            }
        });
        ui.collapsing("Queue tools", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Export paths…").clicked() {
                    self.export_queue_paths();
                }
                if ui.button("Remap missing root…").clicked() {
                    self.remap_missing_queue_root();
                }
                if ui.button("Remove unavailable").clicked() {
                    let before = self.queue_paths.len();
                    self.queue_paths.retain(|path| path.exists());
                    self.queue_status = Some(format!(
                        "Removed {} unavailable item(s).",
                        before - self.queue_paths.len()
                    ));
                }
                if ui.button("Clear queue").clicked() {
                    // Do not clear queue state if returning to the previous view was
                    // blocked by unsaved edits or no previous view is available.
                    if self.queue_mode && !self.return_to_previous_view(ctx) {
                        return;
                    }
                    self.queue_paths.clear();
                    self.queue_root = None;
                    self.queue_mode = false;
                    self.queue_status = Some("Queue cleared.".into());
                }
            });
            ui.horizontal(|ui| {
                crate::ui_theme::switch(ui, &mut self.queue_repeat, "Repeat queue");
            });
        });
        ui.separator();

        if self.queue_paths.is_empty() {
            ui.label(
                "Queue is empty. Add files, the current image, or the images listed in Browse.",
            );
        } else {
            let queue_items = self.queue_paths.clone();
            let mut remove_index = None;
            let mut move_action: Option<(usize, isize)> = None;
            let mut open_index = None;

            // Dynamic sizing — same logic as the Browse sidebar.
            let available = ui.available_width().max(CELL_MIN);
            let (columns, card_size) =
                fit_cards(available, self.settings_manager.get().queue_entry_size);

            egui::ScrollArea::vertical()
                .id_salt("queue_sidebar_items")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("queue_sidebar_grid")
                        .num_columns(columns)
                        .spacing(egui::vec2(GAP, GAP))
                        .show(ui, |ui| {
                            for (index, path) in queue_items.iter().enumerate() {
                                let name = path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned();
                                let label = if path.exists() {
                                    name
                                } else {
                                    format!("⚠ {name}")
                                };
                                let key = format!("file:{}", absolute_path(path).display());

                                // Card (thumbnail + label) and queue controls,
                                // stacked vertically and centered under each other.
                                let mut card_clicked = false;
                                ui.allocate_ui_with_layout(
                                    egui::vec2(card_size, 0.0),
                                    egui::Layout::top_down(egui::Align::Center),
                                    |ui| {
                                        card_clicked = self.render_thumbnail_card(
                                            ui,
                                            path,
                                            &key,
                                            &label,
                                            false,
                                            false,
                                            card_size,
                                            entry_size,
                                            Some(index),
                                            1,
                                            ctx,
                                        );
                                        // Queue controls overlay the cell: vertically centered at far right in compact mode,
                                        // or across the thumbnail's top-right corner at larger sizes.
                                        let compact = entry_size < 112.0;
                                        let rect = ui.min_rect();
                                        let bw = if compact { 24.0 } else { 26.0 };
                                        let bh = if compact { 18.0 } else { 24.0 };
                                        let gap = 2.0;
                                        for (n, (caption, action)) in
                                            [("↑", -1isize), ("↓", 1isize), ("×", 2isize)]
                                                .into_iter()
                                                .enumerate()
                                        {
                                            let button_rect = if compact {
                                                egui::Rect::from_min_size(
                                                    egui::pos2(
                                                        rect.right() - 3.0 * bw - 2.0 * gap - 3.0
                                                            + n as f32 * (bw + gap),
                                                        rect.center().y - bh * 0.5,
                                                    ),
                                                    egui::vec2(bw, bh),
                                                )
                                            } else {
                                                egui::Rect::from_min_size(
                                                    egui::pos2(
                                                        rect.right()
                                                            - (3.0 - n as f32) * (bw + gap),
                                                        rect.top() + 4.0,
                                                    ),
                                                    egui::vec2(bw, bh),
                                                )
                                            };
                                            if ui
                                                .put(
                                                    button_rect,
                                                    egui::Button::new(caption).small(),
                                                )
                                                .on_hover_text(match action {
                                                    -1 => "Move up",
                                                    1 => "Move down",
                                                    _ => "Remove from queue",
                                                })
                                                .clicked()
                                            {
                                                if action == 2 {
                                                    remove_index = Some(index);
                                                } else {
                                                    move_action = Some((index, action));
                                                }
                                            }
                                        }
                                    },
                                );
                                if card_clicked {
                                    open_index = Some(index);
                                }
                                if (index + 1) % columns == 0 {
                                    ui.end_row();
                                }
                            }
                            if queue_items.len() % columns != 0 {
                                ui.end_row();
                            }
                        });
                });
            if let Some(index) = remove_index {
                self.queue_paths.remove(index);
            }
            if let Some((index, delta)) = move_action {
                let target = index as isize + delta;
                if target >= 0 && (target as usize) < self.queue_paths.len() {
                    self.queue_paths.swap(index, target as usize);
                }
            }
            if let Some(index) = open_index {
                if let Some(path) = self.queue_paths.get(index).cloned() {
                    if path.is_dir() {
                        self.show_folder_contents(&path, ctx);
                    } else {
                        self.open_queue_in_viewer_at(ctx, index);
                    }
                }
            }
        }
        if let Some(status) = self.queue_status.as_deref() {
            ui.separator();
            ui.label(status);
        }

        if let Some((paths, root)) = self.queue_import_prompt.clone() {
            egui::Window::new("Import saved queue")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("The .jvq file contains {} item(s).", paths.len()));
                    ui.label("Add appends unique items; Replace discards the current queue.");
                    ui.horizontal(|ui| {
                        if ui.button("Add to queue").clicked() {
                            let mut added = 0;
                            for path in &paths {
                                added += usize::from(self.add_queue_path(path.clone()));
                            }
                            if self.queue_root.is_none() {
                                self.queue_root = root.clone();
                            }
                            self.queue_status =
                                Some(format!("Added {added} item(s) from saved queue."));
                            self.queue_import_prompt = None;
                        }
                        if ui.button("Replace queue").clicked() {
                            self.queue_paths = paths.clone();
                            self.queue_root = root.clone();
                            self.queue_mode = false;
                            self.queue_status = Some(format!(
                                "Replaced queue with {} item(s).",
                                self.queue_paths.len()
                            ));
                            self.queue_import_prompt = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.queue_import_prompt = None;
                        }
                    });
                });
        }
    }

    pub fn render_favorites_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.ensure_favorites_loaded();
        self.queue_drop_rect = Some(ui.available_rect_before_wrap());
        ui.heading("Favorites");
        ui.label(format!("{} saved item(s)", self.favorite_paths.len()));
        let mut entry_size = self.settings_manager.get().favorites_entry_size;
        ui.horizontal(|ui| {
            ui.label("Entry size");
            if ui
                .add(
                    crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut entry_size, 64.0..=240.0)
                        .show_value(false)
                        .step_by(8.0),
                ))
                .changed()
            {
                let _ = self
                    .settings_manager
                    .update(|settings| settings.favorites_entry_size = entry_size);
            }
            ui.label(if entry_size < 104.0 {
                "Compact"
            } else if entry_size < 176.0 {
                "Medium"
            } else {
                "Large"
            });
        });
        ui.horizontal(|ui| {
            if ui.button("＋ Add current image").clicked() {
                self.add_current_image_to_favorites();
            }
            if ui.button("＋ Add files…").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    for path in paths {
                        self.add_favorite_path(path);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("＋ Add folder as entry…")
                .on_hover_text("Save a folder as one favorite entry")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.add_favorite_path(path);
                }
            }
        });
        ui.separator();
        if self.favorite_paths.is_empty() {
            ui.label("No favorites yet. Right-click an image in Browse or Queue to add it here.");
            return;
        }
        let items = self.favorite_paths.clone();
        let available = ui.available_width().max(CELL_MIN);
        let (columns, card_size) =
            fit_cards(available, self.settings_manager.get().favorites_entry_size);
        egui::ScrollArea::vertical()
            .id_salt("favorites_sidebar_items")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("favorites_sidebar_grid")
                    .num_columns(columns)
                    .spacing(egui::vec2(GAP, GAP))
                    .show(ui, |ui| {
                        for (index, path) in items.iter().enumerate() {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            let label = if path.exists() {
                                name
                            } else {
                                format!("⚠ {name}")
                            };
                            let key = format!("file:{}", absolute_path(path).display());
                            let mut clicked = false;
                            ui.allocate_ui_with_layout(
                                egui::vec2(card_size, 0.0),
                                egui::Layout::top_down(egui::Align::Center),
                                |ui| {
                                    clicked = self.render_thumbnail_card(
                                        ui,
                                        path,
                                        &key,
                                        &label,
                                        false,
                                        false,
                                        card_size,
                                        entry_size,
                                        Some(index),
                                        2,
                                        ctx,
                                    );
                                },
                            );
                            if clicked {
                                if path.is_dir() {
                                    self.show_folder_contents(path, ctx);
                                } else if path.is_file() {
                                    if self.prepare_for_image_change(
                                        "Save or discard unsaved edits before opening favorites.",
                                    ) {
                                        let mut favorite_entries = Vec::new();
                                        let mut selected_index = 0usize;
                                        for favorite in &self.favorite_paths {
                                            if paths_equal(favorite, path) {
                                                selected_index = favorite_entries.len();
                                            }
                                            if favorite.is_dir() {
                                                favorite_entries.extend(
                                                    actions::collect_folder_images(favorite)
                                                        .into_iter()
                                                        .map(crate::image_entry::ImageEntry::File),
                                                );
                                            } else {
                                                favorite_entries.push(
                                                    crate::image_entry::ImageEntry::File(
                                                        favorite.clone(),
                                                    ),
                                                );
                                            }
                                        }
                                        if !favorite_entries.is_empty() {
                                            self.queue_mode = false;
                                            self.set_image_entries(favorite_entries, selected_index);
                                            self.sync_current_entry_after_collection_change(ctx);
                                            self.queue_status =
                                                Some("Opened favorites in the viewer.".into());
                                        }
                                    }
                                } else {
                                    self.queue_status =
                                        Some("This favorite item is unavailable.".into());
                                }
                            }
                            if (index + 1) % columns == 0 {
                                ui.end_row();
                            }
                        }
                        if items.len() % columns != 0 {
                            ui.end_row();
                        }
                    });
            });
    }

    /// Add dropped files to the queue without changing the current viewer contents.
    pub fn add_dropped_paths_to_queue(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0;
        let mut ignored = 0;
        for path in paths {
            if path.is_dir() {
                added += usize::from(self.add_queue_path(path));
            } else if path.is_file() && is_supported_image(&path) {
                added += usize::from(self.add_queue_path(path));
            } else {
                ignored += 1;
            }
        }
        self.queue_status = Some(if ignored == 0 {
            format!("Added {added} dropped item(s) to the queue.")
        } else {
            format!("Added {added} image(s); ignored {ignored} unsupported item(s).")
        });
    }
}
