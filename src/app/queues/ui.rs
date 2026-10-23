//! Queue sidebar and thumbnail-card rendering.
use super::super::types::ViewerApp;
use super::storage::absolute_path;
use crate::helpers::is_supported_image;
use eframe::egui;
use std::path::{Path, PathBuf};

/// Maximum side length of each thumbnail cell.
pub(super) const CELL_MAX: f32 = 128.0;
/// Minimum side length of each thumbnail cell.
pub(super) const CELL_MIN: f32 = 64.0;
/// Gap between grid cells (both axes).
pub(super) const GAP: f32 = 10.0;

/// Choose the number of columns and the card size.
///
/// Cards stay between CELL_MIN and CELL_MAX whenever the available width
/// permits it. The layout uses the fewest columns that can fill the width
/// without exceeding CELL_MAX.
pub(super) fn fit_cards(available: f32) -> (usize, f32) {
    let available = available.max(CELL_MIN);

    let columns = (((available + GAP) / (CELL_MAX + GAP)).ceil() as usize).clamp(1, 24);

    let card_size =
        ((available - GAP * (columns as f32 - 1.0)) / columns as f32).clamp(CELL_MIN, CELL_MAX);

    (columns, card_size)
}

impl ViewerApp {
    pub fn render_queue_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Remember the active queue pane's bounds so external drops can be routed
        // to the queue without changing drops elsewhere in the application.
        self.queue_drop_rect = Some(ui.available_rect_before_wrap());

        ui.heading("Selection & Queue");
        ui.label(format!("{} item(s)", self.queue_paths.len()));
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Add files…").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    let mut added = 0;
                    for path in paths {
                        added += usize::from(self.add_queue_path(path));
                    }
                    self.queue_status = Some(format!("Added {added} file(s)."));
                }
            }
            if ui.button("➕ Add current image").clicked() {
                self.add_current_image_to_queue();
            }
            if ui.button("Add sidebar files if empty").on_hover_text(
                "Adds all filesystem images currently listed in Browse, but only when the queue is empty."
            ).clicked() {
                if self.queue_paths.is_empty() {
                    self.add_current_folder_images();
                } else {
                    self.queue_status = Some("Queue is not empty; existing items were preserved.".into());
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("📁 Add current folder images").clicked() {
                self.add_current_folder_images();
            }
            if ui.button("Add folder recursively…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    let added = self.add_folder_recursively(&path);
                    self.queue_status = Some(format!("Added {added} image(s) from folder tree."));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open queue").clicked() {
                self.open_queue_in_viewer(ctx);
            }
            if ui.button("Return to previous view").clicked() {
                self.return_to_previous_view(ctx);
            }
            if ui.button("💾 Save…").clicked() {
                self.save_queue_dialog();
            }
            if ui.button("📥 Load…").clicked() {
                self.load_queue_dialog();
            }
            if ui.button("Export paths…").clicked() {
                self.export_queue_paths();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Remap missing root…").clicked() {
                self.remap_missing_queue_root();
            }
            if ui.button("Remove unavailable").clicked() {
                let before = self.queue_paths.len();
                self.queue_paths.retain(|path| path.exists());
                self.queue_status = Some(format!(
                    "Removed {} unavailable queue item(s).",
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
            ui.checkbox(&mut self.queue_shuffle, "Shuffle next");
            ui.checkbox(&mut self.queue_repeat, "Repeat queue");
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
            let (columns, card_size) = fit_cards(available);

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
                                            ui, path, &key, &label, false, false, card_size, None, ctx,
                                        );
                                        // Queue controls, below the label.
                                        ui.horizontal(|ui| {
                                            if ui
                                                .small_button("🔼")
                                                .on_hover_text("Move up")
                                                .clicked()
                                            {
                                                move_action = Some((index, -1));
                                            }
                                            if ui
                                                .small_button("🔽")
                                                .on_hover_text("Move down")
                                                .clicked()
                                            {
                                                move_action = Some((index, 1));
                                            }
                                            if ui
                                                .small_button("✖️")
                                                .on_hover_text("Remove from queue")
                                                .clicked()
                                            {
                                                remove_index = Some(index);
                                            }
                                        });
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
                self.open_queue_in_viewer_at(ctx, index);
            }
        }
        if let Some(status) = self.queue_status.as_deref() {
            ui.separator();
            ui.label(status);
        }
    }

    /// Add dropped files to the queue without changing the current viewer contents.
    pub fn add_dropped_paths_to_queue(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0;
        let mut ignored = 0;
        for path in paths {
            if path.is_dir() {
                added += self.add_folder_recursively(&path);
            } else if path.is_file() && is_supported_image(&path) {
                added += usize::from(self.add_queue_path(path));
            } else {
                ignored += 1;
            }
        }
        self.queue_status = Some(if ignored == 0 {
            format!("Added {added} dropped image(s) to the queue.")
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
        context_index: Option<usize>,
        ctx: &egui::Context,
    ) -> bool {
        let card_width = thumbnail_width.max(1.0);
        let label_height = (card_width * 0.25).clamp(20.0, 32.0);
        let image_height = (card_width - label_height).max(1.0);

        let mut clicked = false;

        ui.vertical(|ui| {
            ui.set_width(card_width);

            // Image area. Leave unused space unpainted.
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(card_width, image_height), egui::Sense::click());

            if let Some(texture) = self.sidebar_thumbnails.get(key) {
                let size = texture.size_vec2();

                // Validate dimensions before calculating the scale.
                if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
                    // Contain-fit: preserve the original aspect ratio and
                    // ensure the entire image fits inside the image area.
                    let scale_x = rect.width() / size.x;
                    let scale_y = rect.height() / size.y;
                    let scale = scale_x.min(scale_y);

                    if scale.is_finite() && scale > 0.0 {
                        let draw_size = size * scale;
                        let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);

                        ui.painter().image(
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

                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    message,
                    egui::FontId::proportional(11.0),
                    ui.visuals().weak_text_color(),
                );

                if ui.is_rect_visible(rect) {
                    self.request_sidebar_thumbnail(path.to_path_buf(), key.to_string());
                }
            }

            // Selection outline around the image area.
            if selected {
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(2.0 as f32, ui.visuals().selection.stroke.color),
                    egui::StrokeKind::Inside,
                );
            }

            // Scroll the selected image into view when selection changes.
            if auto_scroll && selected && !ui.clip_rect().contains_rect(rect) {
                ui.scroll_to_rect(rect, Some(egui::Align::Min));
            }

            let response = response.on_hover_text(path.display().to_string());
            clicked = response.clicked();
            if let Some(index) = context_index {
                response.context_menu(|ui| {
                    if ui.button("Open image").clicked() {
                        self.navigate_to_index(ctx, index);
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
                        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(path.display().to_string())) {
                            Ok(()) => self.queue_status = Some("Copied file path to clipboard.".into()),
                            Err(error) => self.queue_status = Some(format!("Could not copy path: {error}")),
                        }
                        ui.close();
                    }
                    if ui.button("Open containing folder").clicked() {
                        let target = path.parent().unwrap_or(path);
                        if let Err(error) = open::that(target) {
                            self.queue_status = Some(format!("Could not open folder: {error}"));
                        }
                        ui.close();
                    }
                });
            }

            // Filename label — forced to a single line.
            // Give `fit_label` a slightly conservative budget so the button's internal
            // padding never causes egui to truncate again.
            let truncated = fit_label(ui, label, (card_width - 8.0).max(8.0));
            let text = ui.add_sized(
                [card_width, label_height],
                egui::Button::selectable(selected, truncated).wrap_mode(egui::TextWrapMode::Extend), // no wrap, no extra ellipsis
            );

            clicked |= text.on_hover_text(label).clicked();
        });

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
