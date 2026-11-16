//! Masonry workspace surface, filtering, sorting, selection, and grid layout.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    /// Returns true when a non-viewer workspace rendered the central surface.
    pub(crate) fn render_active_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        match self.workspace.active_kind() {
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image => false,
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
        self.collect_sidebar_thumbnails(ctx);
        let Some(tab_index) = self.workspace.tabs.get(self.workspace.active_index)
            .map(|tab| self.workspace.tabs.iter().position(|candidate| candidate.id == tab.id).unwrap_or(self.workspace.active_index))
        else { return; };

        let mut action: Option<MasonryAction> = None;
        let mut reload_folder = false;
        let mut selected_sort_method = self.sort_method;
        let previous_tile_size = self.workspace.tabs[tab_index].masonry_tile_size.clamp(76.0, 512.0);
        let mut tile_size = previous_tile_size;
        let mut persist_tile_size = false;
        let tab_id = self.workspace.tabs[tab_index].id;
        let selection_mode = self.workspace.tabs[tab_index].selection_mode;
        let has_pending_compare_target = self.workspace.pending_compare_target_id.is_some();

        // Masonry is a bottom-docked workspace: the image grid owns the flexible
        // area and scrolls independently, while the themed two/three-row toolbar
        // stays anchored to the bottom edge of the central view.
        let layout_rect = ui.available_rect_before_wrap()
            .intersect(ui.clip_rect())
            .intersect(self.main_view_rect(ctx));
        if layout_rect.width() < 80.0 || layout_rect.height() < 40.0 {
            return;
        }
        let footer_height = if selection_mode { 112.0 } else { 82.0 };
        let footer_bottom = layout_rect.bottom() - 8.0;
        let footer_top = (footer_bottom - footer_height).max(layout_rect.top() + 24.0);
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, footer_top),
            egui::pos2((layout_rect.right() - 8.0).max(layout_rect.left() + 8.0), footer_bottom),
        );
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, layout_rect.top() + 8.0),
            egui::pos2(
                (layout_rect.right() - 8.0).max(layout_rect.left() + 8.0),
                (footer_top - 8.0).max(layout_rect.top() + 8.0),
            ),
        );

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

                                let (entries, selected, search, extension_filter, folder_masonry, recursive, directory, preferred_size) = {
                                    let tab = &self.workspace.tabs[tab_index];
                                    (
                                        tab.entries.clone(),
                                        tab.selected.clone(),
                                        tab.search.to_lowercase(),
                                        tab.extension_filter.to_lowercase(),
                                        tab.folder_masonry,
                                        tab.recursive,
                                        tab.directory.clone(),
                                        tab.masonry_tile_size,
                                    )
                                };
                                let filtered = entries.into_iter().filter(|entry| {
                                    let name = entry_display_name(entry).to_lowercase();
                                    let ext = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
                                    (search.is_empty() || name.contains(&search))
                                        && (extension_filter.is_empty() || ext == extension_filter)
                                }).collect::<Vec<_>>();

                                if filtered.is_empty() {
                                    grid_ui.allocate_ui_with_layout(
                                        egui::vec2(content_width, (content_rect.height() - 32.0).max(60.0)),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |empty_ui| {
                                            empty_ui.add_space((content_rect.height() * 0.25).max(20.0));
                                            empty_ui.label(if self.workspace.tabs[tab_index].entries.is_empty() {
                                                "No supported images in this collection."
                                            } else {
                                                "No images match the current filters."
                                            });
                                        },
                                    );
                                } else {
                                    let headers_enabled = self.settings_manager.get().masonry_show_folder_headers;
                                    // Recursive Folder Masonry always benefits from folder sections.
                                    // Local collections also get grouped when they contain images
                                    // from more than one source folder (or archive subfolder), so
                                    // the user can tell where each entry belongs while scrolling.
                                    let distinct_folders = filtered
                                        .iter()
                                        .map(|entry| entry_breadcrumb(entry, directory.as_deref()))
                                        .collect::<HashSet<_>>();
                                    let show_folder_headers = headers_enabled
                                        && ((folder_masonry && recursive) || distinct_folders.len() > 1);
                                    let mut groups: Vec<(String, Vec<ImageEntry>)> = Vec::new();
                                    if show_folder_headers {
                                        for entry in filtered {
                                            let label = entry_breadcrumb(&entry, directory.as_deref());
                                            if let Some((_, group_entries)) = groups.iter_mut().find(|(name, _)| *name == label) {
                                                group_entries.push(entry);
                                            } else {
                                                groups.push((label, vec![entry]));
                                            }
                                        }
                                    } else {
                                        groups.push((String::new(), filtered));
                                    }

                                    let group_count = groups.len();
                                    for (group_index, (folder_label, group_entries)) in groups.into_iter().enumerate() {
                                        let mut render_group = true;
                                        if !folder_label.is_empty() {
                                            let header_width = grid_ui.available_width();
                                            let collapsed = self.workspace.tabs[tab_index]
                                                .collapsed_folders
                                                .contains(&folder_label);
                                            let (header_rect, header_response) = grid_ui.allocate_exact_size(
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
                                            painter.rect_filled(header_rect, egui::CornerRadius::same(6), fill);
                                            if header_response.hovered() {
                                                painter.rect_stroke(
                                                    header_rect,
                                                    egui::CornerRadius::same(6),
                                                    visuals.widgets.hovered.fg_stroke,
                                                    egui::StrokeKind::Inside,
                                                );
                                            }
                                            painter.text(
                                                egui::pos2(header_rect.left() + 9.0, header_rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                                if collapsed { "▸" } else { "▾" },
                                                egui::FontId::proportional(13.0),
                                                visuals.text_color(),
                                            );
                                            painter.text(
                                                egui::pos2(header_rect.left() + 26.0, header_rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                                format!("📁 {folder_label}"),
                                                egui::FontId::proportional(12.5),
                                                visuals.text_color(),
                                            );
                                            painter.text(
                                                egui::pos2(header_rect.right() - 9.0, header_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                format!("{} images", group_entries.len()),
                                                egui::FontId::proportional(11.0),
                                                visuals.weak_text_color(),
                                            );
                                            if header_response.clicked() {
                                                let collapsed_folders = &mut self.workspace.tabs[tab_index].collapsed_folders;
                                                if collapsed {
                                                    collapsed_folders.remove(&folder_label);
                                                } else {
                                                    collapsed_folders.insert(folder_label.clone());
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
                                                &group_entries,
                                                &selected,
                                                preferred_size,
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
            crate::imaging::sort::sort_images(
                &mut self.workspace.tabs[tab_index].entries,
                self.sort_method,
            );
        }

        if reload_folder {
            let (directory, recursive) = {
                let tab = &self.workspace.tabs[tab_index];
                (tab.directory.clone(), tab.recursive)
            };
            if let Some(directory) = directory.filter(|_| self.workspace.tabs[tab_index].folder_masonry) {
                let paths = if recursive {
                    let mut paths = crate::app::queues::collect_folder_images(&directory);
                    crate::imaging::sort::sort_images(&mut paths, self.sort_method);
                    paths
                } else {
                    crate::imaging::loader::load_directory_images(&directory, self.sort_method)
                };
                let refreshed = paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
                let valid_ids = refreshed.iter().map(ImageEntry::get_id).collect::<HashSet<_>>();
                let tab = &mut self.workspace.tabs[tab_index];
                tab.entries = refreshed;
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
