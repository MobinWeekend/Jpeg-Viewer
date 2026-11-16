//! Masonry folder groups, entry cards, context menus, and pointer interaction.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    pub(super) fn render_masonry_group(
        &mut self,
        ui: &mut egui::Ui,
        tab_index: usize,
        tab_id: u64,
        entries: &[ImageEntry],
        selected: &HashSet<String>,
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
        let gap = ui.spacing().item_spacing.x.max(0.0);
        let preferred_tile_width = preferred_tile_width.clamp(76.0, 512.0);
        let columns = (((available_width + gap) / (preferred_tile_width + gap.max(0.001))).floor() as usize)
            .max(1);
        let tile_width = ((available_width - gap * columns.saturating_sub(1) as f32) / columns as f32)
            .max(1.0);
        const TILE_MARGIN: f32 = 6.0;
        let physical_image_width = (tile_width - TILE_MARGIN * 2.0).max(1.0) * ctx.pixels_per_point();
        let target_thumbnail_size = [64_u32, 128, 256, 512]
            .into_iter()
            .find(|size| *size as f32 >= physical_image_width)
            .unwrap_or(512);

        ui.columns(columns, |column_uis| {
            let mut column_heights = vec![0.0_f32; columns];
            for (index, entry) in entries.iter().enumerate() {
                let key = entry.get_id();
                let thumbnail_source_key = crate::app::queues::source_thumbnail_key(entry);
                let mut texture: Option<egui::TextureHandle> = None;
                let mut has_sufficient_resolution = false;
                // Prefer the smallest cached size that satisfies the current view.
                // If none does, keep the best lower-resolution preview visible while
                // the requested upgrade is generated asynchronously.
                for candidate_size in [64_u32, 128, 256, 512] {
                    let candidate_key = format!("thumb:{candidate_size}:{thumbnail_source_key}");
                    if let Some(candidate) = self.sidebar_thumbnails.get(&candidate_key).cloned() {
                        if candidate_size >= target_thumbnail_size {
                            texture = Some(candidate);
                            has_sufficient_resolution = true;
                            break;
                        }
                        texture = Some(candidate);
                    }
                }
                // Pick the shortest column first, then size the tile to that
                // column's actual width. `ui.columns` divides the full Masonry
                // viewport evenly, so this fills the available width without
                // leaving a trailing blank strip.
                let column = column_heights.iter().enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(column_index, _)| column_index)
                    .unwrap_or(index % columns);
                let actual_tile_width = column_uis[column].available_width()
                    .min(tile_width.max(1.0))
                    .max(1.0);
                let image_width = (actual_tile_width - TILE_MARGIN * 2.0).max(1.0);
                // A Masonry cell must remain useful even for banner-wide or
                // extremely tall scans. Clamp the *cell* ratio to 3:1..1:3,
                // then contain the original thumbnail inside it without cropping.
                let source_aspect = texture.as_ref().map(|handle| {
                    let size = handle.size_vec2();
                    if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
                        size.x / size.y
                    } else {
                        1.0
                    }
                }).unwrap_or(1.0);
                let cell_aspect = source_aspect.clamp(1.0 / 3.0, 3.0);
                let outer_height = (actual_tile_width / cell_aspect).max(64.0);
                let image_height = (outer_height - TILE_MARGIN * 2.0).max(1.0);
                let (rect, _) = column_uis[column].allocate_exact_size(
                    egui::vec2(actual_tile_width, outer_height.max(64.0)),
                    egui::Sense::hover(),
                );
                column_heights[column] += outer_height.max(64.0) + gap;
                let image_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(TILE_MARGIN, TILE_MARGIN),
                    egui::vec2(image_width, image_height),
                );

                if !has_sufficient_resolution
                    && image_rect.intersects(column_uis[column].clip_rect())
                {
                    // Always give the shared scheduler a chance to promote a
                    // matching pending Browse/Queue/Favorites request. The
                    // request method itself deduplicates completed/queued jobs.
                    self.request_masonry_thumbnail(
                        entry.clone(),
                        target_thumbnail_size,
                        ctx,
                    );
                }

                if let Some(texture) = texture {
                    let source_size = texture.size_vec2();
                    let scale = (image_rect.width() / source_size.x.max(1.0))
                        .min(image_rect.height() / source_size.y.max(1.0));
                    let drawn_size = source_size * scale;
                    let drawn_rect = egui::Rect::from_center_size(image_rect.center(), drawn_size);
                    column_uis[column].painter().image(
                        texture.id(),
                        drawn_rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    column_uis[column].painter().text(
                        image_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if self.shared_thumbnail_failed(&thumbnail_source_key, target_thumbnail_size) {
                            "Preview unavailable"
                        } else {
                            "Loading preview…"
                        },
                        egui::FontId::proportional(12.0),
                        column_uis[column].visuals().text_color(),
                    );
                }

                let response = column_uis[column].interact(
                    rect,
                    egui::Id::new(("masonry_entry", tab_id, key.clone())),
                    egui::Sense::click(),
                ).on_hover_text(format!(
                    "{}\n{}",
                    entry_display_name(entry),
                    entry_breadcrumb(entry, self.workspace.tabs[tab_index].directory.as_deref()),
                ));
                if selected.contains(&key) {
                    let visuals = column_uis[column].visuals().clone();
                    let selection_fill = visuals.selection.bg_fill;
                    let tint = egui::Color32::from_rgba_unmultiplied(
                        selection_fill.r(), selection_fill.g(), selection_fill.b(), 64,
                    );
                    let selection_color = visuals.selection.stroke.color;
                    column_uis[column].painter().rect_filled(rect.shrink(1.0), 6.0, tint);
                    column_uis[column].painter().rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        egui::Stroke::new(2.0_f32, selection_color),
                        egui::StrokeKind::Inside,
                    );
                    let check_center = image_rect.right_top() + egui::vec2(-16.0, 16.0);
                    column_uis[column].painter().circle_filled(check_center, 10.0, selection_fill);
                    column_uis[column].painter().text(
                        check_center,
                        egui::Align2::CENTER_CENTER,
                        "✓",
                        egui::FontId::proportional(14.0),
                        crate::ui_theme::contrast_foreground(selection_fill),
                    );
                } else if self.workspace.tabs[tab_index].context_highlight.as_deref() == Some(key.as_str()) {
                    // Context highlighting is deliberately separate from the
                    // selection set: right-clicking never changes multi-selection.
                    let visuals = column_uis[column].visuals();
                    let tint = egui::Color32::from_rgba_unmultiplied(
                        visuals.widgets.hovered.bg_fill.r(),
                        visuals.widgets.hovered.bg_fill.g(),
                        visuals.widgets.hovered.bg_fill.b(),
                        88,
                    );
                    column_uis[column].painter().rect_filled(rect.shrink(1.0), 6.0, tint);
                    column_uis[column].painter().rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        visuals.widgets.hovered.fg_stroke,
                        egui::StrokeKind::Inside,
                    );
                } else if response.hovered() {
                    // The active Look profile controls these widget colors, so
                    // hover feedback matches the rest of the application's UI.
                    let visuals = column_uis[column].visuals().clone();
                    let hover = visuals.widgets.hovered;
                    let tint = egui::Color32::from_rgba_unmultiplied(
                        hover.bg_fill.r(),
                        hover.bg_fill.g(),
                        hover.bg_fill.b(),
                        54,
                    );
                    column_uis[column].painter().rect_filled(rect.shrink(1.0), 6.0, tint);
                    column_uis[column].painter().rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        hover.fg_stroke,
                        egui::StrokeKind::Inside,
                    );
                }

                let right_clicked = response.secondary_clicked();
                if right_clicked {
                    self.workspace.tabs[tab_index].context_highlight = Some(key.clone());
                    ctx.request_repaint();
                }

                let entry_for_menu = entry.clone();
                let selected_for_menu = selected.clone();
                let all_entries_for_menu = self.workspace.tabs[tab_index].entries.clone();
                let pending_compare_target = self.workspace.pending_compare_target_id.is_some();
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
                    let is_selected = selected_for_menu.contains(&key);
                    if menu.button(if is_selected { "Deselect entry" } else { "Select entry" }).clicked() {
                        *action = Some(MasonryAction::ToggleSelection(key.clone()));
                        menu.close();
                    }
                    let mut compare_entries = all_entries_for_menu.iter()
                        .filter(|candidate| selected_for_menu.contains(&candidate.get_id()))
                        .cloned().collect::<Vec<_>>();
                    if !compare_entries.iter().any(|candidate| candidate.get_id() == key) {
                        compare_entries.push(entry_for_menu.clone());
                    }
                    let compare_minimum = if pending_compare_target { 1 } else { 2 };
                    let compare_label = if pending_compare_target { "Add to Compare" } else { "Compare with selection" };
                    if menu.add_enabled(compare_entries.len() >= compare_minimum, egui::Button::new(compare_label)).clicked() {
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
                        if !tab.selected.insert(key.clone()) { tab.selected.remove(&key); }
                    } else {
                        *action = Some(MasonryAction::Open(entry.clone()));
                    }
                }
            }
        });
    }
}
