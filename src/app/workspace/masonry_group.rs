//! Masonry folder groups, entry cards, context menus, and pointer interaction.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    pub(super) fn render_masonry_group(
        &mut self,
        ui: &mut egui::Ui,
        tab_index: usize,
        tab_id: u64,
        entries: &[&ImageEntry],
        selected: &HashSet<String>,
        selected_entries: &[&ImageEntry],
        preferred_tile_width: f32,
        ctx: &egui::Context,
        action: &mut Option<MasonryAction>,
    ) {
        // A right-click highlight is only a visual aid for the open context menu.
        // Clear it as soon as egui reports that the popup has closed; persistent
        // multi-selection remains untouched.
        let context_menu_open = egui::Popup::is_any_open(ctx);
        if !context_menu_open && !ctx.input(|input| input.pointer.secondary_down()) {
            self.workspace.tabs[tab_index].context_highlight = None;
        }

        let available_width = ui.available_width().max(1.0);
        ui.set_min_width(available_width);
        ui.set_max_width(available_width);
        let horizontal_gap = ui.spacing().item_spacing.x.max(0.0);
        let vertical_gap = ui.spacing().item_spacing.y.max(0.0);
        let preferred_tile_width = preferred_tile_width.clamp(76.0, 512.0);
        let columns = (((available_width + horizontal_gap)
            / (preferred_tile_width + horizontal_gap.max(0.001)))
            .floor() as usize)
            .max(1);
        let tile_width = ((available_width - horizontal_gap * columns.saturating_sub(1) as f32)
            / columns as f32)
            .max(1.0);
        const TILE_MARGIN: f32 = 6.0;

        // The caller prepares the selected-entry slice once for the entire
        // collection, so selections spanning different folder groups remain
        // available without cloning the complete source list for every card.
        let pending_compare_target = self.workspace.pending_compare_target_id.is_some();

        // The tile geometry is calculated separately from widget creation. This
        // lets us reserve the full scroll height while creating interactive egui
        // widgets only for tiles that intersect the current viewport.
        // Compute all card rectangles in one shortest-column pass. The previous
        // implementation repeated the same O(N × columns) assignment twice per
        // frame: once to find the scroll height and again to paint the cards.
        // Store local rectangles, reserve the final content area, then translate
        // only the visible cards into screen coordinates.
        let mut column_heights = vec![0.0_f32; columns];
        let mut local_tile_rects = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let entry = *entry;
            let key = entry.get_id();
            let aspect = self.workspace.tabs[tab_index]
                .masonry_aspect_ratios
                .get(&key)
                .copied()
                .unwrap_or(1.0)
                .clamp(1.0 / 3.0, 3.0);
            let column = column_heights
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(column_index, _)| column_index)
                .unwrap_or(index % columns);
            let outer_height = (tile_width / aspect).max(64.0);
            local_tile_rects.push(egui::Rect::from_min_size(
                egui::pos2(
                    column as f32 * (tile_width + horizontal_gap),
                    column_heights[column],
                ),
                egui::vec2(tile_width, outer_height),
            ));
            column_heights[column] += outer_height + vertical_gap;
        }
        let content_height = (column_heights
            .into_iter()
            .fold(0.0_f32, f32::max) - vertical_gap)
            .max(64.0);
        let (_, grid_rect) = ui.allocate_space(egui::vec2(available_width, content_height));
        let clip_rect = ui.clip_rect();
        let painter = ui.painter().clone();

        // All off-screen items still contribute to the layout and scroll height,
        // but only viewport-intersecting cards get thumbnails, widgets, tooltips,
        // painter operations, or context-menu responses.
        for (entry, local_rect) in entries.iter().zip(local_tile_rects.into_iter()) {
            let entry = *entry;
            let tile_rect = local_rect.translate(grid_rect.min.to_vec2());

            if !tile_rect.intersects(clip_rect) {
                continue;
            }

            let key = entry.get_id();
            let thumbnail_source_key = crate::app::queues::source_thumbnail_key(entry);
            let image_rect = tile_rect.shrink(TILE_MARGIN);
            // Request exactly the largest physical dimension needed by this
            // entry's image viewport. This is not restricted to fixed 64/128/
            // 256/512px buckets; native providers decide which sizes they can
            // actually produce.
            let target_thumbnail_size = crate::app::queues::thumbnail_request_size(
                image_rect,
                ctx,
            );

            let has_sufficient_resolution = self.has_shared_thumbnail_at_least(
                &thumbnail_source_key,
                target_thumbnail_size,
            );
            // Prefer the smallest cached preview that satisfies this entry's
            // current display size. If none does, show the best smaller preview
            // while the requested upgrade is generated asynchronously.
            let texture = self.get_shared_thumbnail(&thumbnail_source_key, target_thumbnail_size);

            if !has_sufficient_resolution {
                // The shared scheduler deduplicates jobs and promotes a matching
                // pending Browse/Queue/Favorites request to Masonry priority.
                self.request_masonry_thumbnail(
                    (*entry).clone(),
                    target_thumbnail_size,
                    ctx,
                );
            }

            if let Some(texture) = texture {
                let preview_size = texture.size_vec2();
                // The source image remains authoritative for Masonry's layout.
                // The shared thumbnail pipeline also normalizes preview pixels
                // to this ratio so the same cached texture looks correct in
                // Masonry, sidebar cards, and New Tab previews. Keeping the
                // layout ratio separately makes it stable across texture eviction.
                let source_aspect_ratio = self.workspace.tabs[tab_index]
                    .masonry_aspect_ratios
                    .get(&key)
                    .copied()
                    .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
                    .unwrap_or_else(|| {
                        if preview_size.y > 0.0 {
                            preview_size.x / preview_size.y
                        } else {
                            1.0
                        }
                    });
                let drawn_rect = crate::app::queues::thumbnail_fit_rect(image_rect, source_aspect_ratio);
                painter.image(
                    texture.id(),
                    drawn_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                painter.text(
                    image_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if self.shared_thumbnail_failed(&thumbnail_source_key, target_thumbnail_size) {
                        "Preview unavailable"
                    } else {
                        "Loading preview…"
                    },
                    egui::FontId::proportional(12.0),
                    ui.visuals().text_color(),
                );
            }

            let response = ui
                .interact(
                    tile_rect,
                    egui::Id::new(("masonry_entry", tab_id, key.clone())),
                    egui::Sense::click(),
                )
                .on_hover_text(format!(
                    "{}\n{}",
                    entry_display_name(entry),
                    entry_breadcrumb(entry, self.workspace.tabs[tab_index].directory.as_deref()),
                ));

            if selected.contains(&key) {
                let visuals = ui.visuals().clone();
                let selection_fill = visuals.selection.bg_fill;
                let tint = egui::Color32::from_rgba_unmultiplied(
                    selection_fill.r(), selection_fill.g(), selection_fill.b(), 64,
                );
                painter.rect_filled(tile_rect.shrink(1.0), 6.0, tint);
                painter.rect_stroke(
                    tile_rect.shrink(1.0),
                    6.0,
                    egui::Stroke::new(2.0_f32, visuals.selection.stroke.color),
                    egui::StrokeKind::Inside,
                );
                let check_center = image_rect.right_top() + egui::vec2(-16.0, 16.0);
                painter.circle_filled(check_center, 10.0, selection_fill);
                painter.text(
                    check_center,
                    egui::Align2::CENTER_CENTER,
                    "✓",
                    egui::FontId::proportional(14.0),
                    crate::ui_theme::contrast_foreground(selection_fill),
                );
            } else if self.workspace.tabs[tab_index]
                .context_highlight
                .as_deref()
                == Some(key.as_str())
            {
                // Context highlighting is deliberately separate from the
                // selection set: right-clicking never changes multi-selection.
                let visuals = ui.visuals();
                let tint = egui::Color32::from_rgba_unmultiplied(
                    visuals.widgets.hovered.bg_fill.r(),
                    visuals.widgets.hovered.bg_fill.g(),
                    visuals.widgets.hovered.bg_fill.b(),
                    88,
                );
                painter.rect_filled(tile_rect.shrink(1.0), 6.0, tint);
                painter.rect_stroke(
                    tile_rect.shrink(1.0),
                    6.0,
                    visuals.widgets.hovered.fg_stroke,
                    egui::StrokeKind::Inside,
                );
            } else if response.hovered() {
                let visuals = ui.visuals().clone();
                let hover = visuals.widgets.hovered;
                let tint = egui::Color32::from_rgba_unmultiplied(
                    hover.bg_fill.r(), hover.bg_fill.g(), hover.bg_fill.b(), 54,
                );
                painter.rect_filled(tile_rect.shrink(1.0), 6.0, tint);
                painter.rect_stroke(tile_rect.shrink(1.0), 6.0, hover.fg_stroke, egui::StrokeKind::Inside);
            }

            if response.secondary_clicked() {
                self.workspace.tabs[tab_index].context_highlight = Some(key.clone());
                ctx.request_repaint();
            }

            // The context menu owns only this entry and the selected subset. The
            // collection-wide Vec is never cloned from inside each card.
            let entry_for_menu = (*entry).clone();
            let selected_entries_for_menu = selected_entries;
            let is_selected = selected.contains(&key);
            response.clone().context_menu(|menu| {
                if menu.button("Open in Viewer").clicked() {
                    *action = Some(MasonryAction::Open(entry_for_menu.clone()));
                    menu.close();
                }
                if menu.button("Open in New Tab").clicked() {
                    *action = Some(MasonryAction::OpenNewTab(entry_for_menu.clone()));
                    menu.close();
                }
                menu.separator();
                if menu.button(if is_selected { "Deselect entry" } else { "Select entry" }).clicked() {
                    *action = Some(MasonryAction::ToggleSelection(key.clone()));
                    menu.close();
                }
                let compare_minimum = if pending_compare_target { 1 } else { 2 };
                let compare_label = if pending_compare_target { "Add to Compare" } else { "Compare with selection" };
                let compare_count = selected_entries_for_menu.len() + usize::from(!is_selected);
                if menu.add_enabled(compare_count >= compare_minimum, egui::Button::new(compare_label)).clicked() {
                    // Materialize the selected collection only when the command is
                    // actually invoked, not on every frame that a popup is open.
                    let mut compare_entries = selected_entries_for_menu
                        .iter()
                        .map(|selected_entry| (**selected_entry).clone())
                        .collect::<Vec<_>>();
                    if !is_selected {
                        compare_entries.push(entry_for_menu.clone());
                    }
                    *action = Some(MasonryAction::CompareEntries(compare_entries));
                    menu.close();
                }
                menu.separator();
                if menu.button("Copy / Extract…").clicked() {
                    *action = Some(MasonryAction::CopyEntries(vec![entry_for_menu.clone()]));
                    menu.close();
                }
                match &entry_for_menu {
                    ImageEntry::File(path) => {
                        if menu.button("Move…").clicked() {
                            *action = Some(MasonryAction::MoveEntries(vec![entry_for_menu.clone()]));
                            menu.close();
                        }
                        if menu.button("Delete…").clicked() {
                            *action = Some(MasonryAction::DeleteEntries(vec![entry_for_menu.clone()]));
                            menu.close();
                        }
                        menu.separator();
                        if menu.button("Open With…").clicked() {
                            *action = Some(MasonryAction::OpenWith(path.clone()));
                            menu.close();
                        }
                        if menu.button("Locate in File Manager").clicked() {
                            *action = Some(MasonryAction::Reveal(path.clone()));
                            menu.close();
                        }
                        if menu.button("Rename… (F2)").clicked() {
                            *action = Some(MasonryAction::Rename(path.clone()));
                            menu.close();
                        }
                        if menu.button("Add to Queue").clicked() {
                            *action = Some(MasonryAction::AddToQueue(path.clone()));
                            menu.close();
                        }
                        if menu.button("Add to Favorites").clicked() {
                            *action = Some(MasonryAction::AddToFavorites(path.clone()));
                            menu.close();
                        }
                        if menu.button("Copy File Path").clicked() {
                            *action = Some(MasonryAction::CopyText(path.display().to_string()));
                            menu.close();
                        }
                    }
                    ImageEntry::Zip(item) => {
                        if menu.button("Open Containing Archive").clicked() {
                            *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
                            *action = Some(MasonryAction::CopyText(item.name.clone()));
                            menu.close();
                        }
                    }
                    ImageEntry::S7z(item) => {
                        if menu.button("Open Containing Archive").clicked() {
                            *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
                            *action = Some(MasonryAction::CopyText(item.name.clone()));
                            menu.close();
                        }
                    }
                    ImageEntry::Rar(item) => {
                        if menu.button("Open Containing Archive").clicked() {
                            *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                            menu.close();
                        }
                        if menu.button("Copy Entry Name").clicked() {
                            *action = Some(MasonryAction::CopyText(item.name.clone()));
                            menu.close();
                        }
                    }
                }
            });

            if response.clicked() {
                self.workspace.tabs[tab_index].context_highlight = None;
                if self.workspace.tabs[tab_index].selection_mode {
                    let tab = &mut self.workspace.tabs[tab_index];
                    if !tab.selected.insert(key.clone()) {
                        tab.selected.remove(&key);
                    }
                } else {
                    *action = Some(MasonryAction::Open((*entry).clone()));
                }
            }
        }
    }
}

