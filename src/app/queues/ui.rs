//! Queue sidebar and thumbnail-card rendering.
use super::super::types::ViewerApp;
use super::storage::absolute_path;
use crate::helpers::is_supported_image;
use crate::ui_theme::scaled_text_size;
use eframe::egui;
use std::path::{Path, PathBuf};

/// Maximum side length of each thumbnail cell.
pub(super) const CELL_MAX: f32 = 240.0;
/// Minimum side length of each thumbnail cell.
pub(super) const CELL_MIN: f32 = 64.0;
/// Gap between grid cells (both axes).
pub(super) const GAP: f32 = 10.0;

/// Choose the number of columns and the card size.
///
/// Cards stay between CELL_MIN and CELL_MAX whenever the available width
/// permits it. The layout uses the fewest columns that can fill the width
/// without exceeding CELL_MAX.
pub(super) fn fit_cards(available: f32, preferred_size: f32) -> (usize, f32) {
    let available = available.max(CELL_MIN);
    let preferred_size = preferred_size.clamp(CELL_MIN, CELL_MAX);
    if preferred_size < 112.0 {
        return (1, available.min(2048.0));
    }
    let columns = (((available + GAP) / (preferred_size + GAP)).floor() as usize)
        .max(1)
        .min(24);
    let card_size = ((available - GAP * (columns as f32 - 1.0)) / columns as f32)
        .min(preferred_size)
        .clamp(CELL_MIN, CELL_MAX);
    (columns, card_size)
}

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
                    egui::Slider::new(&mut entry_size, 64.0..=240.0)
                        .show_value(false)
                        .step_by(8.0),
                )
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
                    if self.queue_mode {
                        self.return_to_previous_view(ctx);
                    }
                    self.queue_paths.clear();
                    self.queue_mode = false;
                    self.queue_status = Some("Queue cleared.".into());
                }
            });
            ui.horizontal(|ui| {
                crate::ui_theme::switch(ui, &mut self.queue_shuffle, "Shuffle next");
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
                    egui::Slider::new(&mut entry_size, 64.0..=240.0)
                        .show_value(false)
                        .step_by(8.0),
                )
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
                                    let mut favorite_entries = Vec::new();
                                    let mut selected_index = 0usize;
                                    for favorite in &self.favorite_paths {
                                        if super::storage::paths_equal(favorite, path) {
                                            selected_index = favorite_entries.len();
                                        }
                                        if favorite.is_dir() {
                                            favorite_entries.extend(
                                                super::actions::collect_folder_images(favorite)
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
                                    self.set_image_entries(favorite_entries, selected_index);
                                    self.queue_status =
                                        Some("Opened favorites in the viewer.".into());
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

    pub fn render_thumbnail_card(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        key: &str,
        label: &str,
        selected: bool,
        auto_scroll: bool,
        thumbnail_width: f32,
        entry_size: f32,
        context_index: Option<usize>,
        context_mode: u8,
        ctx: &egui::Context,
    ) -> bool {
        let card_width = thumbnail_width.max(1.0);
        let compact = entry_size < 112.0;
        let label_height = if compact {
            0.0
        } else {
            (card_width * 0.22).clamp(20.0, 32.0)
        };
        // Compact entries are horizontal rows with a 8:1 width-to-height ratio.
        let card_height = if compact {
            (card_width / 8.0).max(18.0)
        } else {
            card_width.max(1.0)
        };
        let (cell_rect, response) = ui.allocate_exact_size(
            egui::vec2(card_width, card_height),
            egui::Sense::click_and_drag(),
        );
        let visuals = ui.visuals().clone();
        // Short hover animation adds depth without changing layout or hitboxes.
        let hover_t = ctx.animate_bool_with_time(
            egui::Id::new(("thumbnail-card-hover", key)),
            response.hovered(),
            0.14,
        );
        let painter = ui.painter();
        let inner = cell_rect.shrink(3.0);
        let image_rect = if compact {
            let number_width = if context_index.is_some() { 20.0 } else { 0.0 };
            let image_size = (card_height - 4.0).clamp(12.0, 58.0);
            let image_left =
                (inner.left() + number_width + if number_width > 0.0 { 4.0 } else { 0.0 })
                    .min(inner.right());
            egui::Rect::from_min_size(
                egui::pos2(image_left, inner.center().y - image_size * 0.5),
                egui::vec2(
                    image_size.min((inner.right() - image_left).max(1.0)),
                    image_size,
                ),
            )
        } else {
            egui::Rect::from_min_max(
                inner.min,
                egui::pos2(
                    inner.max.x,
                    (inner.max.y - label_height).max(inner.min.y + 1.0),
                ),
            )
        };

        if hover_t > 0.01 || selected {
            let shadow_alpha = (hover_t * 34.0).round() as u8;
            if shadow_alpha > 0 {
                painter.rect_filled(
                    cell_rect.translate(egui::vec2(0.0, 2.0 + hover_t)),
                    9.0,
                    egui::Color32::from_black_alpha(shadow_alpha),
                );
            }
        }
        painter.rect_filled(
            cell_rect,
            9.0,
            if selected {
                visuals.selection.bg_fill
            } else if hover_t > 0.01 {
                visuals.widgets.hovered.bg_fill
            } else {
                visuals.widgets.noninteractive.bg_fill
            },
        );

        if path.is_dir() {
            painter.text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "📁 Folder",
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 14.0)),
                visuals.text_color(),
            );
        } else if let Some(texture) = self.sidebar_thumbnails.get(key) {
            let size = texture.size_vec2();
            if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
                let scale = (image_rect.width() / size.x).min(image_rect.height() / size.y);
                if scale.is_finite() && scale > 0.0 {
                    let draw_size = size * scale;
                    let draw_rect = egui::Rect::from_center_size(image_rect.center(), draw_size);
                    painter.image(
                        texture.id(),
                        draw_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
        } else {
            let message = if !path.exists() {
                "Missing"
            } else if self.sidebar_thumbnail_failed.contains(key) {
                "No preview"
            } else {
                "Loading…"
            };
            painter.text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 11.0)),
                visuals.weak_text_color(),
            );
            if !path.is_dir() && ui.is_rect_visible(image_rect) {
                self.request_sidebar_thumbnail(path.to_path_buf(), key.to_string(), ctx);
            }
        }

        if compact {
            let label_left = image_rect.right() + 6.0;
            let text_right = if context_mode == 1 {
                (inner.right() - 82.0).max(label_left)
            } else {
                inner.right()
            };
            let text_rect = egui::Rect::from_min_max(
                egui::pos2(label_left.min(inner.right()), inner.top()),
                egui::pos2(text_right, inner.bottom()),
            );
            painter.text(
                text_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                fit_label(ui, label, (text_rect.width() - 4.0).max(4.0)),
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 12.0)),
                visuals.text_color(),
            );
        } else {
            let label_rect =
                egui::Rect::from_min_max(egui::pos2(inner.left(), image_rect.bottom()), inner.max);
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                fit_label(ui, label, (label_rect.width() - 8.0).max(8.0)),
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 12.0)),
                visuals.text_color(),
            );
        }

        // The position number is painted inside the image area, not in a separate widget.
        if let Some(index) = context_index {
            let badge_size = if compact {
                egui::vec2(18.0, 18.0)
            } else {
                egui::vec2(24.0, 20.0)
            };
            let badge_rect = if compact {
                egui::Rect::from_min_size(
                    egui::pos2(inner.left(), inner.center().y - badge_size.y * 0.5),
                    badge_size,
                )
            } else {
                egui::Rect::from_min_size(image_rect.min + egui::vec2(3.0, 3.0), badge_size)
            };
            painter.rect_filled(badge_rect, 3.0, egui::Color32::from_black_alpha(205));
            painter.text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                (index + 1).to_string(),
                egui::FontId::proportional(if compact { 10.0 } else { 12.0 }),
                egui::Color32::WHITE,
            );
        }

        if selected {
            painter.rect_stroke(
                cell_rect.shrink(1.0),
                9.0,
                egui::Stroke::new(2.0 as f32, visuals.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        } else {
            let border_color = if hover_t > 0.01 {
                visuals.widgets.hovered.bg_stroke.color
            } else {
                visuals.widgets.noninteractive.bg_stroke.color
            };
            painter.rect_stroke(
                cell_rect,
                9.0,
                egui::Stroke::new(1.0 + hover_t * 0.5, border_color),
                egui::StrokeKind::Inside,
            );
        }

        if auto_scroll && selected && !ui.clip_rect().contains_rect(cell_rect) {
            ui.scroll_to_rect(cell_rect, Some(egui::Align::Min));
        }

        let response = response.on_hover_text(format!(
            "{}\nClick to select; drag to reorder",
            path.display()
        ));
        if context_mode == 1 || context_mode == 2 {
            response.clone().context_menu(|ui| {
                if path.is_dir() {
                    if ui.button("Show folder contents in Browse").clicked() {
                        self.show_folder_contents(path, ctx);
                        ui.close();
                    }
                    if context_index.is_some()
                        && ui.button("Replace folder entry with its images").clicked()
                    {
                        let expanded = super::super::queues::actions::collect_folder_images(path);
                        let index = context_index.unwrap();
                        if context_mode == 1 {
                            self.queue_paths.remove(index);
                            for (offset, item) in expanded.iter().cloned().enumerate() {
                                self.queue_paths
                                    .insert((index + offset).min(self.queue_paths.len()), item);
                            }
                            self.queue_status = Some(format!(
                                "Replaced folder entry with {} image(s).",
                                expanded.len()
                            ));
                        } else if context_mode == 2 {
                            self.favorite_paths.remove(index);
                            for (offset, item) in expanded.iter().cloned().enumerate() {
                                self.favorite_paths
                                    .insert((index + offset).min(self.favorite_paths.len()), item);
                            }
                            self.save_favorites_for_ui();
                        }
                        ui.close();
                    }
                    ui.separator();
                }
                if ui.button("Open With…").clicked() {
                    if let Err(error) = super::super::external_open::open_with(path) {
                        self.queue_status =
                            Some(format!("Could not open external application: {error}"));
                    }
                    ui.close();
                }
                if ui.button("Locate in File Manager").clicked() {
                    if let Err(error) = super::super::external_open::reveal_in_file_manager(path) {
                        self.queue_status = Some(format!("Could not locate file: {error}"));
                    }
                    ui.close();
                }
                if ui
                    .button(if path.is_dir() {
                        "Show folder contents"
                    } else {
                        "Open image"
                    })
                    .clicked()
                {
                    if path.is_dir() {
                        self.show_folder_contents(path, ctx);
                    } else if context_mode == 1 {
                        if let Some(index) = self
                            .queue_paths
                            .iter()
                            .position(|p| super::storage::paths_equal(p, path))
                        {
                            self.open_queue_in_viewer_at(ctx, index);
                        }
                    } else if let Some(_) = self
                        .favorite_paths
                        .iter()
                        .position(|p| super::storage::paths_equal(p, path))
                    {
                        let mut entries = Vec::new();
                        let mut selected_index = 0usize;
                        for favorite in &self.favorite_paths {
                            if super::storage::paths_equal(favorite, path) {
                                selected_index = entries.len();
                            }
                            if favorite.is_dir() {
                                entries.extend(
                                    super::actions::collect_folder_images(favorite)
                                        .into_iter()
                                        .map(crate::image_entry::ImageEntry::File),
                                );
                            } else {
                                entries
                                    .push(crate::image_entry::ImageEntry::File(favorite.clone()));
                            }
                        }
                        if !entries.is_empty() {
                            let selected_index =
                                selected_index.min(entries.len().saturating_sub(1));
                            self.set_image_entries(entries, selected_index);
                        }
                    }
                    ui.close();
                }
                if path.is_file() && ui.button("Rename… (F2)").clicked() {
                    self.begin_rename(path);
                    ui.close();
                }
                if let Some(index) = context_index {
                    if ui.button("Move to position…").clicked() {
                        self.sidebar_number_edit =
                            Some((context_mode, index, (index + 1).to_string()));
                        ui.close();
                    }
                }
                if context_mode == 1 && ui.button("Add to favorites").clicked() {
                    self.add_favorite_path(path.to_path_buf());
                    ui.close();
                }
                if context_mode == 2 && ui.button("Remove from favorites").clicked() {
                    self.remove_favorite_path(path);
                    ui.close();
                }
                if context_mode == 1 && ui.button("Remove from queue").clicked() {
                    self.queue_paths
                        .retain(|p| !super::storage::paths_equal(p, path));
                    self.queue_status = Some("Removed item from queue.".into());
                    ui.close();
                }
                if ui.button("Copy file path").clicked() {
                    match arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(path.display().to_string()))
                    {
                        Ok(()) => self.queue_status = Some("Copied file path to clipboard.".into()),
                        Err(error) => {
                            self.queue_status = Some(format!("Could not copy path: {error}"))
                        }
                    }
                    ui.close();
                }
            });
        }

        let clicked = response.clicked();
        if let Some(index) = context_index {
            if response.drag_started() {
                self.sidebar_drag_index = Some(index);
                self.sidebar_drag_collection = context_mode;
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if self.sidebar_drag_index.is_some()
                && self.sidebar_drag_collection == context_mode
                && response.hovered()
                && ctx.input(|i| i.pointer.any_released())
            {
                if let Some(from) = self.sidebar_drag_index.take() {
                    let pointer_y = ctx
                        .input(|i| i.pointer.interact_pos())
                        .map(|p| p.y)
                        .unwrap_or(cell_rect.center().y);
                    let to = if pointer_y < cell_rect.center().y {
                        index
                    } else {
                        index + 1
                    };
                    let len = match context_mode {
                        0 => self.image_entries.len(),
                        1 => self.queue_paths.len(),
                        2 => self.favorite_paths.len(),
                        _ => 0,
                    };
                    if from < len
                        && to <= len
                        && !(from == index && (to == index || to == index + 1))
                    {
                        match context_mode {
                            0 => {
                                let selected_path = self.current_image_path.clone();
                                let item = self.image_entries.remove(from);
                                let insert_at = if from < to { to.saturating_sub(1) } else { to };
                                self.image_entries
                                    .insert(insert_at.min(self.image_entries.len()), item);
                                if let Some(path) = selected_path {
                                    if let Some(new_index) = self.image_entries.iter().position(|e| matches!(e, crate::image_entry::ImageEntry::File(p) if p == &path)) { self.current_index = new_index; }
                                }
                                self.sidebar_last_current_index = None;
                                self.image_cache.clear();
                                self.preloading_indices.clear();
                                self.preload_tasks.clear();
                                self.preload_generation = self.preload_generation.wrapping_add(1);
                                if !self.image_entries.is_empty() {
                                    self.load_current_image_with_cache();
                                }
                            }
                            1 => {
                                let item = self.queue_paths.remove(from);
                                let insert_at = if from < to { to.saturating_sub(1) } else { to };
                                self.queue_paths
                                    .insert(insert_at.min(self.queue_paths.len()), item);
                            }
                            2 => {
                                let item = self.favorite_paths.remove(from);
                                let insert_at = if from < to { to.saturating_sub(1) } else { to };
                                self.favorite_paths
                                    .insert(insert_at.min(self.favorite_paths.len()), item);
                                self.save_favorites_for_ui();
                            }
                            _ => {}
                        }
                        self.queue_status = Some(format!(
                            "Moved item {} to position {}.",
                            from + 1,
                            to.min(len)
                        ));
                    }
                }
            }
            if response.hovered()
                && self.sidebar_drag_index.is_some()
                && self.sidebar_drag_collection == context_mode
            {
                let pointer_y = ctx
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| p.y)
                    .unwrap_or(cell_rect.center().y);
                let y = if pointer_y < cell_rect.center().y {
                    cell_rect.top()
                } else {
                    cell_rect.bottom()
                };
                ui.painter().line_segment(
                    [
                        egui::pos2(cell_rect.left(), y),
                        egui::pos2(cell_rect.right(), y),
                    ],
                    egui::Stroke::new(3.0 as f32, visuals.selection.stroke.color),
                );
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if context_mode == 0 {
                response.context_menu(|ui| {
                    if ui.button("Open With…").clicked() {
                        if let Err(error) = super::super::external_open::open_with(path) {
                            self.queue_status =
                                Some(format!("Could not open the Open With dialog: {error}"));
                        }
                        ui.close();
                    }
                    if ui.button("Locate in File Manager").clicked() {
                        if let Err(error) =
                            super::super::external_open::reveal_in_file_manager(path)
                        {
                            self.queue_status = Some(format!("Could not locate file: {error}"));
                        }
                        ui.close();
                    }
                    if ui.button("Open image").clicked() {
                        self.navigate_to_index(ctx, index);
                        ui.close();
                    }
                    if path.is_file() && ui.button("Rename… (F2)").clicked() {
                        self.begin_rename(path);
                        ui.close();
                    }
                    if ui.button("Move to position…").clicked() {
                        self.sidebar_number_edit = Some((0, index, (index + 1).to_string()));
                        ui.close();
                    }
                    if ui.button("Add to favorites").clicked() {
                        self.add_favorite_path(path.to_path_buf());
                        ui.close();
                    }
                    if ui.button("Add to queue").clicked() {
                        let added = self.add_queue_path(path.to_path_buf());
                        self.queue_status = Some(if added {
                            "Added image to queue.".to_owned()
                        } else {
                            "Image is already in the queue.".to_owned()
                        });
                        ui.close();
                    }
                    if ui.button("Copy file path").clicked() {
                        match arboard::Clipboard::new().and_then(|mut clipboard| {
                            clipboard.set_text(path.display().to_string())
                        }) {
                            Ok(()) => {
                                self.queue_status = Some("Copied file path to clipboard.".into())
                            }
                            Err(error) => {
                                self.queue_status = Some(format!("Could not copy path: {error}"))
                            }
                        }
                        ui.close();
                    }
                });
            }
            if response.clicked() && context_mode == 0 {
                // Browse's full-cell click selects the corresponding entry in the caller.
            }
        }
        clicked
    }
}

/// Truncate `text` with a middle ellipsis so that it fits within `max_width`
/// pixels when rendered with the button font. Always returns a single-line
/// string. Pixel-accurate; guarantees the label never overflows or wraps.
pub(super) fn fit_label(ui: &egui::Ui, text: &str, max_width: f32) -> String {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let measure = |s: &str| -> f32 {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };

    if measure(text) <= max_width {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    // Start with the ellipsis alone, then greedily grow the prefix/suffix
    // while the result still fits. Keep the last candidate that fit.
    let mut best = "…".to_string();
    if measure(&best) > max_width {
        return best; // ellipsis itself doesn't fit; nothing better to do
    }

    let mut left = 0usize;
    let mut right = chars.len();
    while left < right {
        // Grow the side that's currently shorter (keeps the split balanced).
        let grow_left = left <= chars.len() - right;
        let (next_left, next_right) = if grow_left {
            (left + 1, right)
        } else {
            (left, right - 1)
        };

        let candidate: String = chars[..next_left]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(chars[next_right..].iter())
            .collect();

        if measure(&candidate) <= max_width {
            best = candidate;
            left = next_left;
            right = next_right;
        } else {
            // This side can't grow; try the other side once.
            if grow_left {
                // Try growing from the right instead.
                if right > left {
                    let alt_right = right - 1;
                    let alt: String = chars[..left]
                        .iter()
                        .chain(std::iter::once(&'…'))
                        .chain(chars[alt_right..].iter())
                        .collect();
                    if measure(&alt) <= max_width {
                        best = alt;
                        right = alt_right;
                        continue;
                    }
                }
                break;
            } else {
                break;
            }
        }
    }

    best
}
