use super::types::ViewerApp;
use crate::helpers::is_supported_image;
use crate::image_entry::ImageEntry;
use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};

const QUEUE_MAGIC: &str = "JPEG_VIEWER_QUEUE\t1";

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
            if ui.button("Add files…").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    let mut added = 0;
                    for path in paths {
                        added += usize::from(self.add_queue_path(path));
                    }
                    self.queue_status = Some(format!("Added {added} file(s)."));
                }
            }
            if ui.button("Add current image").clicked() {
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
            if ui.button("Add current folder images").clicked() {
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
            if ui.button("Save…").clicked() {
                self.save_queue_dialog();
            }
            if ui.button("Load…").clicked() {
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
                                            ui, path, &key, &label, false, false, card_size,
                                        );
                                        // Queue controls, below the label.
                                        ui.horizontal(|ui| {
                                            if ui
                                                .small_button("^")
                                                .on_hover_text("Move up")
                                                .clicked()
                                            {
                                                move_action = Some((index, -1));
                                            }
                                            if ui
                                                .small_button("v")
                                                .on_hover_text("Move down")
                                                .clicked()
                                            {
                                                move_action = Some((index, 1));
                                            }
                                            if ui
                                                .small_button("×")
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

    /// Render an asynchronously generated thumbnail and its filename below it.
    /// Returns true when the card is clicked.
    pub fn render_thumbnail_card(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        key: &str,
        label: &str,
        selected: bool,
        auto_scroll: bool,
        thumbnail_width: f32,
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

            clicked = response.on_hover_text(path.display().to_string()).clicked();

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

    fn request_sidebar_thumbnail(&mut self, path: PathBuf, key: String) {
        if !path.is_file()
            || self.sidebar_thumbnail_attempted.contains(&key)
            || self.sidebar_thumbnail_pending.contains(&key)
        {
            return;
        }

        self.sidebar_thumbnail_attempted.insert(key.clone());
        self.sidebar_thumbnail_pending.insert(key.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        self.sidebar_thumbnail_receivers.push(rx);

        std::thread::spawn(move || {
            let result = (|| -> Result<(usize, usize, Vec<u8>), String> {
                let bytes = fs::read(&path)
                    .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

                let decoded = crate::decoder::decode_bytes(&bytes)
                    .map_err(|e| format!("Failed to decode {}: {e}", path.display()))?;

                let source_width = decoded.width();
                let source_height = decoded.height();

                if source_width == 0 || source_height == 0 {
                    return Err(format!(
                        "Invalid source dimensions for {}: {}x{}",
                        path.display(),
                        source_width,
                        source_height
                    ));
                }

                let expected_bytes = (source_width as usize)
                    .checked_mul(source_height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .ok_or_else(|| "Source image dimensions overflow".to_string())?;

                let source_pixels = decoded.data();

                if source_pixels.len() != expected_bytes {
                    return Err(format!(
                        "Invalid RGBA data for {}: dimensions {}x{} require {} bytes, got {}",
                        path.display(),
                        source_width,
                        source_height,
                        expected_bytes,
                        source_pixels.len()
                    ));
                }
                /*
                eprintln!(
                    "[sidebar thumbnail] Source: {} — {}x{}",
                    path.display(),
                    source_width,
                    source_height
                );
                */
                let rgba =
                    image::RgbaImage::from_raw(source_width, source_height, source_pixels.to_vec())
                        .ok_or_else(|| {
                            format!("Could not construct RGBA image for {}", path.display())
                        })?;

                // Fit within a max box while preserving the source aspect ratio.
                // `image::imageops::thumbnail` does NOT preserve AR — it forces exactly
                // the given dimensions — so compute the contained size ourselves.
                const THUMB_MAX_W: u32 = 256;
                const THUMB_MAX_H: u32 = 256;

                let scale = (THUMB_MAX_W as f32 / source_width as f32)
                    .min(THUMB_MAX_H as f32 / source_height as f32);
                let target_w = ((source_width as f32 * scale).round() as u32).max(1);
                let target_h = ((source_height as f32 * scale).round() as u32).max(1);

                let thumbnail = image::imageops::thumbnail(&rgba, target_w, target_h);

                let width = thumbnail.width() as usize;
                let height = thumbnail.height() as usize;

                if width == 0 || height == 0 {
                    return Err(format!(
                        "Thumbnail has invalid dimensions for {}",
                        path.display()
                    ));
                }
                /*
                eprintln!(
                    "[sidebar thumbnail] Result: {} — {}x{}",
                    path.display(),
                    width,
                    height
                );
                 */
                Ok((width, height, thumbnail.into_raw()))
            })();

            let _ = tx.send((key, result));
        });
    }

    pub fn collect_sidebar_thumbnails(&mut self, ctx: &egui::Context) {
        let mut completed = Vec::new();
        self.sidebar_thumbnail_receivers
            .retain(|receiver| match receiver.try_recv() {
                Ok(item) => {
                    completed.push(item);
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
            });
        for (key, result) in completed {
            self.sidebar_thumbnail_pending.remove(&key);
            match result {
                Ok((width, height, pixels)) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
                    let texture = ctx.load_texture(
                        format!("sidebar_thumb_{key}"),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    self.sidebar_thumbnails.insert(key, texture);
                }
                Err(_) => {
                    self.sidebar_thumbnail_failed.insert(key);
                }
            }
        }
    }

    pub fn add_current_image_to_queue(&mut self) {
        let path = self
            .image_entries
            .get(self.current_index)
            .and_then(entry_path)
            .cloned();
        if let Some(path) = path {
            let added = self.add_queue_path(path);
            self.queue_status = Some(
                if added {
                    "Added current image to queue."
                } else {
                    "Current image is already in the queue."
                }
                .into(),
            );
        } else {
            self.queue_status = Some(
                "Only filesystem image files can be queued; archive entries are not supported yet."
                    .into(),
            );
        }
    }

    pub fn add_current_folder_images(&mut self) {
        let paths: Vec<PathBuf> = self
            .image_entries
            .iter()
            .filter_map(entry_path)
            .cloned()
            .collect();
        let mut count = 0;
        for path in paths {
            count += usize::from(self.add_queue_path(path));
        }
        self.queue_status = Some(format!("Added {count} image(s) from the current view."));
    }

    fn add_folder_recursively(&mut self, root: &Path) -> usize {
        let mut stack = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(folder) = stack.pop() {
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() && is_supported_image(&path) {
                    found.push(path);
                }
            }
        }
        found.sort();
        let mut added = 0;
        for path in found {
            added += usize::from(self.add_queue_path(path));
        }
        added
    }

    fn add_queue_path(&mut self, path: PathBuf) -> bool {
        let normalized = absolute_path(&path);
        if self
            .queue_paths
            .iter()
            .any(|existing| paths_equal(existing, &normalized))
        {
            return false;
        }
        self.queue_paths.push(normalized);
        true
    }

    pub fn open_queue_in_viewer(&mut self, ctx: &egui::Context) -> bool {
        self.open_queue_in_viewer_at(ctx, 0)
    }

    fn open_queue_in_viewer_at(&mut self, ctx: &egui::Context, index: usize) -> bool {
        if self.queue_paths.is_empty() {
            return false;
        }
        if self
            .edit_session
            .as_ref()
            .is_some_and(|session| session.dirty)
        {
            self.edit_status = Some("Save or discard unsaved edits before opening a queue.".into());
            self.show_edit_menu = true;
            return false;
        }
        if !self.queue_mode {
            self.queue_previous_entries = Some(self.image_entries.clone());
            self.queue_previous_index = self.current_index;
        }
        self.queue_mode = true;
        let entries = self
            .queue_paths
            .iter()
            .cloned()
            .map(ImageEntry::File)
            .collect();
        self.set_image_entries(entries, index.min(self.queue_paths.len() - 1));
        self.queue_status = Some("Browsing queue order. Missing files remain in the queue and show an unavailable warning.".into());
        ctx.request_repaint();
        true
    }

    fn return_to_previous_view(&mut self, ctx: &egui::Context) {
        let Some(entries) = self.queue_previous_entries.take() else {
            self.queue_status = Some("There is no previous view to restore.".into());
            return;
        };
        self.queue_mode = false;
        let index = self
            .queue_previous_index
            .min(entries.len().saturating_sub(1));
        self.set_image_entries(entries, index);
        self.queue_status = Some("Returned to the previous browsing view.".into());
        ctx.request_repaint();
    }

    fn save_queue_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG Viewer queue", &["jvq"])
            .set_file_name("image-queue.jvq")
            .save_file()
        else {
            return;
        };
        match write_queue(&path, &self.queue_paths, self.current_directory.as_deref()) {
            Ok(()) => self.queue_status = Some(format!("Saved queue to {}", path.display())),
            Err(error) => self.queue_status = Some(format!("Could not save queue: {error}")),
        }
    }

    fn load_queue_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG Viewer queue", &["jvq", "txt"])
            .pick_file()
        else {
            return;
        };
        match read_queue(&path) {
            Ok((paths, root)) => {
                self.queue_paths = paths;
                self.queue_root = root;
                let missing = self
                    .queue_paths
                    .iter()
                    .filter(|path| !path.exists())
                    .count();
                self.queue_status = Some(format!(
                    "Loaded {} item(s); {missing} unavailable. Missing entries were preserved.",
                    self.queue_paths.len()
                ));
            }
            Err(error) => self.queue_status = Some(format!("Could not load queue: {error}")),
        }
    }

    fn export_queue_paths(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Path list", &["txt"])
            .set_file_name("image-paths.txt")
            .save_file()
        else {
            return;
        };
        let contents = self
            .queue_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        match fs::write(&path, contents) {
            Ok(()) => {
                self.queue_status = Some(format!("Exported {} path(s).", self.queue_paths.len()))
            }
            Err(error) => self.queue_status = Some(format!("Could not export paths: {error}")),
        }
    }

    fn remap_missing_queue_root(&mut self) {
        let Some(old_root) = self.queue_root.clone() else {
            self.queue_status =
                Some("Load a saved queue first; it must contain a root path to remap.".into());
            return;
        };
        let Some(new_root) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let remapped = remap_paths(&mut self.queue_paths, &old_root, &new_root);
        self.queue_root = Some(new_root);
        let still_missing = self
            .queue_paths
            .iter()
            .filter(|path| !path.exists())
            .count();
        self.queue_status = Some(format!(
            "Remapped {remapped} item(s); {still_missing} remain unavailable."
        ));
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

fn entry_path(entry: &ImageEntry) -> Option<&PathBuf> {
    if let ImageEntry::File(path) = entry {
        Some(path)
    } else {
        None
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn encode_hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(text: &str) -> Result<String, String> {
    if text.len() % 2 != 0 {
        return Err("invalid hex field length".into());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex field".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| "queue path is not valid UTF-8".into())
}

fn write_queue(path: &Path, items: &[PathBuf], root: Option<&Path>) -> Result<(), String> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let root = root
        .filter(|candidate| items.iter().all(|item| item.starts_with(candidate)))
        .map(Path::to_path_buf)
        .or_else(|| common_parent(items));
    let mut output = format!("{QUEUE_MAGIC}\n");
    if let Some(root) = &root {
        output.push_str(&format!("ROOT\t{}\n", encode_hex(&root.to_string_lossy())));
    }
    for item in items {
        if let Ok(relative) = item.strip_prefix(base) {
            output.push_str(&format!(
                "ITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ));
        } else {
            output.push_str(&format!(
                "ITEM\tA\t{}\n",
                encode_hex(&item.to_string_lossy())
            ));
        }
    }
    let temp = path.with_extension("jvq.tmp");
    let backup = path.with_extension("jvq.bak");
    fs::write(&temp, output).map_err(|e| e.to_string())?;
    let had_existing = path.exists();
    if had_existing {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(path, &backup)
            .map_err(|e| format!("cannot protect existing queue file: {e}"))?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot commit queue file: {error}"));
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn read_queue(path: &Path) -> Result<(Vec<PathBuf>, Option<PathBuf>), String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut lines = contents.lines();
    if lines.next() != Some(QUEUE_MAGIC) {
        // Plain-text path lists are accepted as an import format. Missing paths
        // are retained just like missing entries in native queue files.
        let mut imported = Vec::new();
        for line in contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let item = PathBuf::from(line);
            imported.push(if item.is_absolute() {
                item
            } else {
                absolute_path(&base.join(item))
            });
        }
        let imported_root = common_parent(&imported);
        return Ok((imported, imported_root));
    }
    let mut root = None;
    let mut items = Vec::new();
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["ROOT", encoded] => root = Some(PathBuf::from(decode_hex(encoded)?)),
            ["ITEM", kind @ ("A" | "R"), encoded] => {
                let value = PathBuf::from(decode_hex(encoded)?);
                let resolved = if *kind == "R" {
                    absolute_path(&base.join(value))
                } else {
                    value
                };
                items.push(resolved);
            }
            [""] => {}
            _ => return Err("malformed queue entry".into()),
        }
    }
    Ok((items, root))
}

fn remap_paths(items: &mut [PathBuf], old_root: &Path, new_root: &Path) -> usize {
    let mut remapped = 0;
    for path in items {
        if path.exists() {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(old_root) {
            let candidate = new_root.join(relative);
            if candidate.exists() {
                *path = candidate;
                remapped += 1;
            }
        }
    }
    remapped
}

fn common_parent(items: &[PathBuf]) -> Option<PathBuf> {
    let first = items.first()?;
    let mut candidate = first.parent()?.to_path_buf();
    for path in items.iter().skip(1) {
        while !path.starts_with(&candidate) {
            if !candidate.pop() {
                return None;
            }
        }
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("jpeg-viewer-queue-{stamp}-{name}"))
    }

    #[test]
    fn queue_roundtrip_preserves_missing_absolute_paths() {
        let dir = temp_path("roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("sample.jvq");
        let missing = dir.join("not-here.jpg");
        write_queue(&queue_file, std::slice::from_ref(&missing), Some(&dir)).unwrap();
        let (loaded, root) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![missing]);
        assert_eq!(root, Some(dir.clone()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hex_roundtrip_supports_unicode_and_delimiters() {
        let original = "фото folder/一枚 image.jpg";
        assert_eq!(decode_hex(&encode_hex(original)).unwrap(), original);
    }

    #[test]
    fn relative_paths_resolve_from_queue_file_directory() {
        let dir = temp_path("relative");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("portable.jvq");
        let relative = PathBuf::from("sub/photo.jpg");
        fs::write(
            &queue_file,
            format!(
                "{QUEUE_MAGIC}\nITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ),
        )
        .unwrap();
        let (loaded, _) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![absolute_path(&dir.join(relative))]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remapping_preserves_unresolved_items_and_repairs_moved_root() {
        let base = temp_path("remap");
        let old_root = base.join("old");
        let new_root = base.join("new");
        fs::create_dir_all(new_root.join("sub")).unwrap();
        fs::write(new_root.join("sub/photo.jpg"), b"test").unwrap();
        let mut items = vec![old_root.join("sub/photo.jpg"), old_root.join("other.jpg")];
        let remapped = remap_paths(&mut items, &old_root, &new_root);
        assert_eq!(remapped, 1);
        assert_eq!(items[0], new_root.join("sub/photo.jpg"));
        assert_eq!(items[1], old_root.join("other.jpg"));
        fs::remove_dir_all(base).unwrap();
    }
}
