//! Tab activation, closing, ordering, keyboard shortcuts, and tab-strip rendering.

use super::helpers::*;
use super::*;

impl crate::app::types::ViewerApp {
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
    }

    pub(crate) fn activate_workspace_tab(&mut self, id: u64) {
        let Some(target_index) = self.workspace.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if target_index == self.workspace.active_index {
            return;
        }
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
                let index = tab.current_index;
                let directory = tab.directory.clone();
                let zoom = tab.zoom;
                let pan = tab.pan;
                let fit = tab.fit_to_window;
                let zoom_used = tab.zoom_used;
                self.current_directory = directory;
                self.set_image_entries(entries, index.min(entry_count.saturating_sub(1)));
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

    /// Open an entry in a one-image tab. Existing image tabs are focused rather
    /// than duplicated. When opening from Masonry, reuse the most recently used
    /// image tab; never replace the collection-based primary Viewer tab.
    pub(crate) fn open_entry_in_image_tab(&mut self, entry: ImageEntry, activate: bool) {
        let entry_id = entry.get_id();

        // An image that already has a dedicated tab always resolves to that tab.
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

        // A foreground open from an Image tab reuses that tab. From Masonry or
        // Compare, reuse the most recently active Image tab. The primary Viewer
        // remains a collection tab and is never collapsed into a one-image tab.
        let reuse_id = if activate {
            match self.workspace.active_kind() {
                WorkspaceTabKind::Image => self.workspace.active_tab().map(|tab| tab.id),
                WorkspaceTabKind::Masonry | WorkspaceTabKind::Compare => {
                    self.workspace.last_image_tab_id
                }
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Welcome => None,
            }
        } else {
            None
        };

        if let Some(id) = reuse_id {
            if let Some(index) = self
                .workspace
                .tabs
                .iter()
                .position(|tab| tab.id == id && tab.kind == WorkspaceTabKind::Image)
            {
                self.snapshot_active_viewer_tab();
                {
                    let tab = &mut self.workspace.tabs[index];
                    tab.title = entry_display_name(&entry);
                    tab.entries = vec![entry];
                    tab.current_index = 0;
                    tab.directory = entry_parent(&tab.entries[0]);
                    tab.zoom = 1.0;
                    tab.pan = egui::Vec2::ZERO;
                    tab.fit_to_window = true;
                    tab.zoom_used = false;
                }
                self.workspace.active_index = index;
                let entries = self.workspace.tabs[index].entries.clone();
                self.current_directory = self.workspace.tabs[index].directory.clone();
                self.set_image_entries(entries, 0);
                self.workspace.last_image_tab_id = Some(id);
                return;
            }
        }

        self.snapshot_active_viewer_tab();
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(id, entry_display_name(&entry), WorkspaceTabKind::Image);
        tab.directory = entry_parent(&entry);
        tab.entries.push(entry);
        self.workspace.tabs.push(tab);
        let index = self.workspace.tabs.len() - 1;
        self.workspace.last_image_tab_id = Some(id);
        if activate {
            self.workspace.active_index = index;
            let entries = self.workspace.tabs[index].entries.clone();
            self.current_directory = self.workspace.tabs[index].directory.clone();
            self.set_image_entries(entries, 0);
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
        if let Some(target_id) = self.workspace.pending_compare_target_id.take() {
            if let Some(index) = self
                .workspace
                .tabs
                .iter()
                .position(|tab| tab.id == target_id && tab.kind == WorkspaceTabKind::Compare)
            {
                if entries.is_empty() {
                    self.queue_status =
                        Some("Select an image in Masonry to add it to Compare.".into());
                    self.workspace.active_index = index;
                    self.stop_preloading_for_workspace();
                    return;
                }
                let tab = &mut self.workspace.tabs[index];
                let mut known = tab
                    .entries
                    .iter()
                    .map(ImageEntry::get_id)
                    .collect::<HashSet<_>>();
                for entry in entries {
                    if known.insert(entry.get_id()) {
                        tab.entries.push(entry);
                        tab.anchors.push(None);
                        tab.anchors_second.push(None);
                        tab.offsets.push([0.0, 0.0]);
                        tab.pane_zoom.push(1.0);
                        tab.pane_sync.push(true);
                        tab.pane_size.push([1.0, 1.0]);
                    }
                }
                tab.title = format!("Compare ({})", tab.entries.len());
                self.workspace.active_index = index;
                self.stop_preloading_for_workspace();
                return;
            }
        }
        if entries.len() < 2 {
            self.queue_status = Some("Select at least two images to compare.".into());
            return;
        }
        // A comparison workspace owns its own entry list and pane state. It is
        // deliberately independent of ordinary Image tabs: creating, closing,
        // or reordering image tabs must not change this comparison.
        self.snapshot_active_viewer_tab();
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(
            id,
            format!("Compare ({})", entries.len()),
            WorkspaceTabKind::Compare,
        );
        tab.anchors = vec![None; entries.len()];
        tab.anchors_second = vec![None; entries.len()];
        tab.offsets = vec![[0.0, 0.0]; entries.len()];
        tab.pane_zoom = vec![1.0; entries.len()];
        tab.pane_sync = vec![true; entries.len()];
        tab.pane_size = vec![[1.0, 1.0]; entries.len()];
        tab.entries = entries;
        tab.show_anchors = true;
        tab.sync_pan = true;
        tab.sync_zoom = true;
        tab.sync_navigation = false;
        self.workspace.tabs.push(tab);
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
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
        if self.workspace.pending_compare_target_id == Some(id) {
            self.workspace.pending_compare_target_id = None;
        }
        self.workspace.tabs.remove(index);
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

    /// Whether the workspace tab strip is actually rendered in the current
    /// viewport. Overlay placement uses the same decision so fullscreen controls
    /// never reserve space for a row that is hidden with the sole Viewer tab.
    pub(crate) fn workspace_tab_strip_visible(&self, ctx: &egui::Context) -> bool {
        let only_primary_viewer = self.workspace.tabs.len() == 1
            && self.workspace.tabs[0].kind == WorkspaceTabKind::Viewer;
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        !only_primary_viewer && (!fullscreen || self.overlay_visible)
    }

    /// Whether the navigation hamburger is hosted by the leading edge of the
    /// workspace tab strip instead of by the floating navigation toolbar.
    pub(crate) fn workspace_tab_strip_hosts_hamburger(&self, ctx: &egui::Context) -> bool {
        // The hamburger is part of the tab strip, not an independently floating
        // overlay. Its visibility therefore follows the exact same predicate as
        // the strip itself (including the sole-Viewer and fullscreen cases).
        if !self.workspace_tab_strip_visible(ctx) {
            return false;
        }
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        fullscreen
            || (self.settings_manager.get().show_titlebar
                && !crate::app::ui::uses_desktop_menu_bar(ctx))
    }

    pub(crate) fn render_workspace_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.handle_workspace_tab_shortcuts(ctx);
        // In fullscreen the strip is a real CentralPanel row, not a floating
        // overlay. This reserves its height so Masonry, Compare, New Tab, and the
        // image viewport all begin below it instead of being covered by the tabs.
        // When only the pinned Viewer remains, hide the entire strip, including +.
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
        let open_welcome =
            ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::T));
        if open_welcome {
            self.open_new_welcome_tab();
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
        let descriptors = self
            .workspace
            .tabs
            .iter()
            .map(|tab| {
                (
                    tab.id,
                    tab.title.clone(),
                    tab.kind,
                    Some(tab.id) == active_id,
                )
            })
            .collect::<Vec<_>>();
        let show_tab_strip_hamburger = self.workspace_tab_strip_hosts_hamburger(ctx);
        let mut activate = None;
        let mut close = None;
        let mut open_welcome = false;
        let mut tab_rects: Vec<(u64, egui::Rect)> = Vec::with_capacity(descriptors.len());

        let profile = crate::ui_theme::current_look_profile(ctx);
        let tab_rounding = if profile == "win98" {
            egui::CornerRadius::ZERO
        } else if profile == "cosmic" || profile == "gnome" {
            egui::CornerRadius::same(9)
        } else {
            egui::CornerRadius::same(7)
        };
        let strip_height = 48.0;
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
                        top: 1,
                        bottom: 1,
                    })
                    .show(slot, |body| {
                        let body_width = (strip_width - 12.0).max(0.0);
                        body.set_min_width(body_width);
                        body.set_max_width(body_width);
                        body.horizontal(|row| {
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
                            let tab_viewport_width = (body_width - hamburger_width).max(0.0);
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
                                                    let can_close =
                                                        kind != WorkspaceTabKind::Viewer || id != 1;
                                                    // Reserve a predictable close-button gutter and clip the
                                                    // label to its own tab, even for long folder/image titles.
                                                    let label_width =
                                                        display_title.chars().count() as f32 * 6.7
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
                                                            egui::Stroke::new(0.5_f32, stroke.color)
                                                        },
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    let close_rect = egui::Rect::from_center_size(
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
                                                                egui::FontId::proportional(12.5),
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
                                                                format!("{icon}  {display_title}"),
                                                                egui::FontId::proportional(12.5),
                                                                label_color,
                                                            );
                                                    }
                                                    if can_close {
                                                        let close_hovered = response.hovered()
                                                            || ctx.pointer_hover_pos().is_some_and(
                                                                |pos| close_rect.contains(pos),
                                                            );
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
                                                                egui::FontId::proportional(17.0),
                                                                visuals.text_color(),
                                                            );
                                                        }
                                                    }
                                                    if response.clicked() {
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
                                                        && response
                                                            .interact_pointer_pos()
                                                            .is_some_and(|pos| {
                                                                !close_rect.contains(pos)
                                                            })
                                                    {
                                                        self.workspace.dragging_tab_id = Some(id);
                                                    }
                                                    if response.hovered()
                                                        && self.workspace.dragging_tab_id.is_some()
                                                    {
                                                        tabs_ui.output_mut(|output| {
                                                            output.cursor_icon =
                                                                egui::CursorIcon::Grabbing
                                                        });
                                                    }
                                                    tab_rects.push((id, rect));
                                                }

                                                // Keep + in the same scrollable sequence
                                                // as the tabs so it is always the final tab action.
                                                let (plus_rect, plus_response) = tabs_ui
                                                    .allocate_exact_size(
                                                        egui::vec2(32.0, 32.0),
                                                        egui::Sense::click(),
                                                    );
                                                let plus_fill = if plus_response.hovered() {
                                                    tabs_ui.visuals().widgets.hovered.bg_fill
                                                } else {
                                                    egui::Color32::TRANSPARENT
                                                };
                                                tabs_ui.painter().rect_filled(
                                                    plus_rect,
                                                    tab_rounding,
                                                    plus_fill,
                                                );
                                                tabs_ui.painter().text(
                                                    plus_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "+",
                                                    egui::FontId::proportional(23.0),
                                                    tabs_ui.visuals().text_color(),
                                                );
                                                if plus_response.clicked() {
                                                    open_welcome = true;
                                                }
                                                plus_response.on_hover_text("New tab");
                                            });
                                        });
                                },
                            );
                        });
                    });
            },
        );

        // Reorder while the pointer is down over another tab. The active tab is
        // tracked by its stable ID, so reordering cannot accidentally activate a
        // different workspace.
        if let Some(dragging_id) = self.workspace.dragging_tab_id {
            let pointer_down = ctx.input(|input| input.pointer.primary_down());
            if pointer_down {
                if let Some(pointer) = ctx.pointer_interact_pos() {
                    if let Some((target_id, target_rect)) = tab_rects
                        .iter()
                        .find(|(id, rect)| *id != dragging_id && rect.contains(pointer))
                    {
                        let before = pointer.x < target_rect.center().x;
                        self.move_workspace_tab_before(dragging_id, *target_id, before);
                    }
                }
            } else {
                self.workspace.dragging_tab_id = None;
            }
        }

        if let Some(id) = close {
            self.close_workspace_tab(id);
        } else if let Some(id) = activate {
            self.activate_workspace_tab(id);
        } else if open_welcome {
            self.open_new_welcome_tab();
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
