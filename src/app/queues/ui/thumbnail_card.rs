//! Custom-painted thumbnail cards used by Browse, Queue, and Favorites.
use super::layout::fit_label;
use crate::app::types::ViewerApp;
use crate::ui_theme::scaled_text_size;
use eframe::egui;
use std::path::Path;

impl ViewerApp {
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
        let card_fill = if selected {
            visuals.selection.bg_fill
        } else if hover_t > 0.01 {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.noninteractive.bg_fill
        };
        painter.rect_filled(cell_rect, 9.0, card_fill);
        // Thumbnail/card labels are custom-painted rather than standard egui labels,
        // so egui's widget foreground resolver cannot adjust them for the selected or
        // hovered fill. Derive their foreground from the fill actually painted here.
        let card_text_color = crate::ui_theme::contrast_foreground(card_fill);
        let card_weak_text_color = if crate::ui_theme::contrast_prefers_light_text(card_fill) {
            egui::Color32::from_rgb(220, 225, 232)
        } else {
            egui::Color32::from_rgb(78, 82, 88)
        };

        if path.is_dir() {
            painter.text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "📁 Folder",
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 14.0)),
                card_text_color,
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
                card_weak_text_color,
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
                card_text_color,
            );
        } else {
            let label_rect =
                egui::Rect::from_min_max(egui::pos2(inner.left(), image_rect.bottom()), inner.max);
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                fit_label(ui, label, (label_rect.width() - 8.0).max(8.0)),
                egui::FontId::proportional(scaled_text_size(ui.ctx(), 12.0)),
                card_text_color,
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
            painter.rect_filled(badge_rect, 3.0, egui::Color32::from_black_alpha(188));
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
                        let expanded = crate::app::queues::actions::collect_folder_images(path);
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
                    if let Err(error) = crate::app::external_open::open_with(path) {
                        self.queue_status =
                            Some(format!("Could not open external application: {error}"));
                    }
                    ui.close();
                }
                if ui.button("Locate in File Manager").clicked() {
                    if let Err(error) = crate::app::external_open::reveal_in_file_manager(path) {
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
                    } else if context_mode == 0 {
                        if let Some(index) = context_index {
                            self.navigate_to_index(ctx, index);
                        }
                    } else if context_mode == 1 {
                        if let Some(index) = self
                            .queue_paths
                            .iter()
                            .position(|p| crate::app::queues::storage::paths_equal(p, path))
                        {
                            self.open_queue_in_viewer_at(ctx, index);
                        }
                    } else if let Some(_) = self
                        .favorite_paths
                        .iter()
                        .position(|p| crate::app::queues::storage::paths_equal(p, path))
                    {
                        if !self.prepare_for_image_change(
                            "Save or discard unsaved edits before opening favorites.",
                        ) {
                            ui.close();
                            return;
                        }
                        let mut entries = Vec::new();
                        let mut selected_index = 0usize;
                        for favorite in &self.favorite_paths {
                            if crate::app::queues::storage::paths_equal(favorite, path) {
                                selected_index = entries.len();
                            }
                            if favorite.is_dir() {
                                entries.extend(
                                    crate::app::queues::actions::collect_folder_images(favorite)
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
                            self.queue_mode = false;
                            self.set_image_entries(entries, selected_index);
                            self.sync_current_entry_after_collection_change(ctx);
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
                        .retain(|p| !crate::app::queues::storage::paths_equal(p, path));
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
                        if let Err(error) = crate::app::external_open::open_with(path) {
                            self.queue_status =
                                Some(format!("Could not open the Open With dialog: {error}"));
                        }
                        ui.close();
                    }
                    if ui.button("Locate in File Manager").clicked() {
                        if let Err(error) =
                            crate::app::external_open::reveal_in_file_manager(path)
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
