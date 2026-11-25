//! Copy, move, delete, conflict resolution, and confirmation dialogs for workspace entries.

use super::helpers::*;
use super::*;

/// After removing the current entry, its old numeric position is occupied by
/// the next entry. Wrap to the beginning when navigation normally wraps; a
/// non-repeating Queue instead stays on its final remaining entry.
fn next_index_after_removal(previous_index: usize, remaining_len: usize, wrap: bool) -> usize {
    if remaining_len == 0 {
        0
    } else if previous_index < remaining_len {
        previous_index
    } else if wrap {
        0
    } else {
        remaining_len - 1
    }
}

impl crate::app::types::ViewerApp {
    pub(super) fn copy_or_move_selected(&mut self, entries: Vec<ImageEntry>, move_files: bool) {
        if entries.is_empty() {
            return;
        }
        if move_files
            && entries
                .iter()
                .any(|entry| !matches!(entry, ImageEntry::File(_)))
        {
            self.queue_status =
                Some("Move is available only when every selected entry is a regular file.".into());
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title(if move_files {
                "Move selected images to…"
            } else {
                "Copy or extract selected images to…"
            })
            .pick_folder()
        else {
            return;
        };

        let pending = PendingFileOperation {
            entries,
            destination,
            move_files,
        };
        if operation_has_conflicts(&pending) {
            self.workspace.pending_file_operation = Some(pending);
        } else {
            self.execute_file_operation(pending, ConflictResolution::KeepBoth);
        }
    }

    pub(crate) fn render_file_conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.workspace.pending_file_operation.as_ref() else {
            return;
        };
        let mut seen_targets = HashSet::new();
        let conflicts = pending
            .entries
            .iter()
            .filter(|entry| {
                let target = pending.destination.join(entry_display_name(entry));
                let duplicate_in_batch = !seen_targets.insert(target.clone());
                let source_is_target = matches!(entry, ImageEntry::File(path) if *path == target);
                (target.exists() && !source_is_target) || duplicate_in_batch
            })
            .map(entry_display_name)
            .collect::<Vec<_>>();
        let destination = pending.destination.display().to_string();
        let move_files = pending.move_files;
        let mut resolution = None;
        egui::Window::new("File name conflicts")
            .id(egui::Id::unique("masonry_file_conflicts"))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("{} selected item(s) already exist in the destination:", conflicts.len()));
                ui.label(egui::RichText::new(destination).weak());
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for name in conflicts.iter().take(30) { ui.label(name); }
                    if conflicts.len() > 30 { ui.label(format!("…and {} more", conflicts.len() - 30)); }
                });
                ui.separator();
                ui.label("Choose one action for all name conflicts in this operation:");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Replace all").clicked() { resolution = Some(ConflictResolution::Replace); }
                    if ui.button("Skip all").clicked() { resolution = Some(ConflictResolution::Skip); }
                    if ui.button("Keep Both all").clicked() { resolution = Some(ConflictResolution::KeepBoth); }
                    if ui.button("Cancel").clicked() { resolution = Some(ConflictResolution::Cancel); }
                });
                if move_files {
                    ui.label(egui::RichText::new("Replace stages the new file and keeps a backup until the replacement is committed.").small().weak());
                } else {
                    ui.label(egui::RichText::new("Replace overwrites the destination file. Keep Both adds a numbered suffix.").small().weak());
                }
            });
        if let Some(resolution) = resolution {
            let pending = self.workspace.pending_file_operation.take();
            if let Some(pending) = pending
                && resolution != ConflictResolution::Cancel
            {
                self.execute_file_operation(pending, resolution);
            }
        }
    }

    pub(super) fn execute_file_operation(
        &mut self,
        pending: PendingFileOperation,
        resolution: ConflictResolution,
    ) {
        let mut succeeded = 0usize;
        let mut skipped = 0usize;
        let mut failed = Vec::new();
        let mut moved_paths: HashMap<String, PathBuf> = HashMap::new();

        for entry in pending.entries {
            let entry_id = entry.get_id();
            let file_name = entry_display_name(&entry);
            let original_target = pending
                .destination
                .join(Path::new(&file_name).file_name().unwrap_or_default());
            let source_is_target =
                matches!(&entry, ImageEntry::File(source) if *source == original_target);

            if source_is_target && pending.move_files {
                skipped += 1;
                continue;
            }

            let mut target = if original_target.exists() {
                match resolution {
                    ConflictResolution::Replace if !source_is_target => original_target,
                    ConflictResolution::Skip => {
                        skipped += 1;
                        continue;
                    }
                    ConflictResolution::KeepBoth | ConflictResolution::Replace => {
                        // A copy onto itself cannot replace its source. Keep Both
                        // is the only safe interpretation for that edge case.
                        unique_destination_path(&original_target)
                    }
                    ConflictResolution::Cancel => continue,
                }
            } else {
                original_target
            };

            // Stage every output beside its destination, then commit it. Existing
            // destination files are moved to a backup only after staging succeeds;
            // a failed copy/extraction therefore never destroys the old file.
            let staged = unique_sidecar_path(&target, "stage");
            if let Err(error) = stage_entry_to_file(&entry, &staged) {
                let _ = fs::remove_file(&staged);
                failed.push(format!("{file_name}: {error}"));
                continue;
            }
            // Re-check at commit time: another process may have created the
            // destination after the conflict dialog was displayed.
            if target.exists() && resolution != ConflictResolution::Replace {
                match resolution {
                    ConflictResolution::Skip => {
                        let _ = fs::remove_file(&staged);
                        skipped += 1;
                        continue;
                    }
                    ConflictResolution::KeepBoth => {
                        target = unique_destination_path(&target);
                    }
                    ConflictResolution::Cancel => {
                        let _ = fs::remove_file(&staged);
                        continue;
                    }
                    ConflictResolution::Replace => unreachable!(),
                }
            }
            if let Err(error) =
                commit_staged_file(&staged, &target, resolution == ConflictResolution::Replace)
            {
                let _ = fs::remove_file(&staged);
                failed.push(format!("{file_name}: {error}"));
                continue;
            }

            if pending.move_files
                && let ImageEntry::File(source) = &entry
            {
                if let Err(error) = fs::remove_file(source) {
                    // Keep both copies rather than deleting the successfully
                    // installed destination or pretending the move completed.
                    failed.push(format!("{file_name}: copied to destination, but source could not be removed: {error}"));
                    continue;
                }
                moved_paths.insert(entry_id, target.clone());
            }
            succeeded += 1;
        }

        if !moved_paths.is_empty() {
            // Keep every open reference in sync with successful moves. Folder
            // Masonry collections behave like a fresh scan: remove files that
            // leave the folder and add files that move into an already-loaded one.
            for tab in &mut self.workspace.tabs {
                let is_masonry = tab.kind == WorkspaceTabKind::Masonry;
                let is_folder_masonry = is_masonry && tab.folder_masonry;
                let is_browse_folder = tab.kind == WorkspaceTabKind::Viewer
                    && !self.queue_mode
                    && !self.sidebar_queue_tab
                    && !self.sidebar_favorites_tab
                    && tab.directory.as_deref().is_some_and(|folder| folder.is_dir());
                let recursive_folder = is_folder_masonry && tab.recursive;
                let folder = tab.directory.clone();
                let previous_tab_index = tab.current_index;
                let current_entry_id = tab
                    .entries
                    .get(previous_tab_index)
                    .map(ImageEntry::get_id);

                if is_folder_masonry || is_browse_folder {
                    // A move can change the order of more than the moved item (date
                    // sorting, natural sorting, and shuffle all depend on the full
                    // collection). Re-index the complete folder instead of patching
                    // just the moved entry into the stale list.
                    if let Some(root) = folder.as_deref()
                        && std::fs::read_dir(root).is_ok()
                    {
                        let paths = if recursive_folder {
                            crate::app::queues::collect_folder_images(root)
                        } else {
                            crate::imaging::loader::load_directory_images(
                                root,
                                crate::imaging::sort::SortMethod::Natural,
                            )
                        };
                        let mut updated_entries = paths
                            .into_iter()
                            .map(ImageEntry::File)
                            .collect::<Vec<_>>();
                        crate::imaging::sort::sort_images(&mut updated_entries, self.sort_method);

                        // If the selected file itself was moved but remains inside a
                        // recursive collection, follow its new path identity. When it
                        // left the collection, retaining the old numeric position picks
                        // the next image now occupying that slot.
                        let selected_id_after_move = current_entry_id
                            .as_ref()
                            .and_then(|old_id| {
                                moved_paths.get(old_id).map(|new_path| {
                                    ImageEntry::File(new_path.clone()).get_id()
                                })
                            })
                            .or_else(|| current_entry_id.clone());
                        tab.entries = std::mem::take(&mut updated_entries);
                        tab.current_index = selected_id_after_move
                            .as_ref()
                            .and_then(|selected_id| {
                                tab.entries
                                    .iter()
                                    .position(|entry| entry.get_id() == *selected_id)
                            })
                            .unwrap_or_else(|| {
                                next_index_after_removal(
                                    previous_tab_index,
                                    tab.entries.len(),
                                    true,
                                )
                            });
                    }
                } else {
                    // Image tabs and collection-based Masonry retain their
                    // entries when moved; only the backing path and stable ID change.
                    for entry in &mut tab.entries {
                        let old_id = entry.get_id();
                        if let Some(new_path) = moved_paths.get(&old_id)
                            && let ImageEntry::File(path) = entry
                        {
                            *path = new_path.clone();
                        }
                    }
                }

                if is_masonry {
                    // Transfer already-known tile geometry to the new stable ID
                    // only when the moved image still belongs to this collection.
                    for (old_id, new_path) in &moved_paths {
                        let new_id = ImageEntry::File(new_path.clone()).get_id();
                        let retained = tab.entries.iter().any(|entry| entry.get_id() == new_id);
                        if retained {
                            if let Some(aspect) = tab.masonry_aspect_ratios.remove(old_id) {
                                tab.masonry_aspect_ratios.insert(new_id.clone(), aspect);
                            }
                            if let Some(dimensions) = tab.masonry_dimensions.remove(old_id) {
                                tab.masonry_dimensions.insert(new_id, dimensions);
                            }
                        } else {
                            tab.masonry_aspect_ratios.remove(old_id);
                            tab.masonry_dimensions.remove(old_id);
                        }
                    }
                    let valid_ids = tab
                        .entries
                        .iter()
                        .map(ImageEntry::get_id)
                        .collect::<HashSet<_>>();
                    tab.masonry_aspect_ratios
                        .retain(|id, _| valid_ids.contains(id));
                    tab.masonry_dimensions
                        .retain(|id, _| valid_ids.contains(id));
                    for focus_id in [
                        &mut tab.masonry_focus_entry_id,
                        &mut tab.masonry_focus_highlight_id,
                    ] {
                        let moved_focus_path = focus_id
                            .as_ref()
                            .and_then(|old_id| moved_paths.get(old_id))
                            .cloned();
                        if let Some(new_path) = moved_focus_path {
                            *focus_id = Some(ImageEntry::File(new_path).get_id());
                        }
                        if focus_id
                            .as_ref()
                            .is_some_and(|id| !valid_ids.contains(id))
                        {
                            *focus_id = None;
                        }
                    }
                    if tab
                        .context_highlight
                        .as_ref()
                        .is_some_and(|id| !valid_ids.contains(id))
                    {
                        tab.context_highlight = None;
                    }
                    tab.invalidate_masonry_layout();
                }
                tab.selected.clear();
                tab.current_index = tab.current_index.min(tab.entries.len().saturating_sub(1));
                if tab.kind == WorkspaceTabKind::Image
                    && let Some(entry) = tab.entries.first()
                {
                    tab.title = entry_display_name(entry);
                    tab.directory = entry_parent(entry);
                }
            }
            // Browse thumbnails use ViewerApp::image_entries rather than reading
            // directly from WorkspaceTab. Keep that live sidebar source in step with
            // the remembered Viewer tab after a move from Masonry/Compare.
            let browse_tab_id = match self.workspace.active_kind() {
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Image => self
                    .workspace
                    .tabs
                    .get(self.workspace.active_index)
                    .map(|tab| tab.id)
                    .unwrap_or(1),
                WorkspaceTabKind::Masonry => self
                    .workspace
                    .masonry_return_tab_id
                    .or(self.workspace.last_image_tab_id)
                    .unwrap_or(1),
                WorkspaceTabKind::Compare | WorkspaceTabKind::Welcome => self
                    .workspace
                    .last_image_tab_id
                    .unwrap_or(1),
            };
            if let Some(browse_tab) = self
                .workspace
                .tabs
                .iter()
                .find(|tab| tab.id == browse_tab_id && tab.kind != WorkspaceTabKind::Masonry && tab.kind != WorkspaceTabKind::Compare)
            {
                let entries = browse_tab.entries.clone();
                let current_index = browse_tab
                    .current_index
                    .min(entries.len().saturating_sub(1));
                if matches!(
                    self.workspace.active_kind(),
                    WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
                ) {
                    self.current_directory = browse_tab.directory.clone();
                    self.set_image_entries_preserving_cache(entries, current_index);
                    if self.image_entries.is_empty() {
                        self.current_image_path = None;
                        if self.workspace.active_kind() == WorkspaceTabKind::Viewer {
                            self.workspace.home_page_visible = true;
                        }
                    }
                } else {
                    // Masonry/Compare own their visible surfaces. Keep the hidden
                    // viewer collection current without disrupting their renderer.
                    self.image_entries = entries;
                    self.current_index = current_index;
                }
            }

            // Old cache keys no longer identify the moved paths.
            for old_id in moved_paths.keys() {
                self.workspace.compare_images.remove(old_id);
                self.workspace.compare_failed.remove(old_id);
            }
        }

        self.queue_status = Some(if failed.is_empty() {
            format!(
                "{} completed, {} skipped. {}",
                succeeded,
                skipped,
                if pending.move_files {
                    "Move operation finished."
                } else {
                    "Copy/extract operation finished."
                }
            )
        } else {
            format!(
                "Completed {succeeded}, skipped {skipped}, failed {}. {}",
                failed.len(),
                failed.into_iter().take(3).collect::<Vec<_>>().join("; ")
            )
        });
    }

    pub(crate) fn render_delete_confirmation(&mut self, ctx: &egui::Context) {
        let Some((_, entries, archive_count)) = self
            .workspace
            .delete_confirmation
            .as_ref()
            .map(|(id, entries, archive_count)| (*id, entries.clone(), *archive_count))
        else {
            return;
        };
        let mut close = false;
        let mut confirm = false;
        egui::Window::new("Move selected images to trash?")
            .id(egui::Id::unique("masonry_delete_confirmation"))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} regular file(s) will be moved to the system trash.",
                    entries.len()
                ));
                if archive_count > 0 {
                    ui.label(format!(
                        "{archive_count} selected archive entr(y/ies) will remain untouched."
                    ));
                }
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for entry in entries.iter().take(30) {
                            ui.label(entry_display_name(entry));
                        }
                        if entries.len() > 30 {
                            ui.label(format!("…and {} more", entries.len() - 30));
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                    if ui.button("Move to Trash").clicked() {
                        confirm = true;
                    }
                });
            });
        if confirm {
            // Snapshot only when the user confirms. Doing this every frame while
            // the dialog is open needlessly re-caches the current image.
            if matches!(
                self.workspace.active_kind(),
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
            ) {
                self.snapshot_active_viewer_tab();
            }

            let mut removed_entries = Vec::new();
            let mut failures = Vec::new();
            for entry in &entries {
                if let ImageEntry::File(path) = entry {
                    match trash::delete(path) {
                        Ok(()) => removed_entries.push(entry.clone()),
                        Err(error) => failures.push(format!("{}: {error}", path.display())),
                    }
                }
            }

            // Remove only files that were actually moved to the system trash.
            // Keep failures visible in every open collection, and prune Compare
            // pane metadata in lockstep with the entries it describes.
            let removed_ids = removed_entries
                .iter()
                .map(ImageEntry::get_id)
                .collect::<HashSet<_>>();
            let previous_renderer_index = self.current_index;
            let previous_renderer_entry_id = self
                .image_entries
                .get(self.current_index)
                .map(ImageEntry::get_id);
            let active_renderer_entry_was_removed = previous_renderer_entry_id
                .as_ref()
                .is_some_and(|id| removed_ids.contains(id));
            let queue_does_not_repeat = self.queue_mode && !self.queue_repeat;
            if !removed_ids.is_empty() {
                for tab in &mut self.workspace.tabs {
                    if tab.kind == WorkspaceTabKind::Compare {
                        let old_entries = std::mem::take(&mut tab.entries);
                        let old_anchors = std::mem::take(&mut tab.anchors);
                        let old_anchors_second = std::mem::take(&mut tab.anchors_second);
                        let old_offsets = std::mem::take(&mut tab.offsets);
                        let old_zoom = std::mem::take(&mut tab.pane_zoom);
                        let old_rotation = std::mem::take(&mut tab.pane_rotation);
                        let old_sync = std::mem::take(&mut tab.pane_sync);
                        let old_size = std::mem::take(&mut tab.pane_size);
                        for (index, entry) in old_entries.into_iter().enumerate() {
                            if removed_ids.contains(&entry.get_id()) {
                                continue;
                            }
                            tab.entries.push(entry);
                            tab.anchors.push(old_anchors.get(index).copied().flatten());
                            tab.anchors_second
                                .push(old_anchors_second.get(index).copied().flatten());
                            tab.offsets
                                .push(old_offsets.get(index).copied().unwrap_or([0.0, 0.0]));
                            tab.pane_zoom
                                .push(old_zoom.get(index).copied().unwrap_or(1.0));
                            tab.pane_rotation
                                .push(old_rotation.get(index).copied().unwrap_or(0.0));
                            tab.pane_sync
                                .push(old_sync.get(index).copied().unwrap_or(true));
                            tab.pane_size
                                .push(old_size.get(index).copied().unwrap_or([1.0, 1.0]));
                        }
                        // Removing any pane can change index positions; cancel an
                        // in-progress anchor edit rather than targeting another image.
                        tab.anchor_editing = None;
                        tab.anchor_editing_point = 0;
                        tab.title = format!("Compare ({})", tab.entries.len());
                    } else {
                        let previous_tab_index = tab.current_index;
                        let previous_tab_entry_id = tab
                            .entries
                            .get(tab.current_index)
                            .map(ImageEntry::get_id);
                        tab.entries
                            .retain(|entry| !removed_ids.contains(&entry.get_id()));
                        tab.selected.retain(|id| !removed_ids.contains(id));
                        if tab.masonry_focus_entry_id
                            .as_ref()
                            .is_some_and(|id| removed_ids.contains(id))
                        {
                            tab.masonry_focus_entry_id = None;
                        }
                        if tab.masonry_focus_highlight_id
                            .as_ref()
                            .is_some_and(|id| removed_ids.contains(id))
                        {
                            tab.masonry_focus_highlight_id = None;
                        }
                        if tab.context_highlight
                            .as_ref()
                            .is_some_and(|id| removed_ids.contains(id))
                        {
                            tab.context_highlight = None;
                        }
                        if tab.kind == WorkspaceTabKind::Masonry {
                            let valid_ids = tab
                                .entries
                                .iter()
                                .map(ImageEntry::get_id)
                                .collect::<HashSet<_>>();
                            tab.masonry_aspect_ratios
                                .retain(|id, _| valid_ids.contains(id));
                            tab.masonry_dimensions
                                .retain(|id, _| valid_ids.contains(id));
                            tab.invalidate_masonry_layout();
                        }
                        let non_repeating_viewer =
                            tab.kind == WorkspaceTabKind::Viewer && queue_does_not_repeat;
                        tab.current_index = previous_tab_entry_id
                            .as_ref()
                            .and_then(|selected_id| {
                                tab.entries
                                    .iter()
                                    .position(|entry| entry.get_id() == *selected_id)
                            })
                            .unwrap_or_else(|| {
                                next_index_after_removal(
                                    previous_tab_index,
                                    tab.entries.len(),
                                    !non_repeating_viewer,
                                )
                            });
                    }
                }
                self.workspace
                    .compare_images
                    .retain(|key, _| !removed_ids.contains(key));
                self.workspace
                    .compare_failed
                    .retain(|key| !removed_ids.contains(key));
                self.image_entries
                    .retain(|entry| !removed_ids.contains(&entry.get_id()));
                self.current_index = previous_renderer_entry_id
                    .as_ref()
                    .and_then(|selected_id| {
                        self.image_entries
                            .iter()
                            .position(|entry| entry.get_id() == *selected_id)
                    })
                    .unwrap_or_else(|| {
                        next_index_after_removal(
                            previous_renderer_index,
                            self.image_entries.len(),
                            !queue_does_not_repeat,
                        )
                    });
            }

            let empty_image_tabs = self
                .workspace
                .tabs
                .iter()
                .filter(|tab| tab.kind == WorkspaceTabKind::Image && tab.entries.is_empty())
                .map(|tab| tab.id)
                .collect::<Vec<_>>();
            for id in empty_image_tabs {
                self.close_workspace_tab(id);
            }

            if matches!(
                self.workspace.active_kind(),
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
            ) {
                let active_state = self.workspace.active_tab().map(|tab| {
                    (
                        tab.id,
                        tab.kind,
                        tab.entries.clone(),
                        tab.current_index,
                        tab.directory.clone(),
                    )
                });
                if let Some((tab_id, tab_kind, active_entries, active_index, directory)) = active_state {
                    let active_index = active_index.min(active_entries.len().saturating_sub(1));
                    let renderer_matches = self.image_entries.len() == active_entries.len()
                        && self
                            .image_entries
                            .iter()
                            .zip(active_entries.iter())
                            .all(|(current, active)| current.get_id() == active.get_id())
                        && self.current_index == active_index;
                    if !renderer_matches || active_renderer_entry_was_removed {
                        self.current_directory = directory;
                        self.set_image_entries_preserving_cache(active_entries, active_index);
                    }
                    if self.image_entries.is_empty() {
                        self.current_image_path = None;
                        self.file_metadata = None;
                        self.image_rect = None;
                        self.workspace.home_page_visible =
                            tab_kind == WorkspaceTabKind::Viewer && tab_id == 1;
                    }
                }
            }

            self.queue_status = Some(if failures.is_empty() {
                format!(
                    "Moved {} image(s) to the system trash; {archive_count} archive entr(y/ies) left untouched.",
                    removed_entries.len()
                )
            } else {
                format!(
                    "Moved {}; {archive_count} archive entr(y/ies) left untouched; failed: {}",
                    removed_entries.len(),
                    failures.into_iter().take(3).collect::<Vec<_>>().join("; ")
                )
            });
            close = true;
        }
        if close {
            self.workspace.delete_confirmation = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_index_after_removal;

    #[test]
    fn removal_selects_the_entry_that_moves_into_the_old_slot() {
        assert_eq!(next_index_after_removal(2, 5, true), 2);
    }

    #[test]
    fn removal_of_the_last_entry_wraps_when_navigation_wraps() {
        assert_eq!(next_index_after_removal(4, 4, true), 0);
    }

    #[test]
    fn non_repeating_queue_stays_on_its_last_remaining_entry() {
        assert_eq!(next_index_after_removal(4, 4, false), 3);
    }

    #[test]
    fn removing_every_entry_resets_the_index_to_zero() {
        assert_eq!(next_index_after_removal(2, 0, true), 0);
        assert_eq!(next_index_after_removal(2, 0, false), 0);
    }
}
