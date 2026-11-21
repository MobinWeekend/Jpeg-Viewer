//! Tab activation, closing, ordering, keyboard shortcuts, and tab-strip rendering.

use super::helpers::*;
use super::*;

impl crate::app::types::ViewerApp {
    /// Resolve the active Image tab's own texture and animation. A stale texture
    /// from the previously active tab must never be shown in a new pop-out.
    pub(crate) fn active_image_popout_source(
        &self,
    ) -> Option<(
        u64,
        Option<egui::TextureHandle>,
        Option<crate::decoder::gif::animation::GifAnimation>,
        Option<(usize, usize, std::sync::Arc<Vec<u8>>)>,
    )> {
        let tab = self.workspace.tabs.get(self.workspace.active_index)?;
        if tab.kind != WorkspaceTabKind::Image {
            return None;
        }
        let tab_id = tab.id;
        let entry_id = tab.entries.first()?.get_id();
        let texture = if self.texture_image_id.as_deref() == Some(entry_id.as_str()) {
            self.texture.clone()
        } else {
            None
        };
        let animation = texture.as_ref().and_then(|_| self.gif_animation.clone());
        // Retain the original RGBA pixels for static images. Pop-out opacity is
        // applied to alpha in the pixel buffer, preserving color and PNG alpha.
        // Animated images use each GIF frame's own RGBA buffer instead.
        let source_pixels = if texture.is_some() && animation.is_none() {
            self.current_decoded_pixels
                .as_ref()
                .filter(|(source_id, _, _, _)| source_id == &entry_id)
                .map(|(_, width, height, pixels)| {
                    (*width as usize, *height as usize, std::sync::Arc::clone(pixels))
                })
        } else {
            None
        };
        Some((tab_id, texture, animation, source_pixels))
    }

    /// Switch ordinary file-open actions back to the collection-based Viewer tab.
    /// Explicitly opening a new path must take precedence over a restored Masonry
    /// or comparison workspace rather than silently changing an invisible viewer.
    pub(crate) fn activate_primary_viewer_tab(&mut self) {
        let Some(id) = self
            .workspace
            .tabs
            .iter()
            .find(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| {
                self.workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.kind == WorkspaceTabKind::Viewer)
            })
            .map(|tab| tab.id)
        else {
            return;
        };
        self.activate_workspace_tab(id);
        self.workspace.home_page_visible = false;
    }

    /// Sort the persistent Browse/Viewer collection without replacing whichever
    /// Image tab is currently displayed. The stable selected entry stays selected.
    pub(crate) fn sort_primary_viewer_entries(&mut self) {
        let Some(viewer_index) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| {
                self.workspace
                    .tabs
                    .iter()
                    .position(|tab| tab.kind == WorkspaceTabKind::Viewer)
            })
        else {
            return;
        };

        let viewer_is_active = self.workspace.active_index == viewer_index;
        let live_entries = viewer_is_active.then(|| self.image_entries.clone());
        let live_index = self.current_index;
        let (entries, current_index) = {
            let tab = &mut self.workspace.tabs[viewer_index];
            if let Some(entries) = live_entries {
                tab.entries = entries;
                tab.current_index = live_index;
                tab.directory = self.current_directory.clone();
            }
            if tab.entries.is_empty() {
                return;
            }
            let selected_id = tab.entries.get(tab.current_index).map(ImageEntry::get_id);
            crate::imaging::sort::sort_images(&mut tab.entries, self.sort_method);
            if let Some(selected_id) = selected_id
                && let Some(index) = tab
                    .entries
                    .iter()
                    .position(|entry| entry.get_id() == selected_id)
            {
                tab.current_index = index;
            }
            (tab.entries.clone(), tab.current_index)
        };

        if viewer_is_active {
            self.set_image_entries_preserving_cache(entries, current_index);
        }
        self.sidebar_last_current_index = None;
    }

    /// Reorder the primary Browse collection by its stable entry index. The
    /// current image is retained by identity, even when a different Image tab
    /// is active while the Browse sidebar remains visible.
    pub(crate) fn reorder_primary_viewer_entries(
        &mut self,
        from: usize,
        to: usize,
    ) -> bool {
        let Some(viewer_index) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| {
                self.workspace
                    .tabs
                    .iter()
                    .position(|tab| tab.kind == WorkspaceTabKind::Viewer)
            })
        else {
            return false;
        };
        let viewer_is_active = self.workspace.active_index == viewer_index;
        let live_entries = viewer_is_active.then(|| self.image_entries.clone());
        let live_index = self.current_index;
        let live_directory = self.current_directory.clone();
        let (entries, current_index, changed) = {
            let tab = &mut self.workspace.tabs[viewer_index];
            if let Some(entries) = live_entries {
                tab.entries = entries;
                tab.current_index = live_index;
                tab.directory = live_directory;
            }
            let len = tab.entries.len();
            if from >= len || to > len {
                return false;
            }
            let insert_at = if from < to { to.saturating_sub(1) } else { to };
            if insert_at == from || insert_at >= len {
                return false;
            }
            let selected_id = tab.entries.get(tab.current_index).map(ImageEntry::get_id);
            let entry = tab.entries.remove(from);
            tab.entries.insert(insert_at.min(tab.entries.len()), entry);
            if let Some(selected_id) = selected_id
                && let Some(index) = tab
                    .entries
                    .iter()
                    .position(|entry| entry.get_id() == selected_id)
            {
                tab.current_index = index;
            }
            (tab.entries.clone(), tab.current_index, true)
        };

        if changed && viewer_is_active {
            self.set_image_entries_preserving_cache(entries, current_index);
        }
        if changed {
            self.sidebar_last_current_index = None;
        }
        changed
    }

    /// Open Masonry from the Browse collection even when a one-image tab is
    /// currently active in the main Viewer surface.
    pub(crate) fn open_browse_masonry(&mut self) {
        let viewer_index = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| {
                self.workspace
                    .tabs
                    .iter()
                    .position(|tab| tab.kind == WorkspaceTabKind::Viewer)
            });
        let Some(viewer_index) = viewer_index else {
            return;
        };
        let (entries, directory) = if self.workspace.active_index == viewer_index {
            (self.image_entries.clone(), self.current_directory.clone())
        } else {
            let tab = &self.workspace.tabs[viewer_index];
            (tab.entries.clone(), tab.directory.clone())
        };
        if entries.is_empty() {
            self.queue_status = Some("There are no Browse images to show in Masonry.".into());
            return;
        }
        self.open_masonry_collection(
            entries,
            "Masonry · Browse".to_owned(),
            directory,
        );
    }

    pub(crate) fn activate_workspace_tab(&mut self, id: u64) {
        self.zoom_animation = None;
        let Some(target_index) = self.workspace.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if target_index == self.workspace.active_index {
            return;
        }
        self.sidebar_last_current_index = None;
        if self.workspace.active_kind() == WorkspaceTabKind::Image {
            self.workspace.last_image_tab_id = self.workspace.active_tab().map(|tab| tab.id);
        }
        self.snapshot_active_viewer_tab();
        self.workspace.active_index = target_index;
        let kind = self.workspace.tabs[target_index].kind;
        match kind {
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image => {
                let tab = &self.workspace.tabs[target_index];
                let entries = tab.entries.clone();
                let entry_count = entries.len();
                let index = tab.current_index.min(entry_count.saturating_sub(1));
                let directory = tab.directory.clone();
                let zoom = tab.zoom;
                let pan = tab.pan;
                let fit = tab.fit_to_window;
                let zoom_used = tab.zoom_used;
                self.current_directory = directory;

                // Masonry and Compare do not replace the underlying Viewer
                // collection. When returning to the same collection/index, keep
                // its in-flight decoder or displayed texture alive rather than
                // resetting the renderer and starting the decode again.
                let same_viewer_item = self.current_index == index
                    && self.image_entries.len() == entries.len()
                    && self
                        .image_entries
                        .iter()
                        .zip(entries.iter())
                        .all(|(current, restored)| current.get_id() == restored.get_id());
                if !same_viewer_item {
                    self.set_image_entries_preserving_cache(entries, index);
                }
                self.zoom = zoom.max(0.01);
                self.pan = pan;
                self.b_fit_to_window = fit;
                self.b_zoom_used = zoom_used;
                if kind == WorkspaceTabKind::Image {
                    self.workspace.last_image_tab_id = Some(id);
                }
                self.should_stop_caching = false;
            }
            WorkspaceTabKind::Masonry | WorkspaceTabKind::Compare | WorkspaceTabKind::Welcome => {
                self.stop_preloading_for_workspace();
            }
        }
    }

    /// Open an entry in its own one-image tab. If that exact entry already has
    /// an Image tab, focus the existing tab rather than creating a duplicate.
    pub(crate) fn open_entry_in_image_tab(&mut self, entry: ImageEntry, activate: bool) {
        let entry_id = entry.get_id();

        // An image that already has a dedicated tab resolves to that tab instead
        // of creating a duplicate. Different images always receive distinct tabs.
        if let Some(index) = self.workspace.tabs.iter().position(|tab| {
            tab.kind == WorkspaceTabKind::Image
                && tab
                    .entries
                    .first()
                    .is_some_and(|existing| existing.get_id() == entry_id)
        }) {
            let id = self.workspace.tabs[index].id;
            self.workspace.last_image_tab_id = Some(id);
            if activate {
                self.activate_workspace_tab(id);
            }
            return;
        }

        // "Open in New Tab" is an explicit request for a distinct tab. Do not
        // reuse the active or most recently used Image tab: successive opens
        // must preserve every previously opened image in its own tab.
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(id, entry_display_name(&entry), WorkspaceTabKind::Image);
        tab.directory = entry_parent(&entry);
        tab.entries.push(entry);
        self.workspace.tabs.push(tab);
        let index = self.workspace.tabs.len() - 1;
        self.workspace.last_image_tab_id = Some(id);
        // Grow the LRU before snapshotting the outgoing image so caching it
        // cannot evict a useful Browse preload before the new tab slot exists.
        self.sync_image_cache_capacity();
        self.snapshot_active_viewer_tab();
        if activate {
            self.workspace.active_index = index;
            self.sidebar_last_current_index = None;
            let entries = self.workspace.tabs[index].entries.clone();
            self.current_directory = self.workspace.tabs[index].directory.clone();
            self.set_image_entries_preserving_cache(entries, 0);
            self.should_stop_caching = false;
            self.workspace.tabs[index].zoom = 1.0;
            self.workspace.tabs[index].pan = egui::Vec2::ZERO;
            self.workspace.tabs[index].fit_to_window = true;
            self.workspace.tabs[index].zoom_used = false;
        }
    }

    pub(crate) fn open_compare_workspace(&mut self, entries: Vec<ImageEntry>) {
        // Returning from a one-pane Compare placeholder marks that comparison as
        // the destination. Masonry's Compare action then appends new entries to
        // it rather than opening an unrelated comparison tab.
        if let Some(target_id) = self.workspace.pending_compare_target_id.take()
            && let Some(index) = self
                .workspace
                .tabs
                .iter()
                .position(|tab| tab.id == target_id && tab.kind == WorkspaceTabKind::Compare)
        {
            if entries.is_empty() {
                // The menu-bar Compare command may intentionally return here
                // without a current image. Keep the existing workspace open so
                // its two empty panes can accept a drop or image-picker selection.
                self.workspace.active_index = index;
                self.stop_preloading_for_workspace();
                return;
            }
            let target_id = self.workspace.tabs[index].id;
            self.add_entries_to_compare(target_id, entries);
            self.workspace.active_index = index;
            self.stop_preloading_for_workspace();
            return;
        }
        // A comparison workspace owns its own entry list and pane state. It is
        // deliberately independent of ordinary Image tabs: creating, closing,
        // or reordering image tabs must not change this comparison. Empty and
        // one-image comparisons are valid; the renderer always supplies two
        // editable drop targets until both panes contain images.
        self.snapshot_active_viewer_tab();
        let mut unique_entries = Vec::with_capacity(entries.len());
        let mut seen = HashSet::new();
        for entry in entries {
            if seen.insert(entry.get_id()) {
                unique_entries.push(entry);
            }
        }
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(
            id,
            format!("Compare ({})", unique_entries.len()),
            WorkspaceTabKind::Compare,
        );
        tab.anchors = vec![None; unique_entries.len()];
        tab.anchors_second = vec![None; unique_entries.len()];
        tab.offsets = vec![[0.0, 0.0]; unique_entries.len()];
        tab.pane_zoom = vec![1.0; unique_entries.len()];
        tab.pane_rotation = vec![0.0; unique_entries.len()];
        tab.pane_sync = vec![true; unique_entries.len()];
        tab.pane_size = vec![[1.0, 1.0]; unique_entries.len()];
        tab.entries = unique_entries;
        tab.show_anchors = true;
        tab.sync_pan = true;
        tab.sync_zoom = true;
        tab.sync_navigation = false;
        self.workspace.tabs.push(tab);
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
    }

    /// Add unique entries to an existing Compare workspace. The two default
    /// panes (and any user-added empty panes) are filled before the layout grows.
    pub(crate) fn add_entries_to_compare(
        &mut self,
        compare_tab_id: u64,
        entries: Vec<ImageEntry>,
    ) -> usize {
        let Some(tab) = self
            .workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.id == compare_tab_id && tab.kind == WorkspaceTabKind::Compare)
        else {
            return 0;
        };
        // The renderer pads these vectors for empty placeholder panes.
        // Trim that UI-only padding before appending real entries so each new
        // image receives metadata at its actual entry index.
        let existing_count = tab.entries.len();
        tab.anchors.resize(existing_count, None);
        tab.anchors_second.resize(existing_count, None);
        tab.offsets.resize(existing_count, [0.0, 0.0]);
        tab.pane_zoom.resize(existing_count, 1.0);
        tab.pane_rotation.resize(existing_count, 0.0);
        tab.pane_sync.resize(existing_count, true);
        tab.pane_size.resize(existing_count, [1.0, 1.0]);
        let mut known = tab
            .entries
            .iter()
            .map(ImageEntry::get_id)
            .collect::<HashSet<_>>();
        let mut added: usize = 0;
        for entry in entries {
            if known.insert(entry.get_id()) {
                tab.entries.push(entry);
                tab.anchors.push(None);
                tab.anchors_second.push(None);
                tab.offsets.push([0.0, 0.0]);
                tab.pane_zoom.push(1.0);
                tab.pane_rotation.push(0.0);
                tab.pane_sync.push(true);
                tab.pane_size.push([1.0, 1.0]);
                added += 1;
            }
        }
        // The first two panes are implicit defaults. Only images beyond the
        // remaining default empty slots consume explicitly-added extra panes.
        let default_empty_slots = 2usize.saturating_sub(existing_count);
        let extra_slots_filled = added.saturating_sub(default_empty_slots);
        tab.compare_empty_panes = tab.compare_empty_panes.saturating_sub(extra_slots_filled);
        tab.title = format!("Compare ({})", tab.entries.len());
        added
    }

    pub(crate) fn close_workspace_tab(&mut self, id: u64) {
        self.snapshot_active_viewer_tab();
        let Some(index) = self.workspace.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if self.workspace.tabs[index].kind == WorkspaceTabKind::Viewer
            && self.workspace.tabs[index].id == 1
        {
            return;
        }
        let closing_active_tab = self.workspace.active_index == index;
        // A pop-out belongs to its Image tab and must not outlive that tab.
        self.popouts.close_tab(id);
        if self.workspace.pending_compare_target_id == Some(id) {
            self.workspace.pending_compare_target_id = None;
        }
        if self.workspace.masonry_return_tab_id == Some(id) {
            self.workspace.masonry_return_tab_id = None;
        }
        self.workspace.tabs.remove(index);
        self.sync_image_cache_capacity();
        if self.workspace.last_image_tab_id == Some(id) {
            self.workspace.last_image_tab_id = self
                .workspace
                .tabs
                .iter()
                .rev()
                .find(|tab| tab.kind == WorkspaceTabKind::Image)
                .map(|tab| tab.id);
        }
        if self.workspace.tabs.is_empty() {
            self.workspace.tabs.push(WorkspaceTab::viewer());
        }
        if self.workspace.active_index > index {
            self.workspace.active_index -= 1;
        } else if closing_active_tab {
            self.workspace.active_index = index.min(self.workspace.tabs.len() - 1);
        }
        // Only reload the underlying ViewerApp renderer when the visible tab was
        // closed. Closing a background tab must not interrupt the current image.
        if closing_active_tab {
            let active_id = self.workspace.tabs[self.workspace.active_index].id;
            self.workspace.active_index = usize::MAX;
            self.activate_workspace_tab(active_id);
        }
    }

    /// Whether the main-surface Masonry toggle can switch to or from the
    /// singleton Masonry workspace. Home is still the primary Viewer surface,
    /// so an already-loaded Viewer collection remains available from Home.
    pub(crate) fn masonry_toggle_available(&self) -> bool {
        let active_kind = self.workspace.active_kind();
        active_kind == WorkspaceTabKind::Masonry
            || (matches!(
                active_kind,
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
            ) && !self.image_entries.is_empty())
    }

    /// Whether the workspace tab strip is actually rendered in the current
    /// viewport. Overlay placement uses the same decision so fullscreen controls
    /// never reserve space for a row that is hidden with the sole Viewer tab.
    pub(crate) fn workspace_tab_strip_visible(&self, ctx: &egui::Context) -> bool {
        let has_secondary_tabs = self.workspace.tabs.iter().any(|tab| {
            !(tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
                && tab.kind != WorkspaceTabKind::Masonry
                && tab.kind != WorkspaceTabKind::Welcome
        });
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        let inline_titlebar_tabs = !self.settings_manager.get().show_titlebar
            && self.settings_manager.get().workspace_tabs_in_titlebar
            && !fullscreen;
        has_secondary_tabs && !inline_titlebar_tabs && (!fullscreen || self.overlay_visible)
    }

    /// Whether the navigation hamburger is hosted by the workspace strip.
    /// This is now reserved for fullscreen, where the strip occupies the top row.
    pub(crate) fn workspace_tab_strip_hosts_hamburger(&self, ctx: &egui::Context) -> bool {
        // The hamburger is part of the tab strip, not an independently floating
        // overlay. Its visibility therefore follows the exact same predicate as
        // the strip itself (including the sole-Viewer and fullscreen cases).
        if !self.workspace_tab_strip_visible(ctx) {
            return false;
        }
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        // With native decorations, the menu and Masonry toggle belong to the
        // floating navigation overlay beside zoom controls, not the document tab row.
        fullscreen
    }

    /// Compact tab controls for the custom title bar's single horizontal row.
    /// Returns the actual horizontal width used by the visible workspace tabs.
    /// The remaining title-bar space can then host the window title and drag area.
    pub(crate) fn render_workspace_tabs_inline(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> f32 {
        if self.settings_manager.get().show_titlebar
            || !self.settings_manager.get().workspace_tabs_in_titlebar
            || ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
            || self.workspace.tabs.len() <= 1
        {
            return 0.0;
        }

        let active_id = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| tab.id);
        let tabs = self
            .workspace
            .tabs
            .iter()
            .filter(|tab| {
                !(tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
                    && tab.kind != WorkspaceTabKind::Masonry
                    && tab.kind != WorkspaceTabKind::Welcome
            })
            .map(|tab| (tab.id, tab.title.clone(), tab.kind))
            .collect::<Vec<_>>();
        if tabs.is_empty() {
            return 0.0;
        }

        // Reserve only the actual tab widths, leaving remaining space for the
        // centered window title and a usable draggable title-bar region.
        let win98 = crate::ui_theme::current_look_profile(ctx) == "win98";
        let caption_button_width = if win98 { 22.0 } else { 46.0 };
        let controls_width = caption_button_width * 3.0 + 4.0;
        let tab_gap = 3.0;
        let desired_tabs_width = tabs
            .iter()
            .map(|(_, title, kind)| {
                let icon = match kind {
                    WorkspaceTabKind::Viewer => "▣",
                    WorkspaceTabKind::Image => "▧",
                    WorkspaceTabKind::Masonry => "",
                    WorkspaceTabKind::Compare => "⇄",
                    WorkspaceTabKind::Welcome => "⌂",
                };
                let short_title = if title.chars().count() > 16 {
                    format!("{}…", title.chars().take(15).collect::<String>())
                } else {
                    title.clone()
                };
                let label = format!("{icon}  {short_title}");
                (label.chars().count() as f32 * 7.0 + 42.0).clamp(88.0, 220.0)
            })
            .sum::<f32>()
            + tab_gap * tabs.len().saturating_sub(1) as f32;
        let min_drag_width = 84.0;
        let tab_width = desired_tabs_width.min(
            (ui.available_width() - controls_width - min_drag_width)
                .max(0.0)
                .min(720.0),
        );
        if tab_width < 64.0 {
            return 0.0;
        }
        let total_navigation_width = tab_width;

        let mut activate = None;
        let mut close = None;
        // Match the actual caption-control row height and center the chips in
        // that full-height host, including the shorter Win98 title-bar profile.
        let row_height = if win98 { 24.0 } else { 36.0 };
        let chip_height = if win98 { 22.0 } else { 28.0 };
        ui.allocate_ui_with_layout(
            egui::vec2(total_navigation_width, row_height),
            egui::Layout::left_to_right(egui::Align::Center),
            |host| {
                host.allocate_ui_with_layout(
                    egui::vec2(tab_width, row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |tab_host| {
                        tab_host.set_min_width(tab_width);
                        tab_host.set_max_width(tab_width);
                        egui::ScrollArea::horizontal()
                            .id_salt("custom_titlebar_workspace_tabs")
                            .auto_shrink([false, false])
                            .max_width(tab_width)
                            .max_height(row_height)
                            .show(tab_host, |scroll| {
                                scroll.set_min_height(row_height);
                                scroll.set_max_height(row_height);
                                scroll.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |row| {
                                        row.set_min_height(row_height);
                                        row.set_max_height(row_height);
                                        row.spacing_mut().item_spacing.x = 3.0;
                                        for (id, title, kind) in &tabs {
                                            let active = Some(*id) == active_id;
                                            let can_close =
                                                !(*kind == WorkspaceTabKind::Viewer && *id == 1);
                                            let icon = match kind {
                                                WorkspaceTabKind::Viewer => "▣",
                                                WorkspaceTabKind::Image => "▧",
                                                WorkspaceTabKind::Masonry => "▦",
                                                WorkspaceTabKind::Compare => "⇄",
                                                WorkspaceTabKind::Welcome => "⌂",
                                            };
                                            let short_title = if title.chars().count() > 16 {
                                                format!("{}…", title.chars().take(15).collect::<String>())
                                            } else {
                                                title.clone()
                                            };
                                            let label = format!("{icon}  {short_title}");
                                            let width = (label.chars().count() as f32 * 7.0
                                                + if can_close { 42.0 } else { 22.0 })
                                            .clamp(88.0, 220.0);
                                            let (rect, response) = row.allocate_exact_size(
                                                egui::vec2(width, chip_height),
                                                egui::Sense::click_and_drag(),
                                            );
                                            let visuals = row.visuals();
                                            let rounding =
                                                if crate::ui_theme::current_look_profile(ctx) == "win98" {
                                                    egui::CornerRadius::ZERO
                                                } else {
                                                    egui::CornerRadius::same(7)
                                                };
                                            let fill = if active {
                                                visuals.selection.bg_fill
                                            } else if response.hovered() {
                                                visuals.widgets.hovered.bg_fill
                                            } else {
                                                egui::Color32::TRANSPARENT
                                            };
                                            row.painter().rect_filled(rect, rounding, fill);
                                            row.painter().rect_stroke(
                                                rect,
                                                rounding,
                                                if active {
                                                    visuals.selection.stroke
                                                } else {
                                                    egui::Stroke::new(
                                                        0.5_f32,
                                                        visuals.widgets.inactive.bg_stroke.color,
                                                    )
                                                },
                                                egui::StrokeKind::Inside,
                                            );
                                            let close_rect = egui::Rect::from_center_size(
                                                egui::pos2(rect.right() - 13.0, rect.center().y),
                                                egui::vec2(20.0, 20.0),
                                            );
                                            let label_rect = egui::Rect::from_min_max(
                                                rect.min + egui::vec2(8.0, 1.0),
                                                egui::pos2(
                                                    if can_close {
                                                        close_rect.left() - 2.0
                                                    } else {
                                                        rect.right() - 5.0
                                                    },
                                                    rect.bottom() - 1.0,
                                                ),
                                            );
                                            row.painter().with_clip_rect(label_rect).text(
                                                egui::pos2(rect.left() + 8.0, rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                                label,
                                                egui::FontId::proportional(12.5),
                                                if active {
                                                    visuals.text_color()
                                                } else {
                                                    visuals.weak_text_color()
                                                },
                                            );
                                            let close_hovered = can_close
                                                && ctx
                                                    .pointer_hover_pos()
                                                    .is_some_and(|pos| close_rect.contains(pos));
                                            if can_close && (response.hovered() || close_hovered) {
                                                row.painter().circle_filled(
                                                    close_rect.center(),
                                                    8.5,
                                                    visuals.widgets.hovered.bg_fill,
                                                );
                                                row.painter().text(
                                                    close_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "×",
                                                    egui::FontId::proportional(16.0),
                                                    visuals.text_color(),
                                                );
                                            }

                                            if response.clicked_by(egui::PointerButton::Middle) {
                                                if can_close {
                                                    close = Some(*id);
                                                }
                                            } else if response.clicked() {
                                                if can_close
                                                    && response
                                                        .interact_pointer_pos()
                                                        .is_some_and(|pos| close_rect.contains(pos))
                                                {
                                                    close = Some(*id);
                                                } else {
                                                    activate = Some(*id);
                                                }
                                            }
                                            if response.drag_started()
                                                && !self.window_mouse_operation_active
                                                && response
                                                    .interact_pointer_pos()
                                                    .is_some_and(|pos| !close_rect.contains(pos))
                                            {
                                                self.workspace.dragging_tab_id = Some(*id);
                                            }
                                        }
                                    },
                                );
                            });
                    },
                );
            },
        );

        if let Some(id) = close {
            self.close_workspace_tab(id);
        } else if let Some(id) = activate {
            self.activate_workspace_tab(id);
        }
        total_navigation_width
    }

    pub(crate) fn render_workspace_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.handle_workspace_tab_shortcuts(ctx);
        // In fullscreen the strip is a real CentralPanel row, not a floating
        // overlay. This reserves its height so Masonry, Compare, Home, and the
        // image viewport all begin below it instead of being covered by the tabs.
        // When only the primary Viewer remains, hide the workspace strip.
        if self.workspace_tab_strip_visible(ctx) {
            self.render_workspace_tab_strip(ui, ctx, self.main_view_width(ctx));
        }
    }

    pub(super) fn handle_workspace_tab_shortcuts(&mut self, ctx: &egui::Context) {
        // Browser-style keyboard navigation/reordering. Do not steal shortcuts
        // while the user is typing in a search field or another text editor.
        if ctx.wants_keyboard_input() {
            return;
        }
        let tab_action = ctx.input(|input| {
            let ctrl = input.modifiers.ctrl;
            let shift = input.modifiers.shift;
            if ctrl
                && shift
                && (input.key_pressed(egui::Key::PageUp) || input.key_pressed(egui::Key::ArrowLeft))
            {
                Some(-2_i8)
            } else if ctrl
                && shift
                && (input.key_pressed(egui::Key::PageDown)
                    || input.key_pressed(egui::Key::ArrowRight))
            {
                Some(2_i8)
            } else if ctrl && input.key_pressed(egui::Key::PageUp) {
                Some(-1_i8)
            } else if ctrl && input.key_pressed(egui::Key::PageDown) {
                Some(1_i8)
            } else {
                None
            }
        });
        if let Some(action) = tab_action {
            if action == -2 || action == 2 {
                self.move_active_workspace_tab(if action < 0 { -1 } else { 1 });
            } else {
                let count = self.workspace.tabs.len();
                if count > 1 {
                    let next = if action < 0 {
                        self.workspace.active_index.saturating_sub(1)
                    } else {
                        (self.workspace.active_index + 1).min(count - 1)
                    };
                    if let Some(id) = self.workspace.tabs.get(next).map(|tab| tab.id) {
                        self.activate_workspace_tab(id);
                    }
                }
            }
        }
    }

    pub(super) fn render_workspace_tab_strip(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        strip_width: f32,
    ) {
        // Allocate exactly the CentralPanel width. In particular, do not ask the
        // Frame's inner UI for the full outer width: its horizontal margins would
        // otherwise make the tab row wider than the space between sidebars.
        let strip_width = strip_width.min(ui.available_width().max(0.0)).max(0.0);
        if self.workspace.tabs.is_empty() || strip_width < 48.0 {
            return;
        }
        let active_id = self
            .workspace
            .tabs
            .get(
                self.workspace
                    .active_index
                    .min(self.workspace.tabs.len() - 1),
            )
            .map(|tab| tab.id);
        let active_kind = self.workspace.active_kind();
        let descriptors = self
            .workspace
            .tabs
            .iter()
            // Viewer/Home and Masonry are two modes of the main surface, not
            // document tabs. Masonry is represented by the dedicated toggle.
            .filter(|tab| {
                !(tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
                    && tab.kind != WorkspaceTabKind::Masonry
                    && tab.kind != WorkspaceTabKind::Welcome
            })
            .map(|tab| {
                (
                    tab.id,
                    tab.title.clone(),
                    tab.kind,
                    Some(tab.id) == active_id,
                )
            })
            .collect::<Vec<_>>();
        let masonry_active = active_kind == WorkspaceTabKind::Masonry;
        let masonry_available = self.masonry_toggle_available();
        if descriptors.is_empty() && !masonry_available {
            return;
        }
        let show_tab_strip_hamburger = self.workspace_tab_strip_hosts_hamburger(ctx);
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        let show_tab_strip_masonry_toggle = masonry_available && fullscreen;
        let mut activate = None;
        let mut close = None;
        let mut tab_rects: Vec<(u64, egui::Rect)> = Vec::with_capacity(descriptors.len());

        let profile = crate::ui_theme::current_look_profile(ctx);
        let tab_rounding = if profile == "win98" {
            egui::CornerRadius::ZERO
        } else if profile == "cosmic" || profile == "gnome" {
            egui::CornerRadius::same(9)
        } else {
            egui::CornerRadius::same(7)
        };
        let native_titlebar = self.settings_manager.get().show_titlebar;
        let strip_height = if native_titlebar { 40.0 } else { 48.0 };
        ui.allocate_ui_with_layout(
            egui::vec2(strip_width, strip_height),
            egui::Layout::top_down(egui::Align::Min),
            |slot| {
                slot.set_min_width(strip_width);
                slot.set_max_width(strip_width);
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 6,
                        top: if native_titlebar { 0 } else { 1 },
                        bottom: if native_titlebar { 0 } else { 1 },
                    })
                    .show(slot, |body| {
                        let body_width = (strip_width - 12.0).max(0.0);
                        body.set_min_width(body_width);
                        body.set_max_width(body_width);
                        let body_height = body.available_height().max(32.0);
                        body.allocate_ui_with_layout(
                            egui::vec2(body_width, body_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |row| {
                                row.set_min_height(body_height);
                                row.set_max_height(body_height);
                                row.spacing_mut().item_spacing.x = 4.0;
                                // When the tab strip hosts the menu, keep it before the
                                // first tab rather than overlaying or following the tabs.
                                if show_tab_strip_hamburger {
                                    self.render_hamburger_ui(row);
                                    row.add_space(2.0);
                                }
                                let hamburger_width = if show_tab_strip_hamburger {
                                    if profile == "cosmic" || profile == "gnome" {
                                        82.0
                                    } else {
                                        40.0
                                    }
                                } else {
                                    0.0
                                };
                                let masonry_toggle_width = if show_tab_strip_masonry_toggle {
                                    34.0
                                } else {
                                    0.0
                                };
                                if show_tab_strip_masonry_toggle {
                                    let toggle = crate::app::ui::masonry_toggle_button(
                                        row,
                                        masonry_active,
                                        true,
                                        30.0,
                                    )
                                    .on_hover_text(if masonry_active {
                                        "Return to Viewer"
                                    } else {
                                        "Open current entries in Masonry"
                                    });
                                    if toggle.clicked() {
                                        self.activate_masonry_from_navigation_button();
                                    }
                                }
                                let tab_viewport_width = (body_width
                                    - hamburger_width
                                    - masonry_toggle_width)
                                    .max(0.0);
                                row.allocate_ui_with_layout(
                                    egui::vec2(tab_viewport_width, 32.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |scroll_host| {
                                        scroll_host.set_min_width(tab_viewport_width);
                                        scroll_host.set_max_width(tab_viewport_width);
                                        egui::ScrollArea::horizontal()
                                            .id_salt("workspace_firefox_tab_strip")
                                            .auto_shrink([false, true])
                                            .max_width(tab_viewport_width)
                                            .max_height(32.0)
                                            .show(scroll_host, |tabs_ui| {
                                                tabs_ui.spacing_mut().item_spacing.x = 4.0;
                                                tabs_ui.horizontal(|tabs_ui| {
                                                    for (id, title, kind, active) in descriptors {
                                                        let icon = match kind {
                                                            WorkspaceTabKind::Viewer => "▣",
                                                            WorkspaceTabKind::Image => "▧",
                                                            WorkspaceTabKind::Masonry => "",
                                                            WorkspaceTabKind::Compare => "⇄",
                                                            WorkspaceTabKind::Welcome => "✦",
                                                        };
                                                        let display_title =
                                                            if title.chars().count() > 20 {
                                                                format!(
                                                                    "{}…",
                                                                    title
                                                                        .chars()
                                                                        .take(19)
                                                                        .collect::<String>()
                                                                )
                                                            } else {
                                                                title.clone()
                                                            };
                                                        let can_close = kind
                                                            != WorkspaceTabKind::Viewer
                                                            || id != 1;
                                                        // Reserve a predictable close-button gutter and clip the
                                                        // label to its own tab, even for long folder/image titles.
                                                        let label_width =
                                                            display_title.chars().count() as f32
                                                                * 6.7
                                                                + 19.0;
                                                        let width = (label_width
                                                            + if can_close { 38.0 } else { 20.0 })
                                                        .clamp(96.0, 196.0);
                                                        let (rect, response) = tabs_ui
                                                            .allocate_exact_size(
                                                                egui::vec2(width, 32.0),
                                                                egui::Sense::click_and_drag(),
                                                            );
                                                        let visuals = tabs_ui.visuals();
                                                        let fill = if active {
                                                            visuals.selection.bg_fill
                                                        } else if response.hovered() {
                                                            visuals.widgets.hovered.bg_fill
                                                        } else {
                                                            egui::Color32::TRANSPARENT
                                                        };
                                                        let stroke = if active {
                                                            visuals.selection.stroke
                                                        } else {
                                                            visuals.widgets.inactive.bg_stroke
                                                        };
                                                        tabs_ui.painter().rect_filled(
                                                            rect,
                                                            tab_rounding,
                                                            fill,
                                                        );
                                                        tabs_ui.painter().rect_stroke(
                                                            rect,
                                                            tab_rounding,
                                                            if active {
                                                                stroke
                                                            } else {
                                                                egui::Stroke::new(
                                                                    0.5_f32,
                                                                    stroke.color,
                                                                )
                                                            },
                                                            egui::StrokeKind::Inside,
                                                        );
                                                        let close_rect =
                                                            egui::Rect::from_center_size(
                                                                egui::pos2(
                                                                    rect.right() - 14.0,
                                                                    rect.center().y,
                                                                ),
                                                                egui::vec2(22.0, 22.0),
                                                            );
                                                        let label_rect = egui::Rect::from_min_max(
                                                            egui::pos2(
                                                                rect.left() + 9.0,
                                                                rect.top() + 2.0,
                                                            ),
                                                            egui::pos2(
                                                                if can_close {
                                                                    close_rect.left() - 2.0
                                                                } else {
                                                                    rect.right() - 5.0
                                                                },
                                                                rect.bottom() - 2.0,
                                                            ),
                                                        );
                                                        let label_color = if active {
                                                            visuals.text_color()
                                                        } else {
                                                            visuals.weak_text_color()
                                                        };
                                                        if kind == WorkspaceTabKind::Masonry {
                                                            let mark_rect =
                                                                egui::Rect::from_center_size(
                                                                    egui::pos2(
                                                                        rect.left() + 18.0,
                                                                        rect.center().y,
                                                                    ),
                                                                    egui::vec2(14.0, 14.0),
                                                                );
                                                            crate::app::ui::draw_masonry_icon(
                                                                &tabs_ui
                                                                    .painter()
                                                                    .with_clip_rect(label_rect),
                                                                mark_rect,
                                                                label_color,
                                                            );
                                                            tabs_ui
                                                                .painter()
                                                                .with_clip_rect(label_rect)
                                                                .text(
                                                                    egui::pos2(
                                                                        rect.left() + 30.0,
                                                                        rect.center().y,
                                                                    ),
                                                                    egui::Align2::LEFT_CENTER,
                                                                    display_title,
                                                                    egui::FontId::proportional(
                                                                        12.5,
                                                                    ),
                                                                    label_color,
                                                                );
                                                        } else {
                                                            tabs_ui
                                                                .painter()
                                                                .with_clip_rect(label_rect)
                                                                .text(
                                                                    egui::pos2(
                                                                        rect.left() + 9.0,
                                                                        rect.center().y,
                                                                    ),
                                                                    egui::Align2::LEFT_CENTER,
                                                                    format!(
                                                                        "{icon}  {display_title}"
                                                                    ),
                                                                    egui::FontId::proportional(
                                                                        12.5,
                                                                    ),
                                                                    label_color,
                                                                );
                                                        }
                                                        if can_close {
                                                            let close_hovered = response.hovered()
                                                                || ctx
                                                                    .pointer_hover_pos()
                                                                    .is_some_and(|pos| {
                                                                        close_rect.contains(pos)
                                                                    });
                                                            if close_hovered {
                                                                tabs_ui.painter().circle_filled(
                                                                    close_rect.center(),
                                                                    9.5,
                                                                    visuals.widgets.hovered.bg_fill,
                                                                );
                                                                tabs_ui.painter().text(
                                                                    close_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "×",
                                                                    egui::FontId::proportional(
                                                                        17.0,
                                                                    ),
                                                                    visuals.text_color(),
                                                                );
                                                            }
                                                        }
                                                        if response
                                                            .clicked_by(egui::PointerButton::Middle)
                                                        {
                                                            // Middle-click is the browser-style close gesture.
                                                            if can_close {
                                                                close = Some(id);
                                                            }
                                                        } else if response.clicked() {
                                                            if can_close
                                                                && response
                                                                    .interact_pointer_pos()
                                                                    .is_some_and(|pos| {
                                                                        close_rect.contains(pos)
                                                                    })
                                                            {
                                                                close = Some(id);
                                                            } else {
                                                                activate = Some(id);
                                                            }
                                                        }
                                                        if response.drag_started()
                                                            && !self.window_mouse_operation_active
                                                            && response
                                                                .interact_pointer_pos()
                                                                .is_some_and(|pos| {
                                                                    !close_rect.contains(pos)
                                                                })
                                                        {
                                                            self.workspace.dragging_tab_id =
                                                                Some(id);
                                                        }
                                                        if response.hovered()
                                                            && self
                                                                .workspace
                                                                .dragging_tab_id
                                                                .is_some()
                                                        {
                                                            tabs_ui.output_mut(|output| {
                                                                output.cursor_icon =
                                                                    egui::CursorIcon::Grabbing
                                                            });
                                                        }
                                                        tab_rects.push((id, rect));
                                                    }
                                                });
                                            });
                                    },
                                );
                            },
                        );
                    });
            },
        );

        // Reorder while the pointer is down over another tab. The active tab is
        // tracked by its stable ID, so reordering cannot accidentally activate a
        // different workspace.
        if let Some(dragging_id) = self.workspace.dragging_tab_id {
            let pointer_down = ctx.input(|input| input.pointer.primary_down());
            if pointer_down {
                if let Some(pointer) = ctx.pointer_interact_pos()
                    && let Some((target_id, target_rect)) = tab_rects
                        .iter()
                        .find(|(id, rect)| *id != dragging_id && rect.contains(pointer))
                {
                    let before = pointer.x < target_rect.center().x;
                    self.move_workspace_tab_before(dragging_id, *target_id, before);
                }
            } else {
                self.workspace.dragging_tab_id = None;
            }
        }

        if let Some(id) = close {
            self.close_workspace_tab(id);
        } else if let Some(id) = activate {
            self.activate_workspace_tab(id);
        }
    }

    pub(super) fn move_workspace_tab_before(
        &mut self,
        moving_id: u64,
        target_id: u64,
        before: bool,
    ) {
        let Some(from) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.id == moving_id)
        else {
            return;
        };
        let Some(target) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.id == target_id)
        else {
            return;
        };
        let active_id = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| tab.id);
        let tab = self.workspace.tabs.remove(from);
        let target_after_remove = self
            .workspace
            .tabs
            .iter()
            .position(|candidate| candidate.id == target_id)
            .unwrap_or(target);
        let insert_at = if before {
            target_after_remove
        } else {
            target_after_remove + 1
        };
        self.workspace
            .tabs
            .insert(insert_at.min(self.workspace.tabs.len()), tab);
        if let Some(active_id) = active_id {
            self.workspace.active_index = self
                .workspace
                .tabs
                .iter()
                .position(|candidate| candidate.id == active_id)
                .unwrap_or(0);
        }
        // Retain the moving ID as a drag source across frames.
        self.workspace.dragging_tab_id = Some(moving_id);
    }

    pub(super) fn move_active_workspace_tab(&mut self, delta: isize) {
        let Some(tab) = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| tab.id)
        else {
            return;
        };
        let target_index = self
            .workspace
            .active_index
            .saturating_add_signed(delta)
            .min(self.workspace.tabs.len().saturating_sub(1));
        let Some(target_id) = self.workspace.tabs.get(target_index).map(|tab| tab.id) else {
            return;
        };
        self.move_workspace_tab_before(tab, target_id, delta < 0);
    }
}
