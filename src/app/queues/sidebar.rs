//! Resizable file and image-information sidebar.
use super::ui::{GAP, fit_cards};
use crate::app::types::ViewerApp;
use crate::imaging::entry::ImageEntry;
use eframe::egui;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct BrowseSidebarItem {
    entry: ImageEntry,
    /// Index in the persistent primary Viewer collection. Tab-only entries do
    /// not participate in Browse sorting or reordering.
    browse_index: Option<usize>,
    image_tab: Option<(u64, bool)>,
}

fn browse_sidebar_items(app: &ViewerApp) -> (Vec<BrowseSidebarItem>, usize) {
    let viewer_tab = app
        .workspace
        .tabs
        .iter()
        .find(|tab| tab.kind == crate::app::workspace::WorkspaceTabKind::Viewer && tab.id == 1)
        .or_else(|| {
            app.workspace
                .tabs
                .iter()
                .find(|tab| tab.kind == crate::app::workspace::WorkspaceTabKind::Viewer)
        });
    let (browse_entries, browse_current_index) = if app.workspace.active_kind()
        == crate::app::workspace::WorkspaceTabKind::Viewer
    {
        (app.image_entries.clone(), app.current_index)
    } else if let Some(tab) = viewer_tab {
        (tab.entries.clone(), tab.current_index)
    } else {
        (app.image_entries.clone(), app.current_index)
    };

    let mut tab_for_entry = HashMap::<String, (u64, bool)>::new();
    let mut tab_only_entries = Vec::new();
    for tab in app
        .workspace
        .tabs
        .iter()
        .filter(|tab| tab.kind == crate::app::workspace::WorkspaceTabKind::Image)
    {
        let Some(entry) = tab.entries.first() else {
            continue;
        };
        let entry_id = entry.get_id();
        let details = (tab.id, app.popouts.has_tab_popout(tab.id));
        tab_for_entry.entry(entry_id).or_insert(details);
        tab_only_entries.push((entry.clone(), details));
    }

    let mut seen = HashSet::with_capacity(browse_entries.len() + tab_only_entries.len());
    let mut items = Vec::with_capacity(browse_entries.len() + tab_only_entries.len());
    for (index, entry) in browse_entries.iter().cloned().enumerate() {
        let id = entry.get_id();
        seen.insert(id.clone());
        items.push(BrowseSidebarItem {
            entry,
            browse_index: Some(index),
            image_tab: tab_for_entry.get(&id).copied(),
        });
    }
    // Image tabs opened from another folder/archive remain discoverable in the
    // same Browse grid, but never duplicate an image already present there.
    for (entry, details) in tab_only_entries {
        if seen.insert(entry.get_id()) {
            items.push(BrowseSidebarItem {
                entry,
                browse_index: None,
                image_tab: Some(details),
            });
        }
    }

    let active_entry_id = match app.workspace.active_tab() {
        Some(tab) if tab.kind == crate::app::workspace::WorkspaceTabKind::Image => {
            tab.entries.first().map(ImageEntry::get_id)
        }
        Some(tab) if tab.kind == crate::app::workspace::WorkspaceTabKind::Viewer => {
            browse_entries.get(browse_current_index).map(ImageEntry::get_id)
        }
        _ => browse_entries.get(browse_current_index).map(ImageEntry::get_id),
    };
    let current_grid_index = active_entry_id
        .as_deref()
        .and_then(|id| items.iter().position(|item| item.entry.get_id() == id))
        .unwrap_or(usize::MAX);

    (items, current_grid_index)
}

fn render_browse_sort_control(app: &mut ViewerApp, ui: &mut egui::Ui) {
    let old_method = app.sort_method;
    ui.horizontal(|ui| {
        ui.label("Sort:");
        egui::ComboBox::from_id_salt("sort_method_combobox")
            .selected_text(format!("{:?}", app.sort_method))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.sort_method, crate::imaging::sort::SortMethod::Natural, "Natural");
                ui.selectable_value(&mut app.sort_method, crate::imaging::sort::SortMethod::DateNewest, "Newest");
                ui.selectable_value(&mut app.sort_method, crate::imaging::sort::SortMethod::DateOldest, "Oldest");
                ui.selectable_value(&mut app.sort_method, crate::imaging::sort::SortMethod::Shuffle, "Shuffle");
            });
    });
    if old_method != app.sort_method {
        app.sort_primary_viewer_entries();
    }
}

fn paint_image_tab_controls(
    ui: &mut egui::Ui,
    card_rect: egui::Rect,
    tab_id: u64,
    has_popout: bool,
) -> bool {
    let size = card_rect.height().min(20.0).max(14.0);
    let top = card_rect.top() + ((card_rect.height() - size) * 0.5).max(1.0);
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(card_rect.right() - size - 5.0, top),
        egui::vec2(size, size),
    );
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(close_rect.left() - size - 3.0, top),
        egui::vec2(size, size),
    );
    let badge_response = ui.interact(
        badge_rect,
        egui::Id::new(("browse_image_tab_badge", tab_id)),
        egui::Sense::hover(),
    );
    badge_response.on_hover_text(if has_popout {
        "Open Image tab · Pop-out active"
    } else {
        "Open Image tab"
    });
    ui.painter().rect_filled(
        badge_rect,
        egui::CornerRadius::same(5),
        egui::Color32::from_rgba_unmultiplied(30, 105, 150, 225),
    );
    ui.painter().text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        "▣",
        egui::FontId::proportional((size * 0.64).clamp(9.0, 13.0)),
        egui::Color32::WHITE,
    );
    if has_popout {
        let dot = egui::Rect::from_center_size(
            badge_rect.right_bottom() - egui::vec2(2.0, 2.0),
            egui::vec2(7.0, 7.0),
        );
        ui.painter().circle_filled(dot.center(), 3.5, egui::Color32::from_rgb(75, 215, 145));
    }

    let close_response = ui
        .interact(
            close_rect,
            egui::Id::new(("browse_image_tab_close", tab_id)),
            egui::Sense::click(),
        )
        .on_hover_text("Close Image tab");
    ui.painter().circle_filled(
        close_rect.center(),
        size * 0.5,
        if close_response.hovered() {
            egui::Color32::from_rgb(185, 55, 60)
        } else {
            egui::Color32::from_black_alpha(205)
        },
    );
    ui.painter().text(
        close_rect.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional((size * 0.78).clamp(10.0, 15.0)),
        egui::Color32::WHITE,
    );
    close_response.clicked()
}

pub fn render_left_sidebar(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    if !app.settings_manager.get().show_file_sidebar {
        app.sidebar_visible_sources.clear();
        app.sidebar_prefetch_sources.clear();
        app.sidebar_last_scroll_offset = None;
        app.finish_sidebar_thumbnail_frame();
        app.collect_sidebar_thumbnails(ctx);
        app.queue_drop_rect = None;
        app.sidebar_last_current_index = None;
        app.sidebar_actual_width = 0.0;
        app.sidebar_resize_dragging = false;
        return;
    }

    app.collect_sidebar_thumbnails(ctx);
    app.sidebar_visible_sources.clear();
    app.sidebar_prefetch_sources.clear();
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

    let sidebar_settings = app.settings_manager.get().clone();
    let sidebar_background = if crate::ui_theme::current_look_profile(ctx) == "win98" {
        // Use the active Win98 palette for both Light and Dark themes; other looks
        // retain the existing image-view surface behavior.
        ctx.global_style().visuals.panel_fill
    } else {
        crate::ui_theme::main_view_surface_color(
            ctx,
            &sidebar_settings.main_view_background,
            sidebar_settings.main_view_background_opacity,
        )
    };
    let sidebar_frame = egui::Frame::side_top_panel(&ctx.global_style())
        .fill(sidebar_background)
        .stroke(egui::Stroke::NONE);

    let response = egui::Panel::left("file_sidebar")
        .resizable(true)
        .default_size(default_width)
        .size_range(min_sidebar_width..=max_sidebar_width)
        .frame(sidebar_frame)
        .show(ui, |ui| {
            // Extend the same window-local blurred photo backdrop behind the sidebar.
            // The panel painter is clipped to this sidebar, while the backdrop target
            // remains the full client area so its crop aligns with the title bar/canvas.
            if sidebar_settings.tricks_backdrop_enabled {
                let sidebar_clip = ui.painter().clip_rect();
                let backdrop_rect = crate::app::ui::tricks_backdrop_window_rect(ctx, sidebar_clip);
                app.paint_tricks_backdrop(
                    ui.painter(),
                    sidebar_clip,
                    backdrop_rect,
                    crate::ui_theme::main_accent_color(ctx),
                    sidebar_settings.tricks_backdrop_tint,
                    sidebar_settings.tricks_backdrop_opacity,
                );
            }
            // Lock this frame's content UI to the width allocated by SidePanel.
            // Individual tabs contain different controls (some with long labels);
            // those controls must wrap/clip within this width rather than
            // establishing a new minimum width for the sidebar when tabs change.
            let sidebar_content_width = ui.available_width().max(0.0);
            ui.set_width(sidebar_content_width);
            ui.set_min_width(0.0);
            ui.set_max_width(sidebar_content_width);

            ui.horizontal_wrapped(|ui| {
                // With the custom title bar enabled, the sidebar drawer control
                // already lives in the title bar. Keep this in-sidebar duplicate
                // only when Windows/native decorations own the title bar.
                if sidebar_settings.show_titlebar {
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
            ui.set_width(tab_content_width);
            ui.set_min_width(0.0);
            ui.set_max_width(tab_content_width);

            if app.sidebar_queue_tab {
                app.sidebar_last_current_index = None;
                app.sidebar_last_scroll_offset = None;
                app.render_queue_sidebar(ui, ctx);
            } else if app.sidebar_favorites_tab {
                app.sidebar_last_current_index = None;
                app.sidebar_last_scroll_offset = None;
                app.render_favorites_sidebar(ui, ctx);
            } else if app.sidebar_info_tab {
                app.sidebar_last_scroll_offset = None;
                app.queue_drop_rect = None;
                ui.heading("Image information");
                ui.separator();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
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
                // Browse always renders the primary Viewer collection, even while
                // a dedicated Image tab is active. An open Image tab decorates the
                // matching Browse tile; entries outside the collection are appended
                // to this same grid rather than placed in a separate tab section.
                let (browse_items, _) = browse_sidebar_items(app);
                let base_count = browse_items
                    .iter()
                    .filter(|item| item.browse_index.is_some())
                    .count();
                let extra_tabs = browse_items
                    .iter()
                    .filter(|item| item.browse_index.is_none())
                    .count();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} images", browse_items.len()));
                    if extra_tabs > 0 {
                        ui.label(egui::RichText::new(format!("+ {extra_tabs} outside Browse")).weak());
                    }
                    render_browse_sort_control(app, ui);
                    if crate::app::ui::masonry_button(ui, "Open in Masonry", base_count > 0, 0.0, false, true).clicked() {
                        app.open_browse_masonry();
                    }
                });
                let mut entry_size = app.settings_manager.get().browse_entry_size;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Entry size");
                    if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut entry_size, 64.0..=240.0).show_value(false).step_by(8.0))).changed() {
                        let _ = app.settings_manager.update(|settings| settings.browse_entry_size = entry_size);
                    }
                    ui.label(if entry_size < 104.0 { "Compact" } else if entry_size < 176.0 { "Medium" } else { "Large" });
                });

                // Sorting can change tile order in the header above, so refresh
                // the presentation list after the sort control has run.
                let (browse_items, current_grid_index) = browse_sidebar_items(app);
                let active_workspace_tab_id = app.workspace.active_tab().map(|tab| tab.id);
                let entries = browse_items.iter().map(|item| item.entry.clone()).collect::<Vec<_>>();
                let available = ui.available_width();
                let (columns, card_size) = fit_cards(available, app.settings_manager.get().browse_entry_size);

                let auto_scroll_current =
                    app.sidebar_last_current_index != Some(current_grid_index);

                let mut selected = None;
                let mut close_tab_requested = None;
                let mut visible_indices = Vec::new();

                let scroll_output = egui::ScrollArea::both()
                    .id_salt("file_sidebar_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Overflow stays in this scrollable child; its viewport is
                        // fixed to the width allocated by the parent SidePanel.
                        ui.set_min_width(0.0);
                        ui.set_max_width(available.max(1.0));
                        egui::Grid::new("file_sidebar_thumbnail_grid")
                            .num_columns(columns)
                            .spacing(egui::vec2(GAP, GAP))
                            .show(ui, |ui| {
                                for (index, item) in browse_items.iter().enumerate() {
                                    let entry = &item.entry;
                                    let tab_meta = item.image_tab;
                                    let browse_index = item.browse_index;
                                    let is_current = index == current_grid_index;
                                    // Grid keeps every row in the scroll area's content size, but
                                    // don't build thumbnail widgets for rows outside the viewport.
                                    // This is a lightweight form of virtualization: it preserves
                                    // scrolling geometry while avoiding per-card work off-screen.
                                    let compact_entry = app.settings_manager.get().browse_entry_size < 112.0;
                                    // Match render_thumbnail_card's actual compact
                                    // row height so Grid virtualization, hit testing,
                                    // and the tab controls share the same geometry.
                                    let estimated_height = if compact_entry {
                                        (card_size / 7.0).max(24.0)
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
                                    if is_current
                                        && auto_scroll_current
                                        && !ui.clip_rect().contains_rect(estimated_rect)
                                    {
                                        ui.scroll_to_rect(estimated_rect, Some(egui::Align::Min));
                                    }

                                    let clicked = if !ui.is_rect_visible(estimated_rect) {
                                        ui.allocate_space(egui::vec2(card_size, estimated_height));
                                        false
                                    } else {
                                        visible_indices.push(index);
                                        let name = entry_name(entry);
                                        match entry {
                                        ImageEntry::File(path) => {
                                            let absolute_path = super::storage::absolute_path(path);
                                            let key = format!("file:{}", absolute_path.display());

                                            // Reuse the same thumbnail card renderer as the queue UI
                                            // so image sizing, selection outlines, and filename fitting
                                            // stay consistent between Browse and Queue.
                                            let mut clicked = false;
                                            let card_output = ui.allocate_ui_with_layout(
                                                egui::vec2(
                                                    card_size,
                                                    if entry_size < 112.0 { (card_size / 7.0).max(24.0) } else { card_size },
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
                                                        is_current,
                                                        auto_scroll_current && is_current,
                                                        card_size,
                                                        entry_size,
                                                        browse_index,
                                                        0,
                                                        tab_meta.map(|(tab_id, _)| tab_id),
                                                        ctx,
                                                    );
                                                },
                                            );
                                            let close_clicked = if let Some((tab_id, has_popout)) = tab_meta {
                                                paint_image_tab_controls(ui, card_output.response.rect, tab_id, has_popout)
                                            } else {
                                                false
                                            };
                                            if close_clicked {
                                                close_tab_requested = tab_meta.map(|(tab_id, _)| tab_id);
                                            }
                                            clicked && !close_clicked
                                        }
                                        _ => {
                                            let key = entry.get_id();
                                            let label_h =
                                                (card_size * 0.25).clamp(20.0, 32.0);
                                            let thumb_h = (card_size - label_h).max(1.0);

                                            let mut clicked = false;

                                            // Allocate the whole card as a top-down vertical
                                            // block: thumbnail on top, label directly below it.
                                            let card_output = ui.allocate_ui_with_layout(
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
                                                            if let Some((tab_id, _)) = tab_meta {
                                                                app.activate_workspace_tab(tab_id);
                                                            } else if let Some(browse_index) = browse_index {
                                                                app.activate_primary_viewer_tab();
                                                                app.navigate_to_index(ctx, browse_index);
                                                            }
                                                            ui.close();
                                                        }
                                                        if ui.button("Open in New Tab").clicked() {
                                                            app.open_entry_in_image_tab(entry.clone(), true);
                                                            ui.close();
                                                        }
                                                        if ui.button("Share…").clicked() {
                                                            let entry_for_share = entry.clone();
                                                            app.share_entry(&entry_for_share);
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

                                                    let target_thumbnail_size = crate::app::queues::thumbnail_request_size(rect, ctx);
                                                    if ui.is_rect_visible(rect) {
                                                        app.sidebar_visible_sources.insert(key.clone());
                                                        let current_item = is_current;
                                                        let priority = if current_item {
                                                            crate::app::queues::SIDEBAR_CURRENT_PRIORITY
                                                        } else if response.hovered() {
                                                            crate::app::queues::SIDEBAR_HOVER_PRIORITY
                                                        } else {
                                                            crate::app::queues::SIDEBAR_VISIBLE_PRIORITY
                                                        };
                                                        app.request_visible_sidebar_archive_thumbnail(
                                                            entry.clone(),
                                                            key.clone(),
                                                            target_thumbnail_size,
                                                            priority,
                                                            ctx,
                                                        );
                                                    }
                                                    if let Some(texture) = app.get_shared_thumbnail(&key, target_thumbnail_size) {
                                                        let size = texture.size_vec2();
                                                        if size.x > 0.0 && size.y > 0.0 {
                                                            let draw = crate::app::queues::thumbnail_fit_rect(
                                                                rect,
                                                                size.x / size.y,
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
                                                            if app.shared_thumbnail_failed(&key, target_thumbnail_size)
                                                            {
                                                                "No preview"
                                                            } else {
                                                                "Loading…"
                                                            },
                                                            egui::FontId::proportional(crate::ui_theme::scaled_text_size(ui.ctx(), 11.0)),
                                                            ui.visuals().weak_text_color(),
                                                        );
                                                    }

                                                    if is_current
                                                        && auto_scroll_current
                                                        && !ui.clip_rect().contains_rect(rect)
                                                    {
                                                        ui.scroll_to_rect(
                                                            rect,
                                                            Some(egui::Align::Min),
                                                        );
                                                    }

                                                    let label_selected = is_current;
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
                                            let close_clicked = if let Some((tab_id, has_popout)) = tab_meta {
                                                paint_image_tab_controls(ui, card_output.response.rect, tab_id, has_popout)
                                            } else {
                                                false
                                            };
                                            if close_clicked {
                                                close_tab_requested = tab_meta.map(|(tab_id, _)| tab_id);
                                            }

                                            clicked && !close_clicked
                                        }
                                    }
                                    };

                                    if clicked && close_tab_requested.is_none() {
                                        let should_activate = browse_items.get(index).is_some_and(|item| {
                                            match item.image_tab {
                                                Some((tab_id, _)) => active_workspace_tab_id != Some(tab_id),
                                                None => !is_current,
                                            }
                                        });
                                        if should_activate {
                                            // The shared navigation method owns unsaved-edit
                                            // validation and per-image transitions. A tile
                                            // linked to an Image tab activates that tab even
                                            // when the primary Viewer happens to show the same
                                            // source image.
                                            selected = Some(index);
                                        }
                                    }

                                    if (index + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }

                                if !entries.len().is_multiple_of(columns) {
                                    ui.end_row();
                                }
                            });
                    });
                let scroll_offset = scroll_output.state.unclamped_offset().y;
                let scroll_delta_y = app.sidebar_last_scroll_offset
                    .map(|previous| scroll_offset - previous)
                    .unwrap_or(0.0);
                app.sidebar_last_scroll_offset = Some(scroll_offset);

                // Prefetch one row in the direction of travel. When the user is
                // stationary, keep a single row on either side warm so keyboard
                // navigation and small scroll movements do not expose blank cards.
                if let (Some(first_visible), Some(last_visible)) = (
                    visible_indices.iter().copied().min(),
                    visible_indices.iter().copied().max(),
                ) {
                    let first_row = first_visible / columns.max(1);
                    let last_row = last_visible / columns.max(1);
                    let mut prefetch_rows = Vec::with_capacity(2);
                    if scroll_delta_y > 0.5 {
                        prefetch_rows.push(last_row + 1);
                    } else if scroll_delta_y < -0.5 {
                        prefetch_rows.push(first_row.saturating_sub(1));
                    } else {
                        prefetch_rows.push(first_row.saturating_sub(1));
                        prefetch_rows.push(last_row + 1);
                    }
                    prefetch_rows.sort_unstable();
                    prefetch_rows.dedup();

                    let compact_prefetch = entry_size < 112.0;
                    let prefetch_side = if compact_prefetch {
                        (card_size / 7.0 - 4.0).clamp(12.0, 58.0)
                    } else {
                        (card_size - 6.0).max(1.0)
                    };
                    let prefetch_rect = egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(prefetch_side, prefetch_side),
                    );
                    let prefetch_size = crate::app::queues::thumbnail_request_size(prefetch_rect, ctx)
                        .min(crate::app::queues::SIDEBAR_PLACEHOLDER_SIZE);

                    for row in prefetch_rows {
                        if row >= first_row && row <= last_row {
                            continue;
                        }
                        let row_start = row.saturating_mul(columns.max(1));
                        let row_end = (row_start + columns).min(entries.len());
                        for index in row_start..row_end {
                            let entry = entries[index].clone();
                            if matches!(&entry, ImageEntry::File(path) if path.is_dir()) {
                                continue;
                            }
                            app.prefetch_sidebar_entry_thumbnail(entry, prefetch_size, ctx);
                        }
                    }
                }

                if let Some(tab_id) = close_tab_requested {
                    app.close_workspace_tab(tab_id);
                    app.sidebar_last_current_index = None;
                    selected = None;
                }
                if let Some(index) = selected
                    && let Some(item) = browse_items.get(index)
                {
                    if let Some((tab_id, _)) = item.image_tab {
                        app.activate_workspace_tab(tab_id);
                    } else if let Some(browse_index) = item.browse_index {
                        // A normal Browse tile activates the primary collection
                        // before navigating; the active Image tab is never used
                        // as the index source for the Browse grid.
                        app.activate_primary_viewer_tab();
                        app.navigate_to_index(ctx, browse_index);
                    }
                }

                app.sidebar_last_current_index = Some(current_grid_index);

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
                    0 => {
                        let viewer_len = app.workspace.tabs.iter()
                            .find(|tab| tab.kind == crate::app::workspace::WorkspaceTabKind::Viewer && tab.id == 1)
                            .or_else(|| app.workspace.tabs.iter().find(|tab| tab.kind == crate::app::workspace::WorkspaceTabKind::Viewer))
                            .map(|tab| tab.entries.len())
                            .unwrap_or(app.image_entries.len());
                        if index < viewer_len && target < viewer_len {
                            // `reorder_primary_viewer_entries` takes a drop slot,
                            // while this dialog specifies the final 0-based index.
                            let drop_slot = if index < target { target + 1 } else { target };
                            app.reorder_primary_viewer_entries(index, drop_slot);
                        } else {
                            app.queue_status = Some("Position is outside the list.".into());
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

    app.finish_sidebar_thumbnail_frame();
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

/// Render the existing rename dialog independently of the file sidebar so
/// title-bar and keyboard rename actions work even when the sidebar is hidden.
pub fn render_rename_dialog(app: &mut ViewerApp, ctx: &egui::Context) {
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
                                if let ImageEntry::File(p) = entry
                                    && *p == path
                                {
                                    *p = new_path.clone();
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
                            app.cancel_all_preload_tasks();
                            app.preload_generation = app.preload_generation.wrapping_add(1);
                            app.preloading_indices.clear();
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
