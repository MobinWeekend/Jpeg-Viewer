//! Copy, move, delete, conflict resolution, and confirmation dialogs for workspace entries.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    pub(super) fn copy_or_move_selected(&mut self, entries: Vec<ImageEntry>, move_files: bool) {
        if entries.is_empty() { return; }
        if move_files && entries.iter().any(|entry| !matches!(entry, ImageEntry::File(_))) {
            self.queue_status = Some("Move is available only when every selected entry is a regular file.".into());
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title(if move_files { "Move selected images to…" } else { "Copy or extract selected images to…" })
            .pick_folder() else { return; };

        let pending = PendingFileOperation { entries, destination, move_files };
        if operation_has_conflicts(&pending) {
            self.workspace.pending_file_operation = Some(pending);
        } else {
            self.execute_file_operation(pending, ConflictResolution::KeepBoth);
        }
    }

    pub(super) fn render_file_conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.workspace.pending_file_operation.as_ref() else { return; };
        let mut seen_targets = HashSet::new();
        let conflicts = pending.entries.iter().filter(|entry| {
            let target = pending.destination.join(entry_display_name(entry));
            let duplicate_in_batch = !seen_targets.insert(target.clone());
            let source_is_target = matches!(entry, ImageEntry::File(path) if *path == target);
            (target.exists() && !source_is_target) || duplicate_in_batch
        }).map(entry_display_name).collect::<Vec<_>>();
        let destination = pending.destination.display().to_string();
        let move_files = pending.move_files;
        let mut resolution = None;
        egui::Window::new("File name conflicts")
            .id(egui::Id::new("masonry_file_conflicts"))
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
            if let Some(pending) = pending {
                if resolution != ConflictResolution::Cancel {
                    self.execute_file_operation(pending, resolution);
                }
            }
        }
    }

    pub(super) fn execute_file_operation(&mut self, pending: PendingFileOperation, resolution: ConflictResolution) {
        let mut succeeded = 0usize;
        let mut skipped = 0usize;
        let mut failed = Vec::new();
        let mut moved_paths: HashMap<String, PathBuf> = HashMap::new();

        for entry in pending.entries {
            let entry_id = entry.get_id();
            let file_name = entry_display_name(&entry);
            let original_target = pending.destination.join(Path::new(&file_name).file_name().unwrap_or_default());
            let source_is_target = matches!(&entry, ImageEntry::File(source) if *source == original_target);

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
            if let Err(error) = commit_staged_file(&staged, &target, resolution == ConflictResolution::Replace) {
                let _ = fs::remove_file(&staged);
                failed.push(format!("{file_name}: {error}"));
                continue;
            }

            if pending.move_files {
                if let ImageEntry::File(source) = &entry {
                    if let Err(error) = fs::remove_file(source) {
                        // Keep both copies rather than deleting the successfully
                        // installed destination or pretending the move completed.
                        failed.push(format!("{file_name}: copied to destination, but source could not be removed: {error}"));
                        continue;
                    }
                    moved_paths.insert(entry_id, target.clone());
                }
            }
            succeeded += 1;
        }

        if !moved_paths.is_empty() {
            // Keep open image/Compare/Local Masonry references valid after a
            // successful move. Folder Masonry removes entries that have left
            // that folder, matching what a fresh folder scan would show.
            for tab in &mut self.workspace.tabs {
                let is_folder_masonry = tab.kind == WorkspaceTabKind::Masonry && tab.folder_masonry;
                let recursive_folder = tab.recursive;
                let folder = tab.directory.clone();
                if is_folder_masonry {
                    tab.entries.retain(|entry| {
                        moved_paths.get(&entry.get_id()).map_or(true, |new_path| {
                            if recursive_folder {
                                folder.as_ref().is_some_and(|root| new_path.starts_with(root))
                            } else {
                                new_path.parent() == folder.as_deref()
                            }
                        })
                    });
                }
                for entry in &mut tab.entries {
                    let old_id = entry.get_id();
                    if let Some(new_path) = moved_paths.get(&old_id) {
                        if let ImageEntry::File(path) = entry {
                            *path = new_path.clone();
                        }
                    }
                }
                tab.selected.clear();
                tab.current_index = tab.current_index.min(tab.entries.len().saturating_sub(1));
                if tab.kind == WorkspaceTabKind::Image {
                    if let Some(entry) = tab.entries.first() {
                        tab.title = entry_display_name(entry);
                        tab.directory = entry_parent(entry);
                    }
                }
            }
            // Old cache keys no longer identify the moved paths.
            for old_id in moved_paths.keys() {
                self.workspace.compare_images.remove(old_id);
                self.workspace.compare_failed.remove(old_id);
            }
        }

        self.queue_status = Some(if failed.is_empty() {
            format!("{} completed, {} skipped. {}", succeeded, skipped, if pending.move_files { "Move operation finished." } else { "Copy/extract operation finished." })
        } else {
            format!("Completed {succeeded}, skipped {skipped}, failed {}. {}", failed.len(), failed.into_iter().take(3).collect::<Vec<_>>().join("; "))
        });
    }

    pub(super) fn render_delete_confirmation(&mut self, ctx: &egui::Context) {
        let Some((_, entries, archive_count)) = self.workspace.delete_confirmation.as_ref()
            .map(|(id, entries, archive_count)| (*id, entries.clone(), *archive_count)) else { return; };
        let mut close = false;
        let mut confirm = false;
        egui::Window::new("Move selected images to trash?")
            .id(egui::Id::new("masonry_delete_confirmation"))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("{} regular file(s) will be moved to the system trash.", entries.len()));
                if archive_count > 0 {
                    ui.label(format!("{archive_count} selected archive entr(y/ies) will remain untouched."));
                }
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for entry in entries.iter().take(30) { ui.label(entry_display_name(entry)); }
                    if entries.len() > 30 { ui.label(format!("…and {} more", entries.len() - 30)); }
                });
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { close = true; }
                    if ui.button("Move to Trash").clicked() { confirm = true; }
                });
            });
        if confirm {
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
            let removed_ids = removed_entries.iter().map(ImageEntry::get_id).collect::<HashSet<_>>();
            if !removed_ids.is_empty() {
                for tab in &mut self.workspace.tabs {
                    if tab.kind == WorkspaceTabKind::Compare {
                        let old_entries = std::mem::take(&mut tab.entries);
                        let old_anchors = std::mem::take(&mut tab.anchors);
                        let old_anchors_second = std::mem::take(&mut tab.anchors_second);
                        let old_offsets = std::mem::take(&mut tab.offsets);
                        let old_zoom = std::mem::take(&mut tab.pane_zoom);
                        let old_sync = std::mem::take(&mut tab.pane_sync);
                        let old_size = std::mem::take(&mut tab.pane_size);
                        for (index, entry) in old_entries.into_iter().enumerate() {
                            if removed_ids.contains(&entry.get_id()) { continue; }
                            tab.entries.push(entry);
                            tab.anchors.push(old_anchors.get(index).copied().flatten());
                            tab.anchors_second.push(old_anchors_second.get(index).copied().flatten());
                            tab.offsets.push(old_offsets.get(index).copied().unwrap_or([0.0, 0.0]));
                            tab.pane_zoom.push(old_zoom.get(index).copied().unwrap_or(1.0));
                            tab.pane_sync.push(old_sync.get(index).copied().unwrap_or(true));
                            tab.pane_size.push(old_size.get(index).copied().unwrap_or([1.0, 1.0]));
                        }
                        // Removing any pane can change index positions; cancel an
                        // in-progress anchor edit rather than targeting another image.
                        tab.anchor_editing = None;
                        tab.anchor_editing_point = 0;
                        tab.title = format!("Compare ({})", tab.entries.len());
                    } else {
                        tab.entries.retain(|entry| !removed_ids.contains(&entry.get_id()));
                        tab.selected.retain(|id| !removed_ids.contains(id));
                        tab.current_index = tab.current_index.min(tab.entries.len().saturating_sub(1));
                    }
                }
                self.workspace.compare_images.retain(|key, _| !removed_ids.contains(key));
                self.workspace.compare_failed.retain(|key| !removed_ids.contains(key));
            }

            let empty_image_tabs = self.workspace.tabs.iter()
                .filter(|tab| tab.kind == WorkspaceTabKind::Image && tab.entries.is_empty())
                .map(|tab| tab.id)
                .collect::<Vec<_>>();
            for id in empty_image_tabs { self.close_workspace_tab(id); }

            self.queue_status = Some(if failures.is_empty() {
                format!("Moved {} image(s) to the system trash; {archive_count} archive entr(y/ies) left untouched.", removed_entries.len())
            } else {
                format!("Moved {}; {archive_count} archive entr(y/ies) left untouched; failed: {}", removed_entries.len(), failures.into_iter().take(3).collect::<Vec<_>>().join("; "))
            });
            close = true;
        }
        if close { self.workspace.delete_confirmation = None; }
    }
}
