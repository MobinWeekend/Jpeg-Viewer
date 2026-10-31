//! Resizable file and image-information sidebar.
use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use eframe::egui;

/// Maximum side length of each thumbnail cell.
const CELL_MAX: f32 = 240.0;
/// Minimum side length of each thumbnail cell.
const CELL_MIN: f32 = 64.0;
/// Gap between grid cells (both axes).
const GAP: f32 = 10.0;

/// Choose the number of columns and the card size.
///
/// Cards stay between CELL_MIN and CELL_MAX whenever the available width
/// permits it. The layout uses the fewest columns that can fill the width
/// without exceeding CELL_MAX.
fn fit_cards(available: f32, preferred_size: f32) -> (usize, f32) {
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

pub fn render_left_sidebar(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.settings_manager.get().show_file_sidebar {
        app.queue_drop_rect = None;
        app.sidebar_last_current_index = None;
        app.sidebar_actual_width = 0.0;
        app.sidebar_resize_dragging = false;
        return;
    }

    app.collect_sidebar_thumbnails(ctx);
    app.ensure_favorites_loaded();

    let window_width = ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .map(|rect| rect.width())
            .unwrap_or(1280.0)
    });
    // The 50% value is a ceiling, not the target width. Keep the user's
    // saved width whenever it fits, with a modest minimum for usable controls.
    let max_sidebar_width = (window_width * 0.5).max(1.0);
    let min_sidebar_width = 90.0_f32.min(max_sidebar_width);
    let default_width = app
        .settings_manager
        .get()
        .file_sidebar_width
        .clamp(min_sidebar_width, max_sidebar_width);

    let response = egui::SidePanel::left("file_sidebar")
        .resizable(true)
        .default_width(default_width)
        .width_range(min_sidebar_width..=max_sidebar_width)
        .show(ctx, |ui| {
            // Lock this frame's content UI to the width allocated by SidePanel.
            // Individual tabs contain different controls (some with long labels);
            // those controls must wrap/clip within this width rather than
            // establishing a new minimum width for the sidebar when tabs change.
            let sidebar_content_width = ui.available_width().max(0.0);
            ui.set_width(sidebar_content_width);
            ui.set_min_width(0.0);
            ui.set_max_width(sidebar_content_width);

            ui.horizontal(|ui| {
                // Drawer toggle belongs to the sidebar edge, not the navigation
                // toolbar. Use a panel-and-arrow glyph to show the action clearly.
                let hide_sidebar = ui.add(
                    egui::Button::new("")
                        .frame(false)
                        .min_size(egui::vec2(30.0, 28.0)),
                );
                crate::app::ui::helpers::paint_sidebar_drawer_icon(
                    ui,
                    hide_sidebar.rect,
                    false,
                );
                if hide_sidebar.on_hover_text("Hide sidebar").clicked() {
                    let _ = app.settings_manager.update(|settings| {
                        settings.show_file_sidebar = false;
                    });
                }
                ui.heading("JPEG Viewer");
            });

            ui.separator();

            // The tab strip always occupies exactly the sidebar's inner width.
            // Keep the row and its buttons from imposing a wider minimum size on
            // the SidePanel; labels are shortened before the buttons can overflow.
            let tab_available_width = ui.available_width().max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(tab_available_width, 30.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(0.0);
                    ui.set_max_width(tab_available_width);
                    ui.spacing_mut().item_spacing.x = 1.0;
                    ui.spacing_mut().button_padding.x = 1.0;

                    let tab_width = ((tab_available_width - 3.0).max(0.0) / 4.0).max(0.0);
                    let (browse_label, queue_label, favorites_label, info_label) = if tab_width >= 118.0 {
                        ("▦ Browse", "↕ Queue", "★ Favorites", "ⓘ Info")
                    } else if tab_width >= 72.0 {
                        ("Browse", "Queue", "Favorites", "Info")
                    } else if tab_width >= 42.0 {
                        ("Brw", "Que", "Fav", "Info")
                    } else {
                        ("▦", "↕", "★", "ⓘ")
                    };
                    let browse = !app.sidebar_queue_tab && !app.sidebar_favorites_tab && !app.sidebar_info_tab;
                    if ui.add_sized([tab_width, 30.0], egui::Button::selectable(browse, browse_label))
                        .on_hover_text("Browse images").clicked() {
                        app.sidebar_queue_tab = false; app.sidebar_favorites_tab = false; app.sidebar_info_tab = false; app.queue_drop_rect = None;
                    }
                    if ui.add_sized([tab_width, 30.0], egui::Button::selectable(app.sidebar_queue_tab, queue_label))
                        .on_hover_text(format!("Queue ({})", app.queue_paths.len())).clicked() {
                        app.sidebar_queue_tab = true; app.sidebar_favorites_tab = false; app.sidebar_info_tab = false;
                    }
                    if ui.add_sized([tab_width, 30.0], egui::Button::selectable(app.sidebar_favorites_tab, favorites_label))
                        .on_hover_text(format!("Favorites ({})", app.favorite_paths.len())).clicked() {
                        app.sidebar_queue_tab = false; app.sidebar_favorites_tab = true; app.sidebar_info_tab = false;
                    }
                    if ui.add_sized([tab_width, 30.0], egui::Button::selectable(app.sidebar_info_tab, info_label))
                        .on_hover_text("Image information").clicked() {
                        app.sidebar_queue_tab = false; app.sidebar_favorites_tab = false; app.sidebar_info_tab = true; app.queue_drop_rect = None;
                    }
                },
            );

            ui.separator();

            // Keep every tab's content area tied to the panel width. This is
            // intentionally independent of the active tab's widget contents.
            let tab_content_width = ui.available_width().max(0.0);
            ui.set_min_width(0.0);
            ui.set_max_width(tab_content_width);

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

                ui.horizontal(|ui| {
                    ui.label(format!("{} images  |  ", app.image_entries.len()));

                    //sorting
                    crate::app::ui::settings_menu::general::sort_method_horizontal_ui(app, ctx, ui);
                });
                let mut entry_size = app.settings_manager.get().browse_entry_size;
                ui.horizontal(|ui| {
                    ui.label("Entry size");
                    if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut entry_size, 64.0..=240.0).show_value(false).step_by(8.0))).changed() {
                        let _ = app.settings_manager.update(|settings| settings.browse_entry_size = entry_size);
                    }
                    ui.label(if entry_size < 104.0 { "Compact" } else if entry_size < 176.0 { "Medium" } else { "Large" });
                });

                let entries = app.image_entries.clone();
                let available = ui.available_width();
                let (columns, card_size) = fit_cards(available, app.settings_manager.get().browse_entry_size);

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
                                    // Grid keeps every row in the scroll area's content size, but
                                    // don't build thumbnail widgets for rows outside the viewport.
                                    // This is a lightweight form of virtualization: it preserves
                                    // scrolling geometry while avoiding per-card work off-screen.
                                    let compact_entry = app.settings_manager.get().browse_entry_size < 112.0;
                                    let estimated_height = if compact_entry {
                                        card_size / 8.0
                                    } else {
                                        card_size
                                    };
                                    let estimated_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        egui::vec2(card_size, estimated_height),
                                    );

                                    // Scroll to the selected entry even when it is
                                    // currently virtualized outside the viewport. The
                                    // previous visibility guard skipped the scroll
                                    // request for off-screen entries, which made
                                    // navigation toward earlier entries unreliable.
                                    if index == app.current_index
                                        && auto_scroll_current
                                        && !ui.clip_rect().contains_rect(estimated_rect)
                                    {
                                        ui.scroll_to_rect(estimated_rect, Some(egui::Align::Min));
                                    }

                                    let clicked = if !ui.is_rect_visible(estimated_rect) {
                                        ui.allocate_space(egui::vec2(card_size, estimated_height));
                                        false
                                    } else {
                                        let name = entry_name(entry);
                                        match entry {
                                        ImageEntry::File(path) => {
                                            let absolute_path = super::storage::absolute_path(path);
                                            let key = format!("file:{}", absolute_path.display());

                                            // Reuse the same thumbnail card renderer as the queue UI
                                            // so image sizing, selection outlines, and filename fitting
                                            // stay consistent between Browse and Queue.
                                            let mut clicked = false;
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(
                                                    card_size,
                                                    if entry_size < 112.0 { card_size / 8.0 } else { card_size },
                                                ),
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
                                                        entry_size,
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
                                                            egui::FontId::proportional(crate::ui_theme::scaled_text_size(ui.ctx(), 11.0)),
                                                            ui.visuals().weak_text_color(),
                                                        );
                                                        if ui.is_rect_visible(rect) {
                                                            app.request_sidebar_archive_thumbnail(
                                                                entry.clone(),
                                                                key.clone(),
                                                                ctx,
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

                                                    let label_selected = index == app.current_index;
                                                    let label_position = ui.cursor().min;
                                                    let label_rect = egui::Rect::from_min_size(
                                                        label_position,
                                                        egui::vec2(card_size, label_h),
                                                    );
                                                    let pointer_over_label = ctx.input(|input| {
                                                        input.pointer.hover_pos().is_some_and(|pos| label_rect.contains(pos))
                                                    });
                                                    let label_text_color = if crate::ui_theme::current_look_profile(ctx) == "win98" {
                                                        let visuals = ui.visuals();
                                                        let fill = if label_selected {
                                                            visuals.selection.bg_fill
                                                        } else if pointer_over_label {
                                                            visuals.widgets.hovered.bg_fill
                                                        } else {
                                                            visuals.widgets.inactive.bg_fill
                                                        };
                                                        crate::ui_theme::contrast_foreground(fill)
                                                    } else {
                                                        ui.visuals().text_color()
                                                    };
                                                    let label = egui::RichText::new(middle_ellipsis(
                                                        &name,
                                                        (card_size / 6.0) as usize,
                                                    ))
                                                    .color(label_text_color);
                                                    let text = ui.add_sized(
                                                        [card_size, label_h],
                                                        egui::Button::selectable(label_selected, label),
                                                    );

                                                    clicked = response.clicked()
                                                        || text.on_hover_text(&name).clicked();
                                                },
                                            );

                                            clicked
                                        }
                                    }
                                    };

                                    if clicked && index != app.current_index {
                                        // The shared navigation method owns unsaved-edit
                                        // validation and all per-image state transitions.
                                        selected = Some(index);
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
                    // Use the same navigation pipeline as keyboard/toolbar navigation.
                    // This keeps the window title, current path, file-type detection,
                    // edit/view state, navigation timer, loading, and metadata in sync.
                    app.navigate_to_index(ctx, index);
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
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Enter the new position (starting at 1):");
                let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(100.0));
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Move").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if commit {
            if let Ok(position) = text.trim().parse::<usize>() {
                let target = position.saturating_sub(1);
                match collection {
                    0 if index < app.image_entries.len() && target < app.image_entries.len() => {
                        let selected_path = app.current_image_path.clone();
                        let item = app.image_entries.remove(index);
                        app.image_entries
                            .insert(target.min(app.image_entries.len()), item);
                        if let Some(path) = selected_path {
                            if let Some(new_index) = app
                                .image_entries
                                .iter()
                                .position(|e| matches!(e, ImageEntry::File(p) if p == &path))
                            {
                                app.current_index = new_index;
                            }
                        }
                        app.image_cache.clear();
                        app.preloading_indices.clear();
                        app.preload_tasks.clear();
                        app.preload_generation = app.preload_generation.wrapping_add(1);
                        if !app.image_entries.is_empty() {
                            app.load_current_image_with_cache();
                        }
                    }
                    1 if index < app.queue_paths.len() && target < app.queue_paths.len() => {
                        let item = app.queue_paths.remove(index);
                        app.queue_paths
                            .insert(target.min(app.queue_paths.len()), item);
                    }
                    2 if index < app.favorite_paths.len() && target < app.favorite_paths.len() => {
                        let item = app.favorite_paths.remove(index);
                        app.favorite_paths
                            .insert(target.min(app.favorite_paths.len()), item);
                        app.save_favorites_for_ui();
                    }
                    _ => {
                        app.queue_status = Some("Position is outside the list.".into());
                    }
                }
            } else {
                app.queue_status = Some("Enter a valid whole number.".into());
            }
            app.sidebar_number_edit = None;
        } else if cancel {
            app.sidebar_number_edit = None;
        } else {
            app.sidebar_number_edit = Some((collection, index, text));
        }
    }

    if let Some((path, value)) = app.rename_dialog.clone() {
        let mut name = value;
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Rename file")
            .id(egui::Id::new("rename_file_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("New filename (extension is preserved):");
                let response = ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0));
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if commit {
            let trimmed = name.trim();
            if !trimmed.is_empty()
                && !trimmed.contains('/')
                && !trimmed.contains('\\')
                && trimmed != "."
                && trimmed != ".."
            {
                let extension = path.extension().map(|e| e.to_os_string());
                let mut new_path = path.with_file_name(trimmed);
                if let Some(ext) = extension {
                    if new_path.extension().is_none() {
                        new_path.set_extension(ext);
                    } else {
                        new_path.set_extension(ext);
                    }
                }
                if new_path != path {
                    match std::fs::rename(&path, &new_path) {
                        Ok(()) => {
                            for entry in &mut app.image_entries {
                                if let ImageEntry::File(p) = entry {
                                    if *p == path {
                                        *p = new_path.clone();
                                    }
                                }
                            }
                            for p in &mut app.queue_paths {
                                if *p == path {
                                    *p = new_path.clone();
                                }
                            }
                            for p in &mut app.favorite_paths {
                                if *p == path {
                                    *p = new_path.clone();
                                }
                            }
                            app.save_favorites_for_ui();
                            app.current_image_path = Some(new_path.clone());
                            app.update_current_image_path();
                            app.sidebar_thumbnails.clear();
                            app.sidebar_thumbnail_attempted.clear();
                            app.sidebar_thumbnail_failed.clear();
                            app.image_cache.clear();
                            app.preloading_indices.clear();
                            app.preload_tasks.clear();
                            app.preload_generation = app.preload_generation.wrapping_add(1);
                            app.queue_status = Some(format!(
                                "Renamed file to {}.",
                                new_path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        Err(error) => app.queue_status = Some(format!("Rename failed: {error}")),
                    }
                }
            } else {
                app.queue_status = Some("Enter a valid filename without path separators.".into());
            }
            app.rename_dialog = None;
        } else if cancel {
            app.rename_dialog = None;
        } else {
            app.rename_dialog = Some((path, name));
        }
    }

    // Keep the pointer feedback active while a card is being dragged.
    if app.sidebar_drag_index.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    // Cancel an unfinished drag if the pointer is released outside a valid card.
    if ctx.input(|input| input.pointer.any_released()) && app.sidebar_drag_index.is_some() {
        app.sidebar_drag_index = None;
    }

    let panel_rect = response.response.rect;
    let width = panel_rect
        .width()
        .clamp(min_sidebar_width, max_sidebar_width);
    app.sidebar_actual_width = width;

    // Track resizing only when the initial pointer press lands on the panel's
    // right-edge resize grip. A generic panel drag can also be triggered by
    // dragging cards or selecting content, which caused width changes to be
    // accidentally persisted during ordinary sidebar interaction.
    let (primary_pressed, primary_down, any_released, press_origin) = ctx.input(|input| {
        (
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.any_released(),
            input.pointer.press_origin(),
        )
    });

    if primary_pressed {
        app.sidebar_resize_dragging = press_origin.is_some_and(|origin| {
            (origin.x - panel_rect.right()).abs() <= 14.0
                && origin.y >= panel_rect.top()
                && origin.y <= panel_rect.bottom()
        });
    }

    if app.sidebar_resize_dragging && primary_down {
        let previous = app.settings_manager.get().file_sidebar_width;
        if (width - previous).abs() > 1.0 {
            app.settings_manager.settings.file_sidebar_width = width;
            app.sidebar_width_needs_save = true;
        }
    }

    if app.sidebar_resize_dragging && any_released {
        if app.sidebar_width_needs_save {
            let _ = app.settings_manager.save();
        }
        app.sidebar_width_needs_save = false;
        app.sidebar_resize_dragging = false;
    } else if !primary_down
        && !ctx.input(|input| input.pointer.any_down())
        && app.sidebar_width_needs_save
    {
        // Recovery path if the release happened outside the application window.
        let _ = app.settings_manager.save();
        app.sidebar_width_needs_save = false;
        app.sidebar_resize_dragging = false;
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

pub fn middle_ellipsis(text: &str, max_chars: usize) -> String {
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
