//! Masonry workspace surface, filtering, sorting, selection, and grid layout.

use super::helpers::*;
use super::*;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

impl crate::app::types::ViewerApp {
    /// Returns true when a non-viewer workspace rendered the central surface.
    pub(crate) fn render_active_workspace(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> bool {
        if self.workspace.active_kind() != WorkspaceTabKind::Masonry {
            self.masonry_visible_sources.clear();
            crate::app::queues::update_masonry_thumbnail_visibility(&HashSet::new());
        }
        match self.workspace.active_kind() {
            WorkspaceTabKind::Viewer => {
                if self.is_home_surface_active() && !self.workspace.session_prompt {
                    self.render_welcome_ui(ctx, ui);
                    true
                } else {
                    false
                }
            }
            WorkspaceTabKind::Image => false,
            WorkspaceTabKind::Masonry => {
                self.render_masonry_workspace(ui, ctx);
                true
            }
            WorkspaceTabKind::Compare => {
                self.render_compare_workspace(ui, ctx);
                true
            }
            WorkspaceTabKind::Welcome => {
                if !self.workspace.session_prompt {
                    self.render_welcome_ui(ctx, ui);
                }
                true
            }
        }
    }

    pub(super) fn render_masonry_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Dimension metadata must be applied before Masonry computes its
        // shortest-column layout. Defer completed thumbnail uploads until after
        // this frame has recalculated which cards are actually visible.
        self.collect_masonry_dimensions();
        let Some(tab_index) = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| {
                self.workspace
                    .tabs
                    .iter()
                    .position(|candidate| candidate.id == tab.id)
                    .unwrap_or(self.workspace.active_index)
            })
        else {
            return;
        };

        let mut action: Option<MasonryAction> = None;
        let mut reload_folder = false;
        let mut selected_sort_method = self.sort_method;
        let previous_tile_size = self.workspace.tabs[tab_index]
            .masonry_tile_size
            .clamp(76.0, 512.0);
        let mut tile_size = previous_tile_size;
        let mut persist_tile_size = false;
        let tab_id = self.workspace.tabs[tab_index].id;
        let selection_mode = self.workspace.tabs[tab_index].selection_mode;
        let has_pending_compare_target = self.workspace.pending_compare_target_id.is_some();

        // Masonry is a bottom-docked workspace: the image grid owns the flexible
        // area and scrolls independently, while the themed two/three-row toolbar
        // stays anchored to the bottom edge of the central view.
        let layout_rect = ui
            .available_rect_before_wrap()
            .intersect(ui.clip_rect())
            .intersect(self.main_view_rect(ctx));
        if layout_rect.width() < 80.0 || layout_rect.height() < 40.0 {
            self.masonry_visible_sources.clear();
            crate::app::queues::update_masonry_thumbnail_visibility(&HashSet::new());
            return;
        }
        let footer_height = if selection_mode { 112.0 } else { 82.0 };
        let footer_bottom = layout_rect.bottom() - 8.0;
        let footer_top = (footer_bottom - footer_height).max(layout_rect.top() + 24.0);
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, footer_top),
            egui::pos2(
                (layout_rect.right() - 8.0).max(layout_rect.left() + 8.0),
                footer_bottom,
            ),
        );
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, layout_rect.top() + 8.0),
            egui::pos2(
                (layout_rect.right() - 8.0).max(layout_rect.left() + 8.0),
                (footer_top - 8.0).max(layout_rect.top() + 8.0),
            ),
        );

        // Temporarily take ownership of the Masonry entry vector while building
        // the scroll area. Cloning the complete collection here used to duplicate
        // every PathBuf/archive name on every repaint for large folders.
        let masonry_entries = std::mem::take(&mut self.workspace.tabs[tab_index].entries);

        // Only entries intersecting the current Masonry viewport should keep
        // exclusive Masonry thumbnail work alive.
        let mut visible_masonry_sources = HashSet::new();

        // The content scroll area is constrained to the actual remaining CentralPanel
        // rect, which already accounts for both docked sidebars. The scrollbar is
        // widened and kept inside that rect; no outer-window subtraction is needed.
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |content_ui| {
                {
                    let scroll_style = &mut content_ui.spacing_mut().scroll;
                    scroll_style.floating = false;
                    scroll_style.bar_width = 14.0;
                    scroll_style.handle_min_length = 32.0;
                    scroll_style.bar_inner_margin = 4.0;
                    scroll_style.bar_outer_margin = 1.0;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("masonry_scroll", tab_id))
                    .auto_shrink([false, false])
                    .show(content_ui, |scroll_ui| {
                        let viewport_width = scroll_ui.available_width().max(1.0);
                        scroll_ui.set_min_width(viewport_width);
                        scroll_ui.set_max_width(viewport_width);
                        scroll_ui.add_space(12.0);
                        scroll_ui.horizontal(|outer| {
                            outer.spacing_mut().item_spacing.x = 0.0;
                            outer.add_space(12.0);
                            outer.vertical(|grid_ui| {
                                let content_width = (viewport_width - 24.0).max(1.0);
                                grid_ui.set_min_width(content_width);
                                grid_ui.set_max_width(content_width);
                                let entries = masonry_entries.as_slice();
                                let (
                                    selected,
                                    search,
                                    extension_filter,
                                    folder_masonry,
                                    recursive,
                                    directory,
                                    preferred_size,
                                    groups_revision,
                                ) = {
                                    let tab = &self.workspace.tabs[tab_index];
                                    (
                                        tab.selected.clone(),
                                        tab.search.to_lowercase(),
                                        tab.extension_filter.to_lowercase(),
                                        tab.folder_masonry,
                                        tab.recursive,
                                        tab.directory.clone(),
                                        tab.masonry_tile_size,
                                        tab.masonry_groups_revision,
                                    )
                                };
                                let selected_entries = if selected.is_empty() {
                                    Vec::new()
                                } else {
                                    entries
                                        .iter()
                                        .filter(|entry| selected.contains(&entry.get_id()))
                                        .collect::<Vec<_>>()
                                };
                                let headers_enabled =
                                    self.settings_manager.get().masonry_show_folder_headers;
                                let groups_fingerprint = {
                                    let mut hasher =
                                        std::collections::hash_map::DefaultHasher::new();
                                    groups_revision.hash(&mut hasher);
                                    search.hash(&mut hasher);
                                    extension_filter.hash(&mut hasher);
                                    folder_masonry.hash(&mut hasher);
                                    recursive.hash(&mut hasher);
                                    directory.hash(&mut hasher);
                                    headers_enabled.hash(&mut hasher);
                                    entries.len().hash(&mut hasher);
                                    hasher.finish()
                                };

                                // Filtering and grouping used to walk every entry and
                                // allocate a new set of folder vectors on every scroll
                                // repaint. Cache source indices instead; the entries
                                // themselves stay in their existing collection vector.
                                let cached_groups = self.workspace.tabs[tab_index]
                                    .masonry_groups_cache
                                    .as_ref()
                                    .filter(|cache| cache.fingerprint == groups_fingerprint)
                                    .map(|cache| Arc::clone(&cache.groups));
                                let groups = if let Some(groups) = cached_groups {
                                    groups
                                } else {
                                    let filtered_indices = if search.is_empty()
                                        && extension_filter.is_empty()
                                    {
                                        (0..entries.len()).collect::<Vec<_>>()
                                    } else {
                                        entries
                                            .iter()
                                            .enumerate()
                                            .filter_map(|(index, entry)| {
                                                let name = entry_display_name(entry).to_lowercase();
                                                let ext = Path::new(&name)
                                                    .extension()
                                                    .and_then(|value| value.to_str())
                                                    .unwrap_or("");
                                                ((search.is_empty() || name.contains(&search))
                                                    && (extension_filter.is_empty()
                                                        || ext == extension_filter))
                                                    .then_some(index)
                                            })
                                            .collect::<Vec<_>>()
                                    };

                                    // Recursive Folder Masonry always benefits from
                                    // folder sections. Otherwise detect the first
                                    // different parent only once per collection/filter.
                                    let show_folder_headers = if !headers_enabled {
                                        false
                                    } else if folder_masonry && recursive {
                                        true
                                    } else if let Some(first_index) = filtered_indices.first() {
                                        let first = entry_folder_parts(
                                            &entries[*first_index],
                                            directory.as_deref(),
                                        );
                                        filtered_indices.iter().skip(1).any(|index| {
                                            entry_folder_parts(
                                                &entries[*index],
                                                directory.as_deref(),
                                            ) != first
                                        })
                                    } else {
                                        false
                                    };

                                    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
                                    if show_folder_headers {
                                        let mut group_indices = HashMap::<String, usize>::new();
                                        for entry_index in filtered_indices {
                                            let label = entry_breadcrumb(
                                                &entries[entry_index],
                                                directory.as_deref(),
                                            );
                                            if let Some(group_index) =
                                                group_indices.get(&label).copied()
                                            {
                                                groups[group_index].1.push(entry_index);
                                            } else {
                                                let group_index = groups.len();
                                                group_indices.insert(label.clone(), group_index);
                                                groups.push((label, vec![entry_index]));
                                            }
                                        }
                                    } else {
                                        groups.push((String::new(), filtered_indices));
                                    }

                                    let groups = Arc::new(groups);
                                    let active_layout_keys = groups
                                        .iter()
                                        .map(|(label, _)| label.clone())
                                        .collect::<HashSet<_>>();
                                    self.workspace.tabs[tab_index]
                                        .masonry_layout_cache
                                        .retain(|key, _| active_layout_keys.contains(key));
                                    self.workspace.tabs[tab_index].masonry_groups_cache =
                                        Some(MasonryGroupsCache {
                                            fingerprint: groups_fingerprint,
                                            groups: Arc::clone(&groups),
                                        });
                                    groups
                                };

                                let no_matches = groups
                                    .first()
                                    .is_none_or(|(_, indices)| indices.is_empty());
                                if no_matches {
                                    grid_ui.allocate_ui_with_layout(
                                        egui::vec2(
                                            content_width,
                                            (content_rect.height() - 32.0).max(60.0),
                                        ),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |empty_ui| {
                                            empty_ui.add_space(
                                                (content_rect.height() * 0.25).max(20.0),
                                            );
                                            empty_ui.label(if entries.is_empty() {
                                                "No supported images in this collection."
                                            } else {
                                                "No images match the current filters."
                                            });
                                        },
                                    );
                                } else {
                                    let group_count = groups.len();
                                    for (group_index, (folder_label, group_entry_indices)) in
                                        groups.iter().enumerate()
                                    {
                                        let mut render_group = true;
                                        if !folder_label.is_empty() {
                                            let header_width = grid_ui.available_width();
                                            let collapsed = self.workspace.tabs[tab_index]
                                                .collapsed_folders
                                                .contains(folder_label);
                                            let (header_rect, header_response) = grid_ui
                                                .allocate_exact_size(
                                                    egui::vec2(header_width, 26.0),
                                                    egui::Sense::click(),
                                                );
                                            let visuals = grid_ui.visuals().clone();
                                            let fill = if header_response.hovered() {
                                                visuals.widgets.hovered.bg_fill
                                            } else {
                                                visuals.faint_bg_color
                                            };
                                            let painter = grid_ui.painter();
                                            painter.rect_filled(
                                                header_rect,
                                                egui::CornerRadius::same(6),
                                                fill,
                                            );
                                            if header_response.hovered() {
                                                painter.rect_stroke(
                                                    header_rect,
                                                    egui::CornerRadius::same(6),
                                                    visuals.widgets.hovered.fg_stroke,
                                                    egui::StrokeKind::Inside,
                                                );
                                            }
                                            painter.text(
                                                egui::pos2(
                                                    header_rect.left() + 9.0,
                                                    header_rect.center().y,
                                                ),
                                                egui::Align2::LEFT_CENTER,
                                                if collapsed { "▸" } else { "▾" },
                                                egui::FontId::proportional(13.0),
                                                visuals.text_color(),
                                            );
                                            painter.text(
                                                egui::pos2(
                                                    header_rect.left() + 26.0,
                                                    header_rect.center().y,
                                                ),
                                                egui::Align2::LEFT_CENTER,
                                                format!("📁 {folder_label}"),
                                                egui::FontId::proportional(12.5),
                                                visuals.text_color(),
                                            );
                                            painter.text(
                                                egui::pos2(
                                                    header_rect.right() - 9.0,
                                                    header_rect.center().y,
                                                ),
                                                egui::Align2::RIGHT_CENTER,
                                                format!("{} images", group_entry_indices.len()),
                                                egui::FontId::proportional(11.0),
                                                visuals.weak_text_color(),
                                            );
                                            if header_response.clicked() {
                                                let collapsed_folders = &mut self.workspace.tabs
                                                    [tab_index]
                                                    .collapsed_folders;
                                                if collapsed {
                                                    collapsed_folders.remove(folder_label);
                                                } else {
                                                    collapsed_folders.insert(folder_label.to_owned());
                                                }
                                            }
                                            render_group = !collapsed;
                                            if render_group {
                                                grid_ui.add_space(4.0);
                                            }
                                        }
                                        if render_group {
                                            self.render_masonry_group(
                                                grid_ui,
                                                tab_index,
                                                tab_id,
                                                entries,
                                                group_entry_indices,
                                                &selected,
                                                &selected_entries,
                                                preferred_size,
                                                folder_label,
                                                &mut visible_masonry_sources,
                                                ctx,
                                                &mut action,
                                            );
                                        }
                                        if group_index + 1 < group_count {
                                            grid_ui.add_space(8.0);
                                        }
                                    }
                                }
                            });
                            outer.add_space(12.0);
                        });
                        scroll_ui.add_space(12.0);
                    });
            },
        );

        self.masonry_visible_sources = visible_masonry_sources.clone();
        crate::app::queues::update_masonry_thumbnail_visibility(&visible_masonry_sources);
        // Restore the collection before drawing the footer or dispatching any
        // deferred actions; those controls operate on the tab's owned entries.
        self.workspace.tabs[tab_index].entries = masonry_entries;

        // Collect only after the current viewport has been recomputed and the
        // tab's entries restored. This prevents just-finished off-screen work
        // from being uploaded using stale visibility and preserves aspect-ratio
        // propagation into the active tab.
        self.collect_sidebar_thumbnails(ctx);

        // Look-aware bottom dock. Normal mode is exactly two rows; selection mode
        // adds only one third row for bulk selection and file-management actions.
        let settings = self.settings_manager.get().clone();
        let surface = crate::ui_theme::main_view_surface_color(
            ctx,
            &settings.main_view_background,
            settings.main_view_background_opacity,
        );
        let profile = crate::ui_theme::current_look_profile(ctx);
        let radius = match profile {
            "win98" => 0,
            "cosmic" | "gnome" => 10,
            _ => 14,
        };
        let dock_stroke = if matches!(profile, "cosmic" | "gnome" | "win98") {
            ui.visuals().widgets.noninteractive.bg_stroke
        } else {
            egui::Stroke::NONE
        };

        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(footer_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |footer_ui| {
                egui::Frame::new()
                    .fill(surface)
                    .stroke(dock_stroke)
                    .corner_radius(egui::CornerRadius::same(radius))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(footer_ui, |dock| {
                        dock.spacing_mut().item_spacing.y = 3.0;
                        let tab = &mut self.workspace.tabs[tab_index];

                        // Row 1: search/filter/sort/size controls and the Select
                        // toggle share one line. The toggle stays pinned at the right;
                        // controls scroll horizontally when the central view is narrow.
                        dock.horizontal(|row| {
                            let select_width = 116.0;
                            let controls_width = (row.available_width() - select_width - 8.0).max(0.0);
                            egui::ScrollArea::horizontal()
                                .id_salt(("masonry_controls_row", tab.id))
                                .auto_shrink([false, true])
                                .max_width(controls_width)
                                .max_height(28.0)
                                .show(row, |controls| {
                                    controls.horizontal(|controls| {
                                        controls.add_sized(
                                            egui::vec2(150.0, 22.0),
                                            egui::TextEdit::singleline(&mut tab.search).hint_text("Search images"),
                                        );
                                        egui::ComboBox::from_id_salt(("masonry_format", tab.id))
                                            .width(100.0)
                                            .selected_text(if tab.extension_filter.is_empty() { "All formats" } else { &tab.extension_filter })
                                            .show_ui(controls, |menu| {
                                                menu.selectable_value(&mut tab.extension_filter, String::new(), "All formats");
                                                for ext in crate::imaging::core::ImageFormat::all_extensions() {
                                                    menu.selectable_value(&mut tab.extension_filter, ext.clone(), ext.to_uppercase());
                                                }
                                            });
                                        controls.separator();
                                        egui::ComboBox::from_id_salt(("masonry_sort", tab.id))
                                            .width(132.0)
                                            .selected_text(sort_method_label(selected_sort_method))
                                            .show_ui(controls, |menu| {
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::Natural, "Name (natural)");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::DateNewest, "Newest first");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::DateOldest, "Oldest first");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::Shuffle, "Random");
                                            });
                                        controls.separator();
                                        controls.label("Size");
                                        let slider_width = (controls.available_width() * 0.20).clamp(90.0, 150.0);
                                        let size_response = controls.add_sized(
                                            egui::vec2(slider_width, 20.0),
                                            egui::Slider::new(&mut tile_size, 76.0..=512.0).step_by(4.0).show_value(true),
                                        );
                                        persist_tile_size = size_response.drag_stopped()
                                            || (size_response.changed() && !size_response.dragged());
                                        if tab.folder_masonry {
                                            controls.checkbox(&mut tab.recursive, "Subfolders");
                                            if controls.small_button("↻").on_hover_text("Refresh folder contents").clicked() {
                                                reload_folder = true;
                                            }
                                        }
                                    });
                                });
                            row.add_space(4.0);
                            if row.add_sized(
                                [select_width, 26.0],
                                egui::Button::new(if tab.selection_mode { "Finish selecting" } else { "Select" }),
                            ).clicked() {
                                tab.selection_mode = !tab.selection_mode;
                                if !tab.selection_mode {
                                    tab.selected.clear();
                                }
                            }
                        });

                        // Row 2: workspace/folder context on the left and the
                        // selected count directly below the Select button on the right.
                        dock.horizontal(|row| {
                            row.label(egui::RichText::new(tab.title.clone()).strong());
                            if let Some(directory_name) = tab.directory.as_ref().and_then(|path| path.file_name()) {
                                row.label(egui::RichText::new(directory_name.to_string_lossy()).small().weak());
                            }
                            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                                if selection_mode {
                                    right.strong(format!("{} selected", tab.selected.len()));
                                } else {
                                    right.label(format!("{} images", tab.entries.len()));
                                }
                            });
                        });

                        // Row 3 exists only while selection mode is active. Use the
                        // frame-start state so the footer height and row count always
                        // agree; toggles take effect on the immediately following frame.
                        if selection_mode {
                            dock.horizontal(|row| {
                                egui::ScrollArea::horizontal()
                                    .id_salt(("masonry_selection_actions", tab.id))
                                    .auto_shrink([false, true])
                                    .max_height(28.0)
                                    .show(row, |actions| {
                                        actions.horizontal(|actions| {
                                            if actions.button("Select all visible").clicked() {
                                                let search = tab.search.to_lowercase();
                                                let extension_filter = tab.extension_filter.to_lowercase();
                                                tab.selected = tab.entries.iter().filter(|entry| {
                                                    let name = entry_display_name(entry).to_lowercase();
                                                    let extension = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
                                                    (search.is_empty() || name.contains(&search))
                                                        && (extension_filter.is_empty() || extension == extension_filter)
                                                }).map(ImageEntry::get_id).collect();
                                            }
                                            if actions.button("Clear").clicked() { tab.selected.clear(); }
                                            if actions.button("View selected").clicked() { action = Some(MasonryAction::ViewSelected); }
                                            if actions.button("Open in new tabs").clicked() { action = Some(MasonryAction::OpenSelected); }
                                            let compare_minimum = if has_pending_compare_target { 1 } else { 2 };
                                            let compare_label = if has_pending_compare_target { "Add to Compare" } else { "Compare" };
                                            if actions.add_enabled(tab.selected.len() >= compare_minimum, egui::Button::new(compare_label)).clicked() {
                                                action = Some(MasonryAction::Compare);
                                            }
                                            if actions.add_enabled(!tab.selected.is_empty(), egui::Button::new("Copy / Extract…")).clicked() {
                                                action = Some(MasonryAction::Copy);
                                            }
                                            let all_files = !tab.selected.is_empty() && tab.entries.iter()
                                                .filter(|entry| tab.selected.contains(&entry.get_id()))
                                                .all(|entry| matches!(entry, ImageEntry::File(_)));
                                            if actions.add_enabled(all_files, egui::Button::new("Move…")).clicked() {
                                                action = Some(MasonryAction::Move);
                                            }
                                            if actions.add_enabled(!tab.selected.is_empty(), egui::Button::new("Delete…")).clicked() {
                                                action = Some(MasonryAction::Delete);
                                            }
                                        });
                                    });
                            });
                        }
                    });
            },
        );

        // The tile-size slider updates every open Masonry tab immediately, but
        // writes settings.ini only after the user finishes the adjustment.
        if (tile_size - previous_tile_size).abs() > f32::EPSILON {
            for tab in &mut self.workspace.tabs {
                if tab.kind == WorkspaceTabKind::Masonry {
                    tab.masonry_tile_size = tile_size;
                }
            }
            if persist_tile_size {
                let _ = self.settings_manager.update(|settings| {
                    settings.masonry_thumbnail_size = tile_size;
                });
            }
        }

        if selected_sort_method != self.sort_method {
            self.sort_method = selected_sort_method;
            let tab = &mut self.workspace.tabs[tab_index];
            crate::imaging::sort::sort_images(&mut tab.entries, self.sort_method);
            tab.invalidate_masonry_layout();
        }

        if reload_folder {
            let (directory, recursive) = {
                let tab = &self.workspace.tabs[tab_index];
                (tab.directory.clone(), tab.recursive)
            };
            if let Some(directory) =
                directory.filter(|_| self.workspace.tabs[tab_index].folder_masonry)
            {
                let paths = if recursive {
                    let mut paths = crate::app::queues::collect_folder_images(&directory);
                    crate::imaging::sort::sort_images(&mut paths, self.sort_method);
                    paths
                } else {
                    crate::imaging::loader::load_directory_images(&directory, self.sort_method)
                };
                let refreshed = paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
                let valid_ids = refreshed
                    .iter()
                    .map(ImageEntry::get_id)
                    .collect::<HashSet<_>>();
                let tab = &mut self.workspace.tabs[tab_index];
                tab.entries = refreshed;
                tab.invalidate_masonry_layout();
                tab.masonry_aspect_ratios
                    .retain(|id, _| valid_ids.contains(id));
                tab.masonry_dimensions
                    .retain(|id, _| valid_ids.contains(id));
                tab.selected.retain(|id| valid_ids.contains(id));
            }
        }

        if let Some(action) = action {
            self.handle_masonry_action(tab_index, action, ctx);
        }
        self.render_delete_confirmation(ctx);
        self.render_file_conflict_dialog(ctx);
    }
}
