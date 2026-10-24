//! Resizable file and image-information sidebar.
use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use eframe::egui;

/// Maximum side length of each thumbnail cell.
const CELL_MAX: f32 = 128.0;
/// Minimum side length of each thumbnail cell.
const CELL_MIN: f32 = 64.0;
/// Gap between grid cells (both axes).
const GAP: f32 = 10.0;

/// Choose the number of columns and the card size.
///
/// Cards stay between CELL_MIN and CELL_MAX whenever the available width
/// permits it. The layout uses the fewest columns that can fill the width
/// without exceeding CELL_MAX.
fn fit_cards(available: f32) -> (usize, f32) {
    let available = available.max(CELL_MIN);

    let columns = (((available + GAP) / (CELL_MAX + GAP)).ceil() as usize).clamp(1, 24);

    let card_size =
        ((available - GAP * (columns as f32 - 1.0)) / columns as f32).clamp(CELL_MIN, CELL_MAX);

    (columns, card_size)
}

pub fn render_left_sidebar(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.settings_manager.get().show_file_sidebar {
        app.queue_drop_rect = None;
        app.sidebar_last_current_index = None;
        return;
    }

    app.collect_sidebar_thumbnails(ctx);
    app.ensure_favorites_loaded();

    let default_width = app
        .settings_manager
        .get()
        .file_sidebar_width
        .clamp(260.0, 2048.0);

    let response = egui::SidePanel::left("file_sidebar")
        .resizable(true)
        .default_width(default_width)
        .width_range(260.0..=2048.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("JPEG Viewer");

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text("Hide file sidebar")
                            .clicked()
                        {
                            let _ = app.settings_manager.update(|settings| {
                                settings.show_file_sidebar = false;
                            });
                        }
                    },
                );
            });

            ui.separator();

            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(!app.sidebar_queue_tab && !app.sidebar_favorites_tab && !app.sidebar_info_tab, "Browse").clicked() {
                    app.sidebar_queue_tab = false; app.sidebar_favorites_tab = false; app.sidebar_info_tab = false; app.queue_drop_rect = None;
                }
                if ui.selectable_label(app.sidebar_queue_tab, format!("Queue ({})", app.queue_paths.len())).clicked() {
                    app.sidebar_queue_tab = true; app.sidebar_favorites_tab = false; app.sidebar_info_tab = false;
                }
                if ui.selectable_label(app.sidebar_favorites_tab, format!("Favorites ({})", app.favorite_paths.len())).clicked() {
                    app.sidebar_queue_tab = false; app.sidebar_favorites_tab = true; app.sidebar_info_tab = false;
                }
                if ui.selectable_label(app.sidebar_info_tab, "Info").clicked() {
                    app.sidebar_queue_tab = false; app.sidebar_favorites_tab = false; app.sidebar_info_tab = true; app.queue_drop_rect = None;
                }
            });

            ui.separator();

            if app.sidebar_queue_tab {
                app.sidebar_last_current_index = None;
                app.render_queue_sidebar(ui, ctx);
            } else if app.sidebar_favorites_tab {
                app.sidebar_last_current_index = None;
                app.render_favorites_sidebar(ui, ctx);
            } else if app.sidebar_info_tab {
                app.queue_drop_rect = None;
                ui.heading("Image information");
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(metadata) = app.file_metadata.clone() {
                        app.render_file_metadata(ui, &metadata);
                    } else {
                        ui.label("Metadata is not available yet.");
                        if let Some(path) = app.current_image_path.clone() {
                            ui.label(path.display().to_string());
                        }
                    }
                });
            } else {
                app.queue_drop_rect = None;

                ui.label(format!("{} images", app.image_entries.len()));

                let entries = app.image_entries.clone();
                let available = ui.available_width();
                let (columns, card_size) = fit_cards(available);

                let auto_scroll_current =
                    app.sidebar_last_current_index != Some(app.current_index);

                let mut selected = None;

                egui::ScrollArea::vertical()
                    .id_salt("file_sidebar_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("file_sidebar_thumbnail_grid")
                            .num_columns(columns)
                            .spacing(egui::vec2(GAP, GAP))
                            .show(ui, |ui| {
                                for (index, entry) in entries.iter().enumerate() {
                                    let name = entry_name(entry);

                                    let clicked = match entry {
                                        ImageEntry::File(path) => {
                                            let absolute_path = if path.is_absolute() {
                                                path.clone()
                                            } else {
                                                std::env::current_dir()
                                                    .unwrap_or_default()
                                                    .join(path)
                                            };
                                            let key = format!("file:{}", absolute_path.display());

                                            // Reuse the same thumbnail card renderer as the queue UI
                                            // so image sizing, selection outlines, and filename fitting
                                            // stay consistent between Browse and Queue.
                                            let mut clicked = false;
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(card_size, card_size),
                                                egui::Layout::centered_and_justified(
                                                    egui::Direction::TopDown,
                                                ),
                                                |ui| {
                                                    clicked = app.render_thumbnail_card(
                                                        ui,
                                                        path,
                                                        &key,
                                                        &name,
                                                        index == app.current_index,
                                                        auto_scroll_current
                                                            && index == app.current_index,
                                                        card_size,
                                                        Some(index),
                                                        0,
                                                        ctx,
                                                    );
                                                },
                                            );
                                            clicked
                                        }
                                        _ => {
                                            let key = entry.get_id();
                                            let label_h =
                                                (card_size * 0.25).clamp(20.0, 32.0);
                                            let thumb_h = (card_size - label_h).max(1.0);

                                            let mut clicked = false;

                                            // Allocate the whole card as a top-down vertical
                                            // block: thumbnail on top, label directly below it.
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(card_size, card_size),
                                                egui::Layout::top_down(egui::Align::Center),
                                                |ui| {
                                                    let (rect, response) =
                                                        ui.allocate_exact_size(
                                                            egui::vec2(card_size, thumb_h),
                                                            egui::Sense::click(),
                                                        );
                                                    response.context_menu(|ui| {
                                                        if ui.button("Open image").clicked() {
                                                            app.navigate_to_index(ctx, index);
                                                            ui.close();
                                                        }
                                                        if ui.button("Copy entry name").clicked() {
                                                            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(name.clone())) {
                                                                Ok(()) => app.queue_status = Some("Copied archive entry name.".into()),
                                                                Err(error) => app.queue_status = Some(format!("Could not copy name: {error}")),
                                                            }
                                                            ui.close();
                                                        }
                                                        if ui.button("Open containing archive").clicked() {
                                                            let archive_path = match entry {
                                                                ImageEntry::Zip(item) => &item.archive_path,
                                                                ImageEntry::S7z(item) => &item.archive_path,
                                                                ImageEntry::Rar(item) => &item.archive_path,
                                                                ImageEntry::File(path) => path,
                                                            };
                                                            if let Err(error) = open::that(archive_path) {
                                                                app.queue_status = Some(format!("Could not open archive: {error}"));
                                                            }
                                                            ui.close();
                                                        }
                                                    });

                                                    if let Some(texture) =
                                                        app.sidebar_thumbnails.get(&key)
                                                    {
                                                        let size = texture.size_vec2();
                                                        if size.x > 0.0 && size.y > 0.0 {
                                                            let scale = (rect.width() / size.x)
                                                                .min(rect.height() / size.y);
                                                            let draw =
                                                                egui::Rect::from_center_size(
                                                                    rect.center(),
                                                                    size * scale,
                                                                );
                                                            ui.painter().image(
                                                                texture.id(),
                                                                draw,
                                                                egui::Rect::from_min_max(
                                                                    egui::pos2(0.0, 0.0),
                                                                    egui::pos2(1.0, 1.0),
                                                                ),
                                                                egui::Color32::WHITE,
                                                            );
                                                        }
                                                    } else {
                                                        ui.painter().rect_filled(
                                                            rect,
                                                            4.0,
                                                            ui.visuals()
                                                                .widgets
                                                                .noninteractive
                                                                .bg_fill,
                                                        );
                                                        ui.painter().text(
                                                            rect.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            if app
                                                                .sidebar_thumbnail_failed
                                                                .contains(&key)
                                                            {
                                                                "No preview"
                                                            } else {
                                                                "Loading…"
                                                            },
                                                            egui::FontId::proportional(11.0),
                                                            ui.visuals().weak_text_color(),
                                                        );
                                                        if ui.is_rect_visible(rect) {
                                                            app.request_sidebar_archive_thumbnail(
                                                                entry.clone(),
                                                                key.clone(),
                                                            );
                                                        }
                                                    }

                                                    if index == app.current_index
                                                        && auto_scroll_current
                                                        && !ui.clip_rect().contains_rect(rect)
                                                    {
                                                        ui.scroll_to_rect(
                                                            rect,
                                                            Some(egui::Align::Min),
                                                        );
                                                    }

                                                    let text = ui.add_sized(
                                                        [card_size, label_h],
                                                        egui::Button::selectable(
                                                            index == app.current_index,
                                                            middle_ellipsis(
                                                                &name,
                                                                (card_size / 6.0) as usize,
                                                            ),
                                                        ),
                                                    );

                                                    clicked = response.clicked()
                                                        || text.on_hover_text(&name).clicked();
                                                },
                                            );

                                            clicked
                                        }
                                    };

                                    if clicked && index != app.current_index {
                                        if app
                                            .edit_session
                                            .as_ref()
                                            .is_some_and(|session| session.dirty)
                                        {
                                            app.edit_status = Some(
                                                "Save or discard unsaved edits before changing images."
                                                    .into(),
                                            );
                                            app.show_edit_menu = true;
                                            app.show_settings_menu = false;
                                        } else {
                                            app.edit_session = None;
                                            app.show_edit_menu = false;
                                            app.crop_tool_active = false;
                                            app.crop_drag_start_image = None;
                                            app.crop_selection = None;
                                            selected = Some(index);
                                        }
                                    }

                                    if (index + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }

                                if entries.len() % columns != 0 {
                                    ui.end_row();
                                }
                            });
                    });

                if let Some(index) = selected {
                    app.current_index = index;
                    app.load_current_image_with_cache();
                }

                app.sidebar_last_current_index = Some(app.current_index);

            }
        });

    // Quick position editor: entering a 1-based position inserts the item there,
    // shifting intervening entries rather than swapping them.
    if let Some((collection, index, value)) = app.sidebar_number_edit.clone() {
        let mut text = value;
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Move entry to position")
            .id(egui::Id::new("sidebar_number_editor"))
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Enter the new position (starting at 1):");
                let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(100.0));
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { commit = true; }
                ui.horizontal(|ui| {
                    if ui.button("Move").clicked() { commit = true; }
                    if ui.button("Cancel").clicked() { cancel = true; }
                });
            });
        if commit {
            if let Ok(position) = text.trim().parse::<usize>() {
                let target = position.saturating_sub(1);
                match collection {
                    0 if index < app.image_entries.len() && target < app.image_entries.len() => {
                        let selected_path = app.current_image_path.clone();
                        let item = app.image_entries.remove(index);
                        app.image_entries.insert(target.min(app.image_entries.len()), item);
                        if let Some(path) = selected_path {
                            if let Some(new_index) = app.image_entries.iter().position(|e| matches!(e, ImageEntry::File(p) if p == &path)) { app.current_index = new_index; }
                        }
                        app.image_cache.clear(); app.preloading_indices.clear(); app.preload_tasks.clear();
                        app.preload_generation = app.preload_generation.wrapping_add(1);
                        if !app.image_entries.is_empty() { app.load_current_image_with_cache(); }
                    }
                    1 if index < app.queue_paths.len() && target < app.queue_paths.len() => {
                        let item = app.queue_paths.remove(index); app.queue_paths.insert(target.min(app.queue_paths.len()), item);
                    }
                    2 if index < app.favorite_paths.len() && target < app.favorite_paths.len() => {
                        let item = app.favorite_paths.remove(index); app.favorite_paths.insert(target.min(app.favorite_paths.len()), item); app.save_favorites_for_ui();
                    }
                    _ => { app.queue_status = Some("Position is outside the list.".into()); }
                }
            } else { app.queue_status = Some("Enter a valid whole number.".into()); }
            app.sidebar_number_edit = None;
        } else if cancel { app.sidebar_number_edit = None; }
        else { app.sidebar_number_edit = Some((collection, index, text)); }
    }

    if let Some((path, value)) = app.rename_dialog.clone() {
        let mut name = value;
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Rename file")
            .id(egui::Id::new("rename_file_dialog"))
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("New filename (extension is preserved):");
                let response = ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0));
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { commit = true; }
                ui.horizontal(|ui| { if ui.button("Rename").clicked() { commit = true; } if ui.button("Cancel").clicked() { cancel = true; } });
            });
        if commit {
            let trimmed = name.trim();
            if !trimmed.is_empty() && !trimmed.contains('/') && !trimmed.contains('\\') && trimmed != "." && trimmed != ".." {
                let extension = path.extension().map(|e| e.to_os_string());
                let mut new_path = path.with_file_name(trimmed);
                if let Some(ext) = extension {
                    if new_path.extension().is_none() { new_path.set_extension(ext); }
                    else { new_path.set_extension(ext); }
                }
                if new_path != path {
                    match std::fs::rename(&path, &new_path) {
                        Ok(()) => {
                            for entry in &mut app.image_entries { if let ImageEntry::File(p) = entry { if *p == path { *p = new_path.clone(); } } }
                            for p in &mut app.queue_paths { if *p == path { *p = new_path.clone(); } }
                            for p in &mut app.favorite_paths { if *p == path { *p = new_path.clone(); } }
                            app.save_favorites_for_ui();
                            app.current_image_path = Some(new_path.clone());
                            app.update_current_image_path();
                            app.sidebar_thumbnails.clear(); app.sidebar_thumbnail_attempted.clear(); app.sidebar_thumbnail_failed.clear();
                            app.image_cache.clear(); app.preloading_indices.clear(); app.preload_tasks.clear();
                            app.preload_generation = app.preload_generation.wrapping_add(1);
                            app.queue_status = Some(format!("Renamed file to {}.", new_path.file_name().unwrap_or_default().to_string_lossy()));
                        }
                        Err(error) => app.queue_status = Some(format!("Rename failed: {error}")),
                    }
                }
            } else { app.queue_status = Some("Enter a valid filename without path separators.".into()); }
            app.rename_dialog = None;
        } else if cancel { app.rename_dialog = None; }
        else { app.rename_dialog = Some((path, name)); }
    }

    // Keep the pointer feedback active while a card is being dragged.
    if app.sidebar_drag_index.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    // Cancel an unfinished drag if the pointer is released outside a valid card.
    if ctx.input(|input| input.pointer.any_released()) && app.sidebar_drag_index.is_some() {
        app.sidebar_drag_index = None;
    }

    let width = response.response.rect.width().clamp(260.0, 2048.0);
    let previous = app.settings_manager.get().file_sidebar_width;

    if !ctx.input(|input| input.pointer.any_down()) && (width - previous).abs() > 2.0 {
        let _ = app
            .settings_manager
            .update(|settings| settings.file_sidebar_width = width);
    }
}

fn entry_name(entry: &ImageEntry) -> String {
    match entry {
        ImageEntry::File(path) => path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        ImageEntry::Zip(entry) => entry.name.clone(),
        ImageEntry::S7z(entry) => entry.name.clone(),
        ImageEntry::Rar(entry) => entry.name.clone(),
    }
}

pub(super) fn middle_ellipsis(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();

    if chars.len() <= max_chars || max_chars < 5 {
        return if chars.len() <= max_chars {
            text.to_string()
        } else {
            chars.into_iter().take(max_chars).collect()
        };
    }

    let keep = max_chars - 3;
    let left = (keep * 3 + 2) / 5;
    let right = keep - left;

    format!(
        "{}...{}",
        chars[..left].iter().collect::<String>(),
        chars[chars.len() - right..].iter().collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::middle_ellipsis;

    #[test]
    fn long_names_are_middle_truncated() {
        assert_eq!(
            middle_ellipsis("somephotoIhavethatIshotLongAgoNamed012.jpeg", 27),
            "somephotoIhave...ed012.jpeg"
        );
    }

    #[test]
    fn short_names_are_unchanged() {
        assert_eq!(middle_ellipsis("cat.png", 20), "cat.png");
    }
}
