//! Comparison workspace image loading, synchronized panes, anchors, and rendering.

use super::*;

#[derive(Clone, Copy)]
enum ComparePaneContextAction {
    SetAnchor(usize),
    BeginAnchor(usize),
    MoveLeft,
    MoveRight,
    MoveFirst,
    MoveLast,
    AlignPanZoom,
    AlignRotation,
    ResetView,
    ToggleSync,
    ToggleAnchors,
    ClearAnchors,
    ReplaceImage,
    OpenNewSplit,
    CloseSplit,
}

impl crate::app::types::ViewerApp {
    /// Drop comparison tile textures when the global filtering configuration
    /// changes. VirtualTexture keeps sampler state inside each resident tile,
    /// so changing the primary viewer's filter alone cannot update those tiles.
    pub(crate) fn invalidate_compare_textures(&mut self) {
        self.workspace.compare_images.clear();
        self.workspace.compare_pending.clear();
        self.workspace.compare_failed.clear();
        // Dropping receivers makes in-flight workers abandon their result when
        // they finish instead of reintroducing textures built with the old filter.
        self.workspace.compare_receivers.clear();
    }

    pub(super) fn collect_compare_images(&mut self) {
        let mut waiting = Vec::new();
        let mut completed = Vec::new();
        let mut worker_disconnected = false;
        for receiver in self.workspace.compare_receivers.drain(..) {
            match receiver.try_recv() {
                Ok(result) => completed.push(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => waiting.push(receiver),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    worker_disconnected = true;
                }
            }
        }
        self.workspace.compare_receivers = waiting;
        if worker_disconnected {
            // Compare decoding is intentionally serialized, so a disconnected
            // receiver corresponds to the sole pending job. Clear it to avoid
            // blocking every later pane forever after a worker panic.
            if let Some(key) = self.workspace.compare_pending.iter().next().cloned() {
                self.workspace.compare_pending.remove(&key);
                self.workspace.compare_failed.insert(key);
            }
        }
        for (key, result) in completed {
            self.workspace.compare_pending.remove(&key);
            let still_referenced = self.workspace.tabs.iter().any(|tab| {
                tab.kind == WorkspaceTabKind::Compare
                    && tab.entries.iter().any(|entry| entry.get_id() == key)
            });
            if !still_referenced {
                // The user removed this pane while its decoder was running.
                // Drop the result instead of retaining an orphaned full image.
                continue;
            }
            match result {
                Ok(image) => {
                    // Keep the cache bounded. Each comparison image owns CPU-side
                    // tiles and lazily uploaded GPU textures, so evicting a pane
                    // releases both instead of retaining a second full-size copy.
                    if self.workspace.compare_images.len() >= 12
                        && let Some(evict) = self.workspace.compare_images.keys().next().cloned()
                    {
                        self.workspace.compare_images.remove(&evict);
                    }
                    self.workspace.compare_images.insert(key, image);
                }
                Err(error) => {
                    self.workspace.compare_failed.insert(key);
                    self.queue_status =
                        Some(format!("Could not prepare comparison image: {error}"));
                }
            }
        }
    }

    pub(super) fn request_compare_image(
        &mut self,
        entry: ImageEntry,
        key: String,
        ctx: &egui::Context,
    ) {
        if self.workspace.compare_images.contains_key(&key)
            || self.workspace.compare_failed.contains(&key)
            || self.workspace.compare_pending.contains(&key)
        {
            return;
        }
        // Full-resolution decoders can temporarily allocate a large RGBA buffer.
        // Load one comparison source at a time so 2–4 high-resolution panes do
        // not multiply peak RAM usage. Other visible panes are requested on the
        // next frame after collect_compare_images() completes the current job.
        if !self.workspace.compare_pending.is_empty() {
            return;
        }
        self.workspace.compare_pending.insert(key.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.workspace.compare_receivers.push(rx);
        let repaint = ctx.clone();
        let settings = self.settings_manager.get().clone();
        // Compare panes share the same virtual-texture renderer as the main
        // viewer, but use a conservative tile cap. Very large 8K/16K tiles can
        // exceed common GPU texture limits or consume hundreds of MiB per tile.
        // The main viewer's configured tile size is not changed.
        let tile_size = settings.tile_size.clamp(128, 2048);
        let virtual_texture_threshold = settings.virtual_texture_threshold;
        rayon::spawn(move || {
            let result = (|| -> Result<VirtualTexture, String> {
                // Use the same entry loader as the primary viewer. This preserves
                // RAW embedded-preview behavior, content detection, archive
                // handling, AVIF/HEIC/JXL/SVG decoders, and GIF first-frame policy.
                let loaded = crate::app::loading::load_entry_content(
                    entry,
                    virtual_texture_threshold,
                    false,
                )?;

                let decoded = match loaded {
                    LoadedImage::Static(image) => image,
                    LoadedImage::Animated(animation, _) => {
                        let frame = animation.get_current_frame_ref().ok_or_else(|| {
                            "Animated image contains no displayable frame.".to_owned()
                        })?;
                        crate::imaging::core::DecodedImage {
                            width: frame.width(),
                            height: frame.height(),
                            data: frame.as_raw().clone(),
                        }
                    }
                    LoadedImage::VirtualPending(bytes, width, height) => {
                        // The normal viewer routes large images through this
                        // VirtualPending branch using the configured threshold.
                        // Compare used to force threshold=0 and then reject
                        // sources above 50 MP, so its virtual-tile renderer was
                        // never reached for the very images that needed it.
                        if width == 0 || height == 0 {
                            return Err("Comparison image has invalid dimensions.".to_owned());
                        }
                        crate::decoder::decode_bytes(&bytes)
                            .map_err(|error| format!("Could not decode high-resolution comparison image ({width}×{height}): {error}"))?
                    }
                };

                if decoded.width == 0 || decoded.height == 0 {
                    return Err("Comparison image has invalid decoded dimensions.".to_owned());
                }
                let expected_bytes = (decoded.width as usize)
                    .checked_mul(decoded.height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .ok_or_else(|| {
                        "Comparison image decoded size overflows address space.".to_owned()
                    })?;
                if decoded.data.len() != expected_bytes {
                    return Err(format!(
                        "Comparison decoder returned an incomplete pixel buffer ({} bytes; expected {expected_bytes}).",
                        decoded.data.len(),
                    ));
                }

                // Build the virtual tile map off the UI thread. Pixel extraction
                // and GPU upload then happen asynchronously only for tiles that
                // become visible in a pane, with the shared viewer renderer and
                // no fixed 1600px preview cap.
                let mut image = VirtualTexture::new(decoded, tile_size);
                image.prepare();
                Ok(image)
            })();
            let _ = tx.send((key, result));
            repaint.request_repaint();
        });
    }

    pub(crate) fn compare_entry_from_context(&mut self, entry: ImageEntry) {
        let active_compare_id = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .filter(|tab| tab.kind == WorkspaceTabKind::Compare)
            .map(|tab| tab.id);
        let pending_compare_id = self.workspace.pending_compare_target_id;
        if let Some(compare_tab_id) = active_compare_id.or(pending_compare_id) {
            let added = self.add_entries_to_compare(compare_tab_id, vec![entry]);
            self.queue_status = Some(if added == 0 {
                "That image is already present in Compare.".into()
            } else {
                "Added image to Compare.".into()
            });
            if pending_compare_id == Some(compare_tab_id) {
                self.workspace.pending_compare_target_id = None;
                self.activate_workspace_tab(compare_tab_id);
            }
        } else {
            self.open_compare_workspace(vec![entry]);
        }
    }

    /// Send Masonry selections to the comparison workspace that opened Masonry,
    /// or create a new Compare workspace when there is no pending destination.
    pub(crate) fn compare_entries_from_masonry(&mut self, entries: Vec<ImageEntry>) {
        if let Some(compare_tab_id) = self.workspace.pending_compare_target_id.take() {
            let added = self.add_entries_to_compare(compare_tab_id, entries);
            self.queue_status = Some(if added == 0 {
                "No new images were added to Compare.".into()
            } else {
                format!("Added {added} image(s) to Compare.")
            });
            if self.workspace.tabs.iter().any(|tab| {
                tab.id == compare_tab_id && tab.kind == WorkspaceTabKind::Compare
            }) {
                self.activate_workspace_tab(compare_tab_id);
            }
        } else {
            self.open_compare_workspace(entries);
        }
    }

    pub(super) fn return_to_masonry_from_compare(&mut self, compare_tab_id: u64) {
        self.workspace.pending_compare_target_id = Some(compare_tab_id);
        if let Some(id) = self
            .workspace
            .tabs
            .iter()
            .rev()
            .find(|tab| tab.kind == WorkspaceTabKind::Masonry)
            .map(|tab| tab.id)
        {
            self.activate_workspace_tab(id);
        } else {
            self.open_local_masonry();
        }
    }

    pub(super) fn render_compare_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.collect_compare_images();
        let Some(tab_index) = (self.workspace.active_index < self.workspace.tabs.len())
            .then_some(self.workspace.active_index)
        else {
            return;
        };
        let tab_id = self.workspace.tabs[tab_index].id;
        let compare_central_rect = ui
            .available_rect_before_wrap()
            .intersect(ui.clip_rect())
            .intersect(self.main_view_rect(ctx));
        let mut close = false;
        let mut remove_pane: Option<usize> = None;
        let mut align_anchors_requested = false;
        let mut rotate_anchors_requested = false;
        let mut add_images_requested = false;
        let mut open_masonry_from_compare = false;
        let mut pane_context_action: Option<(usize, ComparePaneContextAction)> = None;
        let mut replace_pane_request: Option<usize> = None;
        let mut drop_pane_request: Option<(usize, std::path::PathBuf)> = None;
        {
            let tab = &mut self.workspace.tabs[tab_index];
            ui.horizontal_wrapped(|ui| {
                ui.heading("Compare");
                if crate::app::ui::masonry_button(ui, "Masonry", true, 92.0, false, true)
                    .on_hover_text("Open the Masonry browser while keeping this comparison available")
                    .clicked()
                {
                    open_masonry_from_compare = true;
                }
                ui.separator();
                ui.label("Sync:");
                ui.checkbox(&mut tab.sync_pan, "Pan");
                ui.checkbox(&mut tab.sync_zoom, "Zoom");
                ui.checkbox(&mut tab.show_anchors, "Show anchors");
                ui.separator();
                ui.selectable_value(&mut tab.compare_rows, false, "Columns");
                ui.selectable_value(&mut tab.compare_rows, true, "Rows");
                if ui.button("Add Images…").on_hover_text("Choose one or more images to fill empty panes or add comparison panes.").clicked() {
                    add_images_requested = true;
                }
                if ui.button("Align anchors").on_hover_text(
                    "Use the two corresponding points in each pane to calculate matching zoom and pan.",
                ).clicked() {
                    align_anchors_requested = true;
                }
                if ui.button("Rotate to align anchors").on_hover_text(
                    "Rotate each image so the line between its two anchors has the same angle as the reference image.",
                ).clicked() {
                    rotate_anchors_requested = true;
                }
                if ui.button("Clear all anchors").clicked() {
                    tab.anchors.fill(None);
                    tab.anchors_second.fill(None);
                    tab.offsets.fill([0.0, 0.0]);
                    tab.anchor_editing = None;
                    tab.anchor_editing_point = 0;
                    tab.pane_rotation.fill(0.0);
                }
                if ui.button("Reset view").clicked() {
                    tab.offsets.fill([0.0, 0.0]);
                    tab.pane_zoom.fill(1.0);
                    tab.pane_rotation.fill(0.0);
                    // Reset view deliberately preserves both anchor sets.
                    tab.anchor_editing = None;
                    tab.anchor_context_position = None;
                    tab.anchor_editing_point = 0;
                }
                if ui.button("Close comparison").clicked() {
                    close = true;
                }
            });
        }

        let entries = self.workspace.tabs[tab_index].entries.clone();
        let actual_entry_count = entries.len();
        // Keep two implicit empty panes in a new comparison, and preserve any
        // additional empty panes explicitly opened from a pane context menu.
            let explicit_empty_panes = self.workspace.tabs[tab_index].compare_empty_panes;
            // Two base panes are always present. `compare_empty_panes` counts
            // only additional panes explicitly opened by the user.
            let pane_count = actual_entry_count.max(2) + explicit_empty_panes;
            let compare_rows = self.workspace.tabs[tab_index].compare_rows;
            if pane_count > 4 {
                ui.label(
                    egui::RichText::new(format!(
                        "{} panes are open. Four is recommended for smoother comparison.",
                        pane_count
                    ))
                    .weak(),
                );
            }
            if self.workspace.tabs[tab_index].pane_zoom.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .pane_zoom
                    .resize(pane_count, 1.0);
            }
            if self.workspace.tabs[tab_index].pane_rotation.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .pane_rotation
                    .resize(pane_count, 0.0);
            }
            if self.workspace.tabs[tab_index].pane_sync.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .pane_sync
                    .resize(pane_count, true);
            }
            if self.workspace.tabs[tab_index].pane_size.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .pane_size
                    .resize(pane_count, [1.0, 1.0]);
            }
            if self.workspace.tabs[tab_index].offsets.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .offsets
                    .resize(pane_count, [0.0, 0.0]);
            }
            if self.workspace.tabs[tab_index].anchors.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .anchors
                    .resize(pane_count, None);
            }
            if self.workspace.tabs[tab_index].anchors_second.len() != pane_count {
                self.workspace.tabs[tab_index]
                    .anchors_second
                    .resize(pane_count, None);
            }
            let mut placed_anchor: Option<(usize, usize, [f32; 2])> = None;
            let mut pan_delta: Option<(usize, [f32; 2])> = None;
            let mut resize_delta: Option<(usize, [f32; 2])> = None;
            // Per-pane image-to-viewport scale factors are used to calculate
            // normalized alignment offsets when anchor points are placed.
            let mut anchor_metrics: Vec<Option<[f32; 2]>> = vec![None; pane_count];
            let mut anchor_image_sizes: Vec<Option<[f32; 2]>> = vec![None; pane_count];
            let mut anchor_source_sizes: Vec<Option<[f32; 2]>> = vec![None; pane_count];
            let mut pan_metrics: Vec<Option<[f32; 2]>> = vec![None; pane_count];

            // Measure the central view's clipped width. `available_width()` can
            // shrink after the toolbar row and produce an undersized comparison grid.
            let compare_viewport_rect = ui
                .available_rect_before_wrap()
                .intersect(ui.clip_rect())
                .intersect(compare_central_rect);
            let compare_viewport_width = compare_viewport_rect.width().max(1.0);
            let compare_viewport_height = compare_viewport_rect.height().max(1.0);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(compare_viewport_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |compare_ui| {
                    {
                        let scroll_style = &mut compare_ui.spacing_mut().scroll;
                        scroll_style.floating = false;
                        scroll_style.bar_width = 14.0;
                        scroll_style.handle_min_length = 32.0;
                        scroll_style.bar_inner_margin = 4.0;
                        scroll_style.bar_outer_margin = 1.0;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt(("compare_scroll", tab_id))
                        .auto_shrink([false, false])
                        .max_height(compare_viewport_height)
                        .show(compare_ui, |ui| {
                            let viewport_width = ui.available_width().min(compare_viewport_width).max(1.0);
                            ui.set_min_width(viewport_width);
                            ui.set_max_width(viewport_width);
                            ui.vertical(|ui| {
                    let content_width = viewport_width.max(1.0);
                    ui.set_min_width(content_width);
                    ui.set_max_width(content_width);
                    let columns = if compare_rows { 1 } else { pane_count.max(1) };
                    // Panes touch edge-to-edge. Their 1px borders are the only
                    // separation; the Grid contributes no gutters or padding.
                    let card_width = (content_width / columns as f32).max(1.0);
                    let base_width = (card_width - 2.0).max(1.0);
                    let controls_height = if base_width < 360.0 {
                        156.0
                    } else if base_width < 520.0 {
                        128.0
                    } else {
                        104.0
                    };
                    let base_height = if compare_rows {
                        // In Rows mode each pane receives an equal share of the
                        // visible compare viewport. Image height is reduced by its
                        // own controls, so the complete row cannot grow past that share.
                        let row_height = compare_viewport_height / pane_count as f32;
                        (row_height - controls_height).clamp(1.0, row_height.max(1.0))
                    } else {
                        (compare_viewport_height - controls_height).clamp(120.0, 1400.0)
                    };

                    egui::Grid::new(("compare_grid", tab_id))
                        .num_columns(columns)
                        .spacing([0.0, 0.0])
                        .show(ui, |ui| {
                            for (index, entry) in entries.iter().enumerate() {
                                let pane_border = ui.visuals().widgets.noninteractive.bg_stroke;
                                egui::Frame::new()
                                    .fill(ui.visuals().extreme_bg_color)
                                    .stroke(egui::Stroke::new(1.0_f32, pane_border.color))
                                    .inner_margin(egui::Margin::ZERO)
                                    .corner_radius(egui::CornerRadius::ZERO)
                                    .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                    ui.set_min_width(base_width);
                                    ui.set_max_width(base_width);
                                    let key = entry.get_id();
                                    let image_loaded = self.workspace.compare_images.contains_key(&key);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.small_button("×").on_hover_text("Remove this image from the comparison").clicked() {
                                            remove_pane = Some(index);
                                        }
                                        let anchor_editing = self.workspace.tabs[tab_index].anchor_editing;
                                        let editing_first = anchor_editing == Some(index)
                                            && self.workspace.tabs[tab_index].anchor_editing_point == 0;
                                        let editing_second = anchor_editing == Some(index)
                                            && self.workspace.tabs[tab_index].anchor_editing_point == 1;
                                        let can_place_anchor = image_loaded;
                                        if ui.add_enabled(
                                            can_place_anchor,
                                            egui::Button::new(if editing_first { "Click for anchor 1…" } else { "Set anchor 1" }),
                                        ).on_hover_text("Place the first corresponding point on this image.").clicked() {
                                            let tab = &mut self.workspace.tabs[tab_index];
                                            tab.anchor_editing = Some(index);
                                            tab.anchor_editing_point = 0;
                                        }
                                        if ui.add_enabled(
                                            can_place_anchor,
                                            egui::Button::new(if editing_second { "Click for anchor 2…" } else { "Set anchor 2" }),
                                        ).on_hover_text("Place the second corresponding point to calculate relative zoom as well as pan.").clicked() {
                                            let tab = &mut self.workspace.tabs[tab_index];
                                            tab.anchor_editing = Some(index);
                                            tab.anchor_editing_point = 1;
                                        }
                                        let first_anchor_set = self.workspace.tabs[tab_index].anchors[index].is_some();
                                        let second_anchor_set = self.workspace.tabs[tab_index].anchors_second[index].is_some();
                                        let has_anchors = first_anchor_set || second_anchor_set;
                                        ui.label(egui::RichText::new(format!(
                                            "{} {}",
                                            if first_anchor_set { "●" } else { "○" },
                                            if second_anchor_set { "●" } else { "○" },
                                        )).small().weak()).on_hover_text("Anchor 1 / Anchor 2 status");
                                        if ui.add_enabled(has_anchors, egui::Button::new("Clear")).clicked() {
                                            let tab = &mut self.workspace.tabs[tab_index];
                                            tab.anchors[index] = None;
                                            tab.anchors_second[index] = None;
                                            tab.offsets[index] = [0.0, 0.0];
                                            tab.anchor_editing = None;
                                        }
                                    });

                                    let pane_factor = self.workspace.tabs[tab_index]
                                        .pane_size.get(index).copied().unwrap_or([1.0, 1.0]);
                                    let pane_width = base_width;
                                    let pane_height = if compare_rows {
                                        base_height
                                    } else {
                                        (base_height * pane_factor[1]).clamp(64.0, 1200.0)
                                    };
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(pane_width, pane_height),
                                        egui::Sense::click_and_drag(),
                                    );
                                    let compare_drag_payload_id = egui::Id::new("jpeg_viewer_compare_drag_entry");
                                    if response.hovered()
                                        && ctx.input(|input| input.pointer.any_released())
                                        && let Some(path) = ctx.data(|data| data.get_temp::<std::path::PathBuf>(compare_drag_payload_id))
                                    {
                                        drop_pane_request = Some((index, path));
                                    }
                                    let tab_state = &self.workspace.tabs[tab_index];
                                    let aligned_anchor_count = tab_state
                                        .anchors
                                        .iter()
                                        .zip(tab_state.anchors_second.iter())
                                        .filter(|(first, second)| first.is_some() && second.is_some())
                                        .count();
                                    let can_align_anchors = aligned_anchor_count >= 2;
                                    let pane_participates = tab_state
                                        .pane_sync
                                        .get(index)
                                        .copied()
                                        .unwrap_or(true);
                                    response.clone().context_menu(|menu| {
                                        menu.label(format!("Split {}", index + 1));
                                        menu.separator();
                                        if menu.button("Set anchor 1 here").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::SetAnchor(0)));
                                            menu.close();
                                        }
                                        if menu.button("Set anchor 2 here").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::SetAnchor(1)));
                                            menu.close();
                                        }
                                        if menu.button("Choose anchor 1 by clicking…").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::BeginAnchor(0)));
                                            menu.close();
                                        }
                                        if menu.button("Choose anchor 2 by clicking…").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::BeginAnchor(1)));
                                            menu.close();
                                        }
                                        if menu.add_enabled(
                                            can_align_anchors,
                                            egui::Button::new("Align pan + zoom from anchors"),
                                        ).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::AlignPanZoom));
                                            menu.close();
                                        }
                                        if menu.add_enabled(
                                            can_align_anchors,
                                            egui::Button::new("Align rotation from anchors"),
                                        ).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::AlignRotation));
                                            menu.close();
                                        }
                                        menu.separator();
                                        if menu.button("Reset view").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::ResetView));
                                            menu.close();
                                        }
                                        if menu.button("Clear anchors").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::ClearAnchors));
                                            menu.close();
                                        }
                                        if menu.button(if tab_state.show_anchors { "Hide anchors" } else { "Show anchors" }).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::ToggleAnchors));
                                            menu.close();
                                        }
                                        if menu.button(if pane_participates { "Desync this split" } else { "Sync this split" }).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::ToggleSync));
                                            menu.close();
                                        }
                                        menu.separator();
                                        if menu.button("Open another image in this split…").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::ReplaceImage));
                                            menu.close();
                                        }
                                        menu.separator();
                                        if menu.add_enabled(index > 0, egui::Button::new("Move split earlier")).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::MoveLeft));
                                            menu.close();
                                        }
                                        if menu.add_enabled(index + 1 < tab_state.entries.len(), egui::Button::new("Move split later")).clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::MoveRight));
                                            menu.close();
                                        }
                                        if menu.button("Move split to first").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::MoveFirst));
                                            menu.close();
                                        }
                                        if menu.button("Move split to last").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::MoveLast));
                                            menu.close();
                                        }
                                        if menu.button("Open new split").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::OpenNewSplit));
                                            menu.close();
                                        }
                                        if menu.button("Close split").clicked() {
                                            pane_context_action = Some((index, ComparePaneContextAction::CloseSplit));
                                            menu.close();
                                        }
                                    });
                                    // Compare panes use the same configurable zoom keys and
                                    // mouse-wheel modifier as the primary Viewer. Only the pane
                                    // under the pointer receives the input.
                                    if response.drag_stopped_by(egui::PointerButton::Secondary) {
                                        self.zoom_drag_start_pos = None;
                                    }
                                    if response.drag_started_by(egui::PointerButton::Secondary) {
                                        self.zoom_drag_start_pos = response.interact_pointer_pos();
                                        self.zoom_drag_start_zoom = self.workspace.tabs[tab_index].pane_zoom[index];
                                    }
                                    let right_drag_zoom = if response.dragged_by(egui::PointerButton::Secondary) {
                                        self.zoom_drag_start_pos
                                            .zip(response.interact_pointer_pos())
                                            .map(|(start, current)| {
                                                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                                let step = self.settings_manager.get().zoom_step_mouse_right_button.clamp(1.05, 2.0);
                                                let sensitivity = step.ln() / 40.0;
                                                (self.zoom_drag_start_zoom * (-(current.y - start.y) * sensitivity).exp())
                                                    .clamp(0.01, 100.0)
                                            })
                                    } else {
                                        None
                                    };
                                    let pinch_zoom = ctx.input(|input| {
                                        let delta = input.zoom_delta();
                                        (delta.is_finite() && (delta - 1.0).abs() > 0.001)
                                            .then_some(delta)
                                    });
                                    if (response.hovered() || right_drag_zoom.is_some()) && !ctx.wants_keyboard_input() {
                                        let zoom_command = ctx.input(|input| {
                                            [
                                                crate::input::shortcuts::ViewerCommand::ZoomIn,
                                                crate::input::shortcuts::ViewerCommand::ZoomOut,
                                                crate::input::shortcuts::ViewerCommand::ResetZoom,
                                                crate::input::shortcuts::ViewerCommand::MakeFit,
                                            ]
                                            .into_iter()
                                            .find(|command| self.input_bindings.keyboard
                                                .get(command)
                                                .is_some_and(|bindings| bindings.iter().any(|binding| binding.matches(input))))
                                        });
                                        let wheel_zoom = ctx.input(|input| {
                                            let delta = input.raw_scroll_delta.y;
                                            let zooming = (!self.b_ctrl_invert && input.modifiers.ctrl)
                                                || (self.b_ctrl_invert && !input.modifiers.ctrl);
                                            let step = self.settings_manager.get().zoom_step_mouse_wheel.clamp(1.05, 2.0);
                                            (zooming && delta.abs() > 0.0).then_some(if delta > 0.0 { step } else { 1.0 / step })
                                        });
                                        let custom_mouse_zoom = ctx.input(|input| {
                                            [crate::input::shortcuts::ViewerCommand::ZoomIn,
                                             crate::input::shortcuts::ViewerCommand::ZoomOut]
                                                .into_iter()
                                                .find(|command| self.input_bindings.mouse
                                                    .get(command)
                                                    .is_some_and(|bindings| bindings.iter().any(|binding| binding.matches(input))))
                                        });
                                        let previous = self.workspace.tabs[tab_index].pane_zoom[index].max(0.01);
                                        let target = right_drag_zoom
                                            .or_else(|| pinch_zoom.map(|factor| (previous * factor).clamp(0.01, 100.0)))
                                            .or_else(|| wheel_zoom.map(|factor| (previous * factor).clamp(0.01, 100.0)))
                                            .or_else(|| match zoom_command.or(custom_mouse_zoom) {
                                                Some(crate::input::shortcuts::ViewerCommand::ZoomIn) => Some((previous * self.settings_manager.get().zoom_step_keyboard.clamp(1.05, 2.0)).clamp(0.01, 100.0)),
                                                Some(crate::input::shortcuts::ViewerCommand::ZoomOut) => Some((previous / self.settings_manager.get().zoom_step_keyboard.clamp(1.05, 2.0)).clamp(0.01, 100.0)),
                                                Some(crate::input::shortcuts::ViewerCommand::ResetZoom)
                                                | Some(crate::input::shortcuts::ViewerCommand::MakeFit) => Some(1.0),
                                                _ => None,
                                            });
                                        if let Some(next) = target {
                                            let ratio = next / previous;
                                            let tab = &mut self.workspace.tabs[tab_index];
                                            tab.pane_zoom[index] = next;
                                            if tab.sync_zoom && tab.pane_sync.get(index).copied().unwrap_or(true) {
                                                let sync_targets = tab.pane_sync.clone();
                                                for (pane_index, zoom) in tab.pane_zoom.iter_mut().enumerate() {
                                                    if pane_index != index && sync_targets.get(pane_index).copied().unwrap_or(true) {
                                                        *zoom = (*zoom * ratio).clamp(0.01, 100.0);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let pane_clip = rect.intersect(ui.clip_rect());
                                    let pane_painter = ui.painter().with_clip_rect(pane_clip);
                                    pane_painter.rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
                                    let image_failed = self.workspace.compare_failed.contains(&key);
                                    if !image_loaded && !image_failed && rect.intersects(ui.clip_rect()) {
                                        self.request_compare_image(entry.clone(), key.clone(), ctx);
                                    }

                                    if image_loaded {
                                        let (image_width, image_height) = self.workspace.compare_images
                                            .get(&key)
                                            .map(VirtualTexture::dimensions)
                                            .unwrap_or((1, 1));
                                        let size = egui::vec2(image_width as f32, image_height as f32);
                                        let rotation = self.workspace.tabs[tab_index].pane_rotation.get(index).copied().unwrap_or(0.0);
                                        let (sin, cos) = rotation.sin_cos();
                                        let rotated_size = egui::vec2(
                                            size.x * cos.abs() + size.y * sin.abs(),
                                            size.x * sin.abs() + size.y * cos.abs(),
                                        );
                                        let fit_scale = (pane_width / rotated_size.x.max(1.0))
                                            .min(pane_height / rotated_size.y.max(1.0));
                                        let zoom = self.workspace.tabs[tab_index].pane_zoom[index].clamp(0.01, 100.0);
                                        let image_zoom = fit_scale * zoom;
                                        let offset = self.workspace.tabs[tab_index].offsets
                                            .get(index).copied().unwrap_or([0.0, 0.0]);
                                        // Offsets are normalized in the rotated image bounds so
                                        // synchronized panning remains screen-aligned after rotation.
                                        let image_pan = egui::vec2(offset[0] * rotated_size.x, offset[1] * rotated_size.y);
                                        let image_center = rect.center() + image_pan * image_zoom;
                                        let image_rect = egui::Rect::from_center_size(image_center, rotated_size * image_zoom);
                                        let scale_x = image_rect.width() / rect.width().max(1.0);
                                        let scale_y = image_rect.height() / rect.height().max(1.0);
                                        anchor_metrics[index] = Some([scale_x, scale_y]);
                                        anchor_image_sizes[index] = Some([image_rect.width().max(1.0), image_rect.height().max(1.0)]);
                                        anchor_source_sizes[index] = Some([size.x.max(1.0), size.y.max(1.0)]);
                                        pan_metrics[index] = anchor_image_sizes[index];
                                        let painter = pane_painter.clone();
                                        let texture_options = self.get_texture_options();
                                        let mipmap_upload_budget =
                                            std::sync::Arc::clone(&self.mipmap_upload_budget);
                                        if let Some(image) = self.workspace.compare_images.get_mut(&key) {
                                            if rotation.abs() < 0.0001 {
                                                image.render(
                                                    ctx,
                                                    &painter,
                                                    image_zoom,
                                                    image_pan,
                                                    rect,
                                                    texture_options,
                                                    mipmap_upload_budget,
                                                );
                                            } else {
                                                image.render_rotated(
                                                    ctx,
                                                    &painter,
                                                    image_zoom,
                                                    image_center,
                                                    rect,
                                                    rotation,
                                                    texture_options,
                                                    mipmap_upload_budget,
                                                );
                                            }
                                        }

                                        let (show_anchors, anchor_pair, editing, editing_point) = {
                                            let tab = &self.workspace.tabs[tab_index];
                                            (
                                                tab.show_anchors,
                                                [
                                                    tab.anchors.get(index).copied().flatten(),
                                                    tab.anchors_second.get(index).copied().flatten(),
                                                ],
                                                tab.anchor_editing == Some(index),
                                                tab.anchor_editing_point,
                                            )
                                        };
                                        if show_anchors {
                                            for (anchor_index, anchor) in anchor_pair.into_iter().enumerate() {
                                                if let Some(anchor) = anchor {
                                                    let source_point = egui::vec2(
                                                        (anchor[0] - 0.5) * size.x,
                                                        (anchor[1] - 0.5) * size.y,
                                                    );
                                                    let rotated_point = egui::vec2(
                                                        source_point.x * cos - source_point.y * sin,
                                                        source_point.x * sin + source_point.y * cos,
                                                    );
                                                    let point = image_center + rotated_point * image_zoom;
                                                    let color = if anchor_index == 0 {
                                                        egui::Color32::YELLOW
                                                    } else {
                                                        egui::Color32::from_rgb(80, 210, 255)
                                                    };
                                                    pane_painter.circle_stroke(
                                                        point, 8.0,
                                                        egui::Stroke::new(2.0_f32, color),
                                                    );
                                                    pane_painter.line_segment(
                                                        [point - egui::vec2(11.0, 0.0), point + egui::vec2(11.0, 0.0)],
                                                        egui::Stroke::new(1.0_f32, color),
                                                    );
                                                    pane_painter.line_segment(
                                                        [point - egui::vec2(0.0, 11.0), point + egui::vec2(0.0, 11.0)],
                                                        egui::Stroke::new(1.0_f32, color),
                                                    );
                                                    pane_painter.text(
                                                        point + egui::vec2(10.0, -10.0),
                                                        egui::Align2::LEFT_BOTTOM,
                                                        if anchor_index == 0 { "1" } else { "2" },
                                                        egui::FontId::proportional(11.0),
                                                        color,
                                                    );
                                                }
                                            }
                                        }

                                        let grip_size = 18.0;
                                        let grip_rect = egui::Rect::from_min_max(
                                            egui::pos2(rect.right() - grip_size, rect.bottom() - grip_size),
                                            rect.max,
                                        );
                                        let resize_response = if compare_rows {
                                            ui.interact(
                                                grip_rect,
                                                ui.make_persistent_id(("compare_resize", tab_id, index)),
                                                egui::Sense::hover(),
                                            )
                                        } else {
                                            ui.interact(
                                                grip_rect,
                                                ui.make_persistent_id(("compare_resize", tab_id, index)),
                                                egui::Sense::drag(),
                                            )
                                        };
                                        if !compare_rows && (resize_response.hovered() || resize_response.dragged()) {
                                            pane_painter.rect_filled(
                                                grip_rect,
                                                3.0,
                                                ui.visuals().widgets.hovered.bg_fill,
                                            );
                                            let stroke = ui.visuals().widgets.hovered.fg_stroke;
                                            for step in 0..3 {
                                                let inset = 4.0 + step as f32 * 4.0;
                                                pane_painter.line_segment(
                                                    [
                                                        egui::pos2(grip_rect.right() - inset, grip_rect.bottom() - 3.0),
                                                        egui::pos2(grip_rect.right() - 3.0, grip_rect.bottom() - inset),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                        }
                                        if !compare_rows && resize_response.dragged() {
                                            let delta = ctx.input(|input| input.pointer.delta());
                                            resize_delta = Some((
                                                index,
                                                [delta.x / base_width.max(1.0), delta.y / base_height.max(1.0)],
                                            ));
                                        }

                                        if response.secondary_clicked() {
                                            // Clear a previous point first: a right-click
                                            // outside the rendered image must not reuse an
                                            // older anchor coordinate from this split.
                                            self.workspace.tabs[tab_index].anchor_context_position = None;
                                            if let Some(point) = response.interact_pointer_pos()
                                                && image_rect.contains(point)
                                            {
                                                let displayed = (point - image_center) / image_zoom.max(0.0001);
                                                let source = egui::vec2(
                                                    displayed.x * cos + displayed.y * sin,
                                                    -displayed.x * sin + displayed.y * cos,
                                                );
                                                let normalized = [source.x / size.x + 0.5, source.y / size.y + 0.5];
                                                if (0.0..=1.0).contains(&normalized[0]) && (0.0..=1.0).contains(&normalized[1]) {
                                                    self.workspace.tabs[tab_index].anchor_context_position = Some((index, normalized));
                                                } else {
                                                    self.workspace.tabs[tab_index].anchor_context_position = None;
                                                }
                                            }
                                        }
                                        if editing && response.clicked() && !resize_response.hovered() {
                                            let point = response.interact_pointer_pos().unwrap_or(rect.center());
                                            if rect.contains(point) {
                                                let displayed = (point - image_center) / image_zoom.max(0.0001);
                                                let source = egui::vec2(
                                                    displayed.x * cos + displayed.y * sin,
                                                    -displayed.x * sin + displayed.y * cos,
                                                );
                                                let normalized = [source.x / size.x + 0.5, source.y / size.y + 0.5];
                                                if (0.0..=1.0).contains(&normalized[0]) && (0.0..=1.0).contains(&normalized[1]) {
                                                    placed_anchor = Some((index, editing_point, normalized));
                                                }
                                            }
                                        } else if response.dragged() && !resize_response.hovered() {
                                            let delta = ctx.input(|input| input.pointer.delta());
                                            if rect.width() > 0.0 && rect.height() > 0.0 {
                                                // Preserve the actual screen-space drag. Each
                                                // destination pane converts it using its own
                                                // rendered image dimensions; otherwise images
                                                // with different aspect ratios drift apart.
                                                pan_delta = Some((index, [delta.x, delta.y]));
                                            }
                                        }
                                    } else {
                                        pane_painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
                                        if image_failed {
                                            pane_painter.text(
                                                rect.center() - egui::vec2(0.0, 12.0),
                                                egui::Align2::CENTER_CENTER,
                                                "Could not load this image",
                                                egui::FontId::proportional(12.0),
                                                ui.visuals().text_color(),
                                            );
                                            if ui.put(
                                                egui::Rect::from_center_size(
                                                    rect.center() + egui::vec2(0.0, 16.0),
                                                    egui::vec2(110.0, 26.0),
                                                ),
                                                egui::Button::new("Retry"),
                                            ).clicked() {
                                                self.workspace.compare_failed.remove(&key);
                                                self.request_compare_image(entry.clone(), key.clone(), ctx);
                                            }
                                        } else {
                                            let loading_label = if self.workspace.compare_pending.contains(&key) {
                                                "Decoding full-resolution image…"
                                            } else {
                                                "Waiting for the next image to load…"
                                            };
                                            pane_painter.text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                loading_label,
                                                egui::FontId::proportional(12.0),
                                                ui.visuals().text_color(),
                                            );
                                        }
                                    }

                                    let mut pane_zoom = self.workspace.tabs[tab_index].pane_zoom[index].clamp(0.01, 100.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Zoom");
                                        if ui.add(egui::Slider::new(&mut pane_zoom, 0.01..=100.0)
                                            .logarithmic(true).show_value(true)).changed() {
                                            let tab = &mut self.workspace.tabs[tab_index];
                                            let previous_zoom = tab.pane_zoom[index].max(0.01);
                                            let ratio = pane_zoom / previous_zoom;
                                            tab.pane_zoom[index] = pane_zoom;
                                            if tab.sync_zoom && tab.pane_sync.get(index).copied().unwrap_or(true) {
                                                let sync_targets = tab.pane_sync.clone();
                                                for (pane_index, zoom) in tab.pane_zoom.iter_mut().enumerate() {
                                                    if pane_index != index && sync_targets.get(pane_index).copied().unwrap_or(true) {
                                                        *zoom = (*zoom * ratio).clamp(0.01, 100.0);
                                                    }
                                                }
                                            }
                                        }
                                    });
                                    let mut offset = self.workspace.tabs[tab_index].offsets[index];
                                    let mut offset_changed = false;
                                    ui.horizontal(|ui| {
                                        ui.label("X");
                                        offset_changed |= ui.add(egui::DragValue::new(&mut offset[0])
                                            .speed(0.001).range(-100.0..=100.0)).changed();
                                        ui.label("Y");
                                        offset_changed |= ui.add(egui::DragValue::new(&mut offset[1])
                                            .speed(0.001).range(-100.0..=100.0)).changed();
                                    });
                                    if offset_changed {
                                        self.workspace.tabs[tab_index].offsets[index] = offset;
                                    }
                                    let target_card_height = if compare_rows {
                                        compare_viewport_height / pane_count as f32
                                    } else {
                                        compare_viewport_height
                                    };
                                    let used_card_height = ui.min_rect().height();
                                    let remaining_card_height = (target_card_height - used_card_height).max(0.0);
                                    if remaining_card_height > 0.5 {
                                        ui.allocate_space(egui::vec2(base_width, remaining_card_height));
                                    }
                                    });
                                });
                                if (index + 1) % columns == 0 {
                                    ui.end_row();
                                }
                            }
                            // Keep at least two panes visible, even when Compare
                            // starts empty. Each blank pane is an independent
                            // drop target visually and offers the same multi-file picker.
                            for index in actual_entry_count..pane_count {
                                let pane_border = ui.visuals().widgets.noninteractive.bg_stroke;
                                egui::Frame::new()
                                    .fill(ui.visuals().extreme_bg_color)
                                    .stroke(egui::Stroke::new(1.0_f32, pane_border.color))
                                    .inner_margin(egui::Margin::ZERO)
                                    .corner_radius(egui::CornerRadius::ZERO)
                                    .show(ui, |pane| {
                                        pane.set_min_width(base_width);
                                        pane.set_max_width(base_width);
                                        let placeholder_height = (if compare_rows {
                                            compare_viewport_height / pane_count as f32
                                        } else {
                                            compare_viewport_height
                                        }).max(1.0);
                                        let (rect, empty_response) = pane.allocate_exact_size(
                                            egui::vec2(base_width, placeholder_height),
                                            egui::Sense::click_and_drag(),
                                        );
                                        let compare_drag_payload_id = egui::Id::new("jpeg_viewer_compare_drag_entry");
                                        if empty_response.hovered()
                                            && ctx.input(|input| input.pointer.any_released())
                                            && let Some(path) = ctx.data(|data| data.get_temp::<std::path::PathBuf>(compare_drag_payload_id))
                                        {
                                            drop_pane_request = Some((index, path));
                                        }
                                        let first_extra_pane = actual_entry_count.max(2);
                                        let can_close_empty = index >= first_extra_pane
                                            && self.workspace.tabs[tab_index].compare_empty_panes > 0;
                                        empty_response.context_menu(|menu| {
                                            menu.label(format!("Empty split {}", index + 1));
                                            if menu.button("Add image to comparison…").clicked() {
                                                pane_context_action = Some((index, ComparePaneContextAction::ReplaceImage));
                                                menu.close();
                                            }
                                            if menu.button("Open new split").clicked() {
                                                pane_context_action = Some((index, ComparePaneContextAction::OpenNewSplit));
                                                menu.close();
                                            }
                                            if menu.add_enabled(can_close_empty, egui::Button::new("Close split")).clicked() {
                                                pane_context_action = Some((index, ComparePaneContextAction::CloseSplit));
                                                menu.close();
                                            }
                                        });
                                        let painter = pane.painter().with_clip_rect(rect.intersect(pane.clip_rect()));
                                        painter.rect_filled(rect, 0.0, pane.visuals().faint_bg_color);
                                        painter.rect_stroke(
                                            rect.shrink(8.0),
                                            4.0,
                                            egui::Stroke::new(1.0_f32, pane.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6)),
                                            egui::StrokeKind::Inside,
                                        );
                                        let message_rect = egui::Rect::from_min_size(
                                            rect.min + egui::vec2(12.0, (rect.height() * 0.5 - 36.0).max(8.0)),
                                            egui::vec2((rect.width() - 24.0).max(1.0), 42.0),
                                        );
                                        pane.put(
                                            message_rect,
                                            egui::Label::new(
                                                egui::RichText::new("Drop an image here")
                                                    .color(pane.visuals().weak_text_color()),
                                            ).wrap(),
                                        );
                                        let button_rect = egui::Rect::from_center_size(
                                            egui::pos2(rect.center().x, rect.center().y + 18.0),
                                            egui::vec2(142.0, 30.0),
                                        );
                                        pane.scope_builder(
                                            egui::UiBuilder::new().max_rect(button_rect),
                                            |button_ui| {
                                                if button_ui.add_sized(
                                                    [142.0, 30.0],
                                                    egui::Button::new("Add Image(s)…"),
                                                ).clicked() {
                                                    add_images_requested = true;
                                                }
                                            },
                                        );
                                        let masonry_button_rect = egui::Rect::from_center_size(
                                            egui::pos2(rect.center().x, rect.center().y + 54.0),
                                            egui::vec2(126.0, 28.0),
                                        );
                                        pane.scope_builder(
                                            egui::UiBuilder::new().max_rect(masonry_button_rect),
                                            |button_ui| {
                                                if crate::app::ui::masonry_button(
                                                    button_ui,
                                                    "Open Masonry",
                                                    true,
                                                    126.0,
                                                    false,
                                                    true,
                                                ).clicked() {
                                                    open_masonry_from_compare = true;
                                                }
                                            },
                                        );
                                    });
                                if (index + 1) % columns == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                            });
                        });
                },
            );

            if let Some((index, action)) = pane_context_action {
                match action {
                    ComparePaneContextAction::SetAnchor(anchor_point) => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if index < tab.entries.len() {
                            let context_position = tab.anchor_context_position.take()
                                .filter(|(pane_index, _)| *pane_index == index)
                                .map(|(_, position)| position);
                            if let Some(position) = context_position {
                                if anchor_point == 0 {
                                    tab.anchors[index] = Some(position);
                                } else {
                                    tab.anchors_second[index] = Some(position);
                                }
                                tab.anchor_editing = None;
                                align_compare_anchor_pairs(
                                    tab,
                                    &anchor_metrics,
                                    &anchor_image_sizes,
                                    &anchor_source_sizes,
                                );
                            } else {
                                // If the right-click was outside the image, retain
                                // the original click-to-place workflow as fallback.
                                tab.anchor_editing = Some(index);
                                tab.anchor_editing_point = anchor_point.min(1);
                            }
                        }
                    }
                    ComparePaneContextAction::BeginAnchor(anchor_point) => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if index < tab.entries.len() {
                            tab.anchor_context_position = None;
                            tab.anchor_editing = Some(index);
                            tab.anchor_editing_point = anchor_point.min(1);
                        }
                    }
                    ComparePaneContextAction::MoveLeft => {
                        if index > 0 {
                            reorder_compare_pane(&mut self.workspace.tabs[tab_index], index, index - 1);
                        }
                    }
                    ComparePaneContextAction::MoveRight => {
                        let last = self.workspace.tabs[tab_index].entries.len().saturating_sub(1);
                        if index < last {
                            reorder_compare_pane(&mut self.workspace.tabs[tab_index], index, index + 1);
                        }
                    }
                    ComparePaneContextAction::MoveFirst => {
                        reorder_compare_pane(&mut self.workspace.tabs[tab_index], index, 0);
                    }
                    ComparePaneContextAction::MoveLast => {
                        let last = self.workspace.tabs[tab_index].entries.len().saturating_sub(1);
                        reorder_compare_pane(&mut self.workspace.tabs[tab_index], index, last);
                    }
                    ComparePaneContextAction::AlignPanZoom => {
                        align_anchors_requested = true;
                    }
                    ComparePaneContextAction::AlignRotation => {
                        rotate_anchors_requested = true;
                    }
                    ComparePaneContextAction::ResetView => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if index < tab.offsets.len() {
                            tab.offsets[index] = [0.0, 0.0];
                        }
                        if index < tab.pane_zoom.len() {
                            tab.pane_zoom[index] = 1.0;
                        }
                        if index < tab.pane_rotation.len() {
                            tab.pane_rotation[index] = 0.0;
                        }
                        // Reset position/zoom/rotation only; anchors persist.
                    }
                    ComparePaneContextAction::ToggleSync => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if let Some(participates) = tab.pane_sync.get_mut(index) {
                            *participates = !*participates;
                        }
                    }
                    ComparePaneContextAction::ToggleAnchors => {
                        self.workspace.tabs[tab_index].show_anchors = !self.workspace.tabs[tab_index].show_anchors;
                    }
                    ComparePaneContextAction::ClearAnchors => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if index < tab.anchors.len() { tab.anchors[index] = None; }
                        if index < tab.anchors_second.len() { tab.anchors_second[index] = None; }
                        tab.anchor_context_position = None;
                        tab.anchor_editing = None;
                    }
                    ComparePaneContextAction::ReplaceImage => {
                        replace_pane_request = Some(index);
                    }
                    ComparePaneContextAction::OpenNewSplit => {
                        let tab = &mut self.workspace.tabs[tab_index];
                        tab.compare_empty_panes = tab.compare_empty_panes.saturating_add(1).min(16);
                        ctx.request_repaint();
                    }
                    ComparePaneContextAction::CloseSplit => {
                        if index < self.workspace.tabs[tab_index].entries.len() {
                            let tab = &mut self.workspace.tabs[tab_index];
                            // When there are fewer than three populated panes,
                            // removing an image would otherwise turn it into a
                            // default empty slot without reducing the pane count.
                            // Consume one extra empty pane as well so Close split
                            // actually closes one split while preserving the two-pane minimum.
                            if tab.entries.len() <= 2 && tab.compare_empty_panes > 0 {
                                tab.compare_empty_panes -= 1;
                            }
                            remove_pane = Some(index);
                        } else {
                            let tab = &mut self.workspace.tabs[tab_index];
                            let first_extra_pane = tab.entries.len().max(2);
                            if index >= first_extra_pane && tab.compare_empty_panes > 0 {
                                tab.compare_empty_panes -= 1;
                                let visible_count = tab.entries.len().max(2) + tab.compare_empty_panes;
                                tab.pane_zoom.truncate(visible_count);
                                tab.pane_rotation.truncate(visible_count);
                                tab.pane_sync.truncate(visible_count);
                                tab.pane_size.truncate(visible_count);
                                tab.offsets.truncate(visible_count);
                                tab.anchors.truncate(visible_count);
                                tab.anchors_second.truncate(visible_count);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
            }

            if let Some(index) = remove_pane {
                let removed = {
                    let tab = &mut self.workspace.tabs[tab_index];
                    if index < tab.entries.len() {
                        let removed = tab.entries.remove(index).get_id();
                        if index < tab.anchors.len() {
                            tab.anchors.remove(index);
                        }
                        if index < tab.anchors_second.len() {
                            tab.anchors_second.remove(index);
                        }
                        if index < tab.offsets.len() {
                            tab.offsets.remove(index);
                        }
                        if index < tab.pane_zoom.len() {
                            tab.pane_zoom.remove(index);
                        }
                        if index < tab.pane_rotation.len() {
                            tab.pane_rotation.remove(index);
                        }
                        if index < tab.pane_sync.len() {
                            tab.pane_sync.remove(index);
                        }
                        if index < tab.pane_size.len() {
                            tab.pane_size.remove(index);
                        }
                        tab.anchor_editing = None;
                        tab.title = format!("Compare ({})", tab.entries.len());
                        Some(removed)
                    } else {
                        None
                    }
                };
                if let Some(removed) = removed {
                    let still_used = self.workspace.tabs.iter().any(|candidate| {
                        candidate.kind == WorkspaceTabKind::Compare
                            && candidate
                                .entries
                                .iter()
                                .any(|entry| entry.get_id() == removed)
                    });
                    if !still_used {
                        self.workspace.compare_images.remove(&removed);
                        self.workspace.compare_failed.remove(&removed);
                    }
                }
                // Any other pointer interaction from this frame can refer to the
                // old pane indices, so discard it after removing a card.
                pan_delta = None;
                resize_delta = None;
                placed_anchor = None;
            }

        if rotate_anchors_requested {
            let pane_count = self.workspace.tabs[tab_index].entries.len();
            let mut angles = vec![None; pane_count];
            for index in 0..pane_count {
                if !self.workspace.tabs[tab_index]
                    .pane_sync
                    .get(index)
                    .copied()
                    .unwrap_or(true)
                {
                    continue;
                }
                let first = self.workspace.tabs[tab_index]
                    .anchors
                    .get(index)
                    .copied()
                    .flatten();
                let second = self.workspace.tabs[tab_index]
                    .anchors_second
                    .get(index)
                    .copied()
                    .flatten();
                let Some((first, second)) = first.zip(second) else {
                    continue;
                };
                let key = self.workspace.tabs[tab_index].entries[index].get_id();
                let Some((width, height)) = self
                    .workspace
                    .compare_images
                    .get(&key)
                    .map(VirtualTexture::dimensions)
                else {
                    continue;
                };
                let dx = (second[0] - first[0]) * width as f32;
                let dy = (second[1] - first[1]) * height as f32;
                if dx.hypot(dy) > 0.01 {
                    angles[index] = Some(dy.atan2(dx));
                }
            }
            if angles.iter().flatten().count() >= 2 {
                let reference_angle = angles.iter().flatten().next().copied().unwrap_or(0.0);
                let tab = &mut self.workspace.tabs[tab_index];
                tab.pane_rotation.resize(pane_count, 0.0);
                tab.pane_rotation.fill(0.0);
                for index in 0..pane_count {
                    if !tab.pane_sync.get(index).copied().unwrap_or(true) {
                        continue;
                    }
                    if let Some(angle) = angles[index] {
                        let delta = reference_angle - angle;
                        tab.pane_rotation[index] = delta.sin().atan2(delta.cos());
                    }
                }
            } else {
                self.queue_status = Some(
                    "Set both anchors on at least two loaded images before rotating to align."
                        .into(),
                );
            }
        }

            if let Some((index, screen_delta)) = pan_delta {
                let tab = &mut self.workspace.tabs[tab_index];
                let sync_source = tab.sync_pan && tab.pane_sync.get(index).copied().unwrap_or(true);
                let sync_targets = tab.pane_sync.clone();
                for (pane_index, offset) in tab.offsets.iter_mut().enumerate() {
                    if pane_index != index
                        && (!sync_source || !sync_targets.get(pane_index).copied().unwrap_or(true))
                    {
                        continue;
                    }
                    // Keep the drag distance identical in screen space while
                    // converting it to each image's own normalized coordinates.
                    // Two-anchor alignment can give panes different zoom factors,
                    // so applying one shared normalized delta would reintroduce drift.
                    if let Some([width, height]) = pan_metrics.get(pane_index).copied().flatten() {
                        offset[0] += screen_delta[0] / width.max(1.0);
                        offset[1] += screen_delta[1] / height.max(1.0);
                    }
                }
            }
            if let Some((index, delta)) = resize_delta
                && let Some(size) = self.workspace.tabs[tab_index].pane_size.get_mut(index)
            {
                size[0] = (size[0] + delta[0]).clamp(0.5, 1.0);
                size[1] = (size[1] + delta[1]).clamp(0.5, 2.0);
            }
            if let Some((index, anchor_slot, anchor)) = placed_anchor {
                let tab = &mut self.workspace.tabs[tab_index];
                if index < tab.anchors.len() {
                    if anchor_slot == 0 {
                        tab.anchors[index] = Some(anchor);
                    } else {
                        tab.anchors_second[index] = Some(anchor);
                    }
                    tab.anchor_editing = None;
                    align_compare_anchor_pairs(
                        tab,
                        &anchor_metrics,
                        &anchor_image_sizes,
                        &anchor_source_sizes,
                    );
                }
            } else if align_anchors_requested {
                let tab = &mut self.workspace.tabs[tab_index];
                align_compare_anchor_pairs(
                    tab,
                    &anchor_metrics,
                    &anchor_image_sizes,
                    &anchor_source_sizes,
                );
            }
        if let Some((index, path)) = drop_pane_request {
            self.sidebar_drag_index = None;
            let drag_payload_id = egui::Id::new("jpeg_viewer_compare_drag_entry");
            ctx.data_mut(|data| { data.remove_temp::<std::path::PathBuf>(drag_payload_id); });
            if path.is_file() && crate::imaging::helpers::is_supported_image(&path) {
                let replaced = self.replace_compare_pane(tab_id, index, ImageEntry::File(path));
                self.queue_status = Some(if replaced {
                    "Added image to comparison split.".into()
                } else {
                    "That image is already present in this comparison.".into()
                });
            } else {
                self.queue_status = Some("The dropped file is not a supported image.".into());
            }
        } else if ctx.input(|input| input.pointer.any_released()) {
            self.sidebar_drag_index = None;
            // Do not leave a stale sidebar payload that could be consumed by a
            // later unrelated click after the user drops outside a split.
            let drag_payload_id = egui::Id::new("jpeg_viewer_compare_drag_entry");
            ctx.data_mut(|data| { data.remove_temp::<std::path::PathBuf>(drag_payload_id); });
        }

        if add_images_requested
            && let Some(paths) = rfd::FileDialog::new()
                .set_title("Add comparison images")
                .pick_files()
        {
            let entries = paths
                .into_iter()
                .filter(|path| {
                    path.is_file() && crate::imaging::helpers::is_supported_image(path)
                })
                .map(ImageEntry::File)
                .collect::<Vec<_>>();
            let added = self.add_entries_to_compare(tab_id, entries);
            self.queue_status = Some(if added == 0 {
                "No new supported images were added to Compare.".into()
            } else {
                format!("Added {added} image(s) to Compare.")
            });
        }
        if let Some(index) = replace_pane_request
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Open an image in this comparison split")
                .pick_file()
        {
            if path.is_file() && crate::imaging::helpers::is_supported_image(&path) {
                let replaced = self.replace_compare_pane(
                    tab_id,
                    index,
                    ImageEntry::File(path),
                );
                self.queue_status = Some(if replaced {
                    "Updated comparison split.".into()
                } else {
                    "That image is already present in this comparison.".into()
                });
            } else {
                self.queue_status = Some("The selected file is not a supported image.".into());
            }
        }
        if close {
            self.close_workspace_tab(tab_id);
        } else if open_masonry_from_compare {
            self.return_to_masonry_from_compare(tab_id);
        }
    }

    /// Replace one populated split, or fill the next available empty split.
    fn replace_compare_pane(
        &mut self,
        compare_tab_id: u64,
        pane_index: usize,
        entry: ImageEntry,
    ) -> bool {
        let Some(tab_index) = self.workspace.tabs.iter().position(|tab| {
            tab.id == compare_tab_id && tab.kind == WorkspaceTabKind::Compare
        }) else {
            return false;
        };
        let new_id = entry.get_id();
        if pane_index >= self.workspace.tabs[tab_index].entries.len() {
            return self.add_entries_to_compare(compare_tab_id, vec![entry]) > 0;
        }

        let old_id = self.workspace.tabs[tab_index].entries[pane_index].get_id();
        if old_id == new_id {
            return false;
        }
        if self.workspace.tabs[tab_index]
            .entries
            .iter()
            .enumerate()
            .any(|(index, existing)| index != pane_index && existing.get_id() == new_id)
        {
            return false;
        }

        {
            let tab = &mut self.workspace.tabs[tab_index];
            tab.entries[pane_index] = entry;
            let entry_count = tab.entries.len();
            tab.anchors.resize(entry_count, None);
            tab.anchors_second.resize(entry_count, None);
            tab.offsets.resize(entry_count, [0.0, 0.0]);
            tab.pane_zoom.resize(entry_count, 1.0);
            tab.pane_rotation.resize(entry_count, 0.0);
            tab.pane_sync.resize(entry_count, true);
            tab.pane_size.resize(entry_count, [1.0, 1.0]);
            tab.anchors[pane_index] = None;
            tab.anchors_second[pane_index] = None;
            tab.offsets[pane_index] = [0.0, 0.0];
            tab.pane_zoom[pane_index] = 1.0;
            tab.pane_rotation[pane_index] = 0.0;
            tab.anchor_editing = None;
            tab.anchor_editing_point = 0;
            tab.title = format!("Compare ({})", tab.entries.len());
        }

        let old_still_referenced = self.workspace.tabs.iter().any(|tab| {
            tab.kind == WorkspaceTabKind::Compare
                && tab.entries.iter().any(|existing| existing.get_id() == old_id)
        });
        if !old_still_referenced {
            self.workspace.compare_images.remove(&old_id);
            self.workspace.compare_failed.remove(&old_id);
        }
        self.workspace.compare_failed.remove(&new_id);
        true
    }
}

fn reorder_compare_pane(tab: &mut WorkspaceTab, from: usize, to: usize) {
    let len = tab.entries.len();
    if from >= len || to >= len || from == to {
        return;
    }
    fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
        if from < items.len() && to < items.len() {
            let item = items.remove(from);
            items.insert(to, item);
        }
    }
    move_item(&mut tab.entries, from, to);
    move_item(&mut tab.anchors, from, to);
    move_item(&mut tab.anchors_second, from, to);
    move_item(&mut tab.offsets, from, to);
    move_item(&mut tab.pane_zoom, from, to);
    move_item(&mut tab.pane_rotation, from, to);
    move_item(&mut tab.pane_sync, from, to);
    move_item(&mut tab.pane_size, from, to);
    if let Some(editing) = tab.anchor_editing {
        tab.anchor_editing = Some(if editing == from {
            to
        } else if from < to && editing > from && editing <= to {
            editing - 1
        } else if from > to && editing >= to && editing < from {
            editing + 1
        } else {
            editing
        });
    }
    if let Some((pane_index, position)) = tab.anchor_context_position {
        let remapped = if pane_index == from {
            to
        } else if from < to && pane_index > from && pane_index <= to {
            pane_index - 1
        } else if from > to && pane_index >= to && pane_index < from {
            pane_index + 1
        } else {
            pane_index
        };
        tab.anchor_context_position = Some((remapped, position));
    }
}

fn compare_anchor_in_rotated_bounds(anchor: [f32; 2], size: [f32; 2], rotation: f32) -> [f32; 2] {
    let (sin, cos) = rotation.sin_cos();
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let rotated_width = width * cos.abs() + height * sin.abs();
    let rotated_height = width * sin.abs() + height * cos.abs();
    let x = (anchor[0] - 0.5) * width;
    let y = (anchor[1] - 0.5) * height;
    [
        0.5 + (x * cos - y * sin) / rotated_width.max(1.0),
        0.5 + (x * sin + y * cos) / rotated_height.max(1.0),
    ]
}

/// Align panes from two corresponding image points. The first point establishes
/// translation; the displayed distance between both points establishes relative zoom.
/// `image_sizes` contains the rendered image rectangle dimensions, not source pixels,
/// so the distance includes each pane's current fit scale and zoom. Anchor coordinates
/// are transformed into each pane's rotated bounding box before alignment.
fn align_compare_anchor_pairs(
    tab: &mut WorkspaceTab,
    metrics: &[Option<[f32; 2]>],
    image_sizes: &[Option<[f32; 2]>],
    source_sizes: &[Option<[f32; 2]>],
) {
    let reference_eligible = |index: usize| {
        tab.pane_sync.get(index).copied().unwrap_or(true)
            && tab.anchors.get(index).copied().flatten().is_some()
            && metrics.get(index).copied().flatten().is_some()
    };
    // Prefer a pane with both anchors so its measured A1–A2 distance defines
    // the target zoom. Fall back to a single-anchor pane for translation-only
    // alignment until a complete reference pair is available.
    let reference_index = (0..tab.entries.len())
        .find(|index| {
            reference_eligible(*index)
                && tab.anchors_second.get(*index).copied().flatten().is_some()
        })
        .or_else(|| (0..tab.entries.len()).find(|index| reference_eligible(*index)));
    let Some(reference_index) = reference_index else {
        return;
    };
    let Some(reference_first_raw) = tab.anchors[reference_index] else {
        return;
    };
    let reference_rotation = tab
        .pane_rotation
        .get(reference_index)
        .copied()
        .unwrap_or(0.0);
    let reference_source_size = source_sizes
        .get(reference_index)
        .copied()
        .flatten()
        .unwrap_or([1.0, 1.0]);
    let reference_first = compare_anchor_in_rotated_bounds(
        reference_first_raw,
        reference_source_size,
        reference_rotation,
    );
    let Some(reference_metrics) = metrics.get(reference_index).copied().flatten() else {
        return;
    };
    let reference_offset = tab
        .offsets
        .get(reference_index)
        .copied()
        .unwrap_or([0.0, 0.0]);
    let target_first = [
        0.5 + (reference_first[0] - 0.5 + reference_offset[0]) * reference_metrics[0],
        0.5 + (reference_first[1] - 0.5 + reference_offset[1]) * reference_metrics[1],
    ];

    let reference_distance = tab
        .anchors_second
        .get(reference_index)
        .copied()
        .flatten()
        .zip(image_sizes.get(reference_index).copied().flatten())
        .map(|(second, size)| {
            let source_size = source_sizes
                .get(reference_index)
                .copied()
                .flatten()
                .unwrap_or(size);
            let second = compare_anchor_in_rotated_bounds(second, source_size, reference_rotation);
            let dx = (second[0] - reference_first[0]) * size[0];
            let dy = (second[1] - reference_first[1]) * size[1];
            dx.hypot(dy)
        })
        .filter(|distance| distance.is_finite() && *distance > 0.01);

    let pane_count = tab.entries.len();
    if tab.pane_zoom.len() < pane_count {
        tab.pane_zoom.resize(pane_count, 1.0);
    }
    if tab.offsets.len() < pane_count {
        tab.offsets.resize(pane_count, [0.0, 0.0]);
    }

    for pane_index in 0..pane_count {
        if !tab.pane_sync.get(pane_index).copied().unwrap_or(true) {
            continue;
        }
        let Some(first_raw) = tab.anchors.get(pane_index).copied().flatten() else {
            continue;
        };
        let pane_rotation = tab.pane_rotation.get(pane_index).copied().unwrap_or(0.0);
        let source_size = source_sizes
            .get(pane_index)
            .copied()
            .flatten()
            .unwrap_or([1.0, 1.0]);
        let first = compare_anchor_in_rotated_bounds(first_raw, source_size, pane_rotation);
        let Some(mut pane_metrics) = metrics.get(pane_index).copied().flatten() else {
            continue;
        };

        // If both anchors are present, adjust this pane's zoom so the distance
        // between corresponding points matches the reference pane. For panes with
        // only one anchor, align position while retaining their current zoom.
        if let (Some(reference_distance), Some(second), Some(size)) = (
            reference_distance,
            tab.anchors_second.get(pane_index).copied().flatten(),
            image_sizes.get(pane_index).copied().flatten(),
        ) {
            let second = compare_anchor_in_rotated_bounds(second, source_size, pane_rotation);
            let dx = (second[0] - first[0]) * size[0];
            let dy = (second[1] - first[1]) * size[1];
            let current_distance = dx.hypot(dy);
            if current_distance.is_finite() && current_distance > 0.01 {
                let desired_ratio = reference_distance / current_distance;
                let current_zoom = tab.pane_zoom[pane_index].max(0.01);
                let target_zoom = (current_zoom * desired_ratio).clamp(0.01, 100.0);
                let actual_ratio = target_zoom / current_zoom;
                tab.pane_zoom[pane_index] = target_zoom;
                pane_metrics[0] *= actual_ratio;
                pane_metrics[1] *= actual_ratio;
            }
        }

        let safe_x = pane_metrics[0].max(0.001);
        let safe_y = pane_metrics[1].max(0.001);
        tab.offsets[pane_index] = [
            (target_first[0] - 0.5) / safe_x - (first[0] - 0.5),
            (target_first[1] - 0.5) / safe_y - (first[1] - 0.5),
        ];
    }
}

#[cfg(test)]
mod compare_rotation_tests {
    use super::compare_anchor_in_rotated_bounds;

    #[test]
    fn zero_rotation_preserves_anchor_coordinates() {
        let anchor = [0.23, 0.71];
        let transformed = compare_anchor_in_rotated_bounds(anchor, [1600.0, 900.0], 0.0);
        assert!((transformed[0] - anchor[0]).abs() < 0.0001);
        assert!((transformed[1] - anchor[1]).abs() < 0.0001);
    }

    #[test]
    fn quarter_turn_moves_right_edge_anchor_to_bottom_edge() {
        let transformed = compare_anchor_in_rotated_bounds(
            [1.0, 0.5],
            [100.0, 50.0],
            std::f32::consts::FRAC_PI_2,
        );
        assert!((transformed[0] - 0.5).abs() < 0.0001);
        assert!((transformed[1] - 1.0).abs() < 0.0001);
    }
}
