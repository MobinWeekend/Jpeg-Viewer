//! Queue operations, navigation, and queue-file dialogs.
use super::super::types::ViewerApp;
use super::entry_path;
use super::storage::{absolute_path, paths_equal, read_queue, remap_paths, write_queue};
use crate::helpers::is_supported_image;
use crate::image_entry::ImageEntry;
use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};

impl ViewerApp {
    pub fn add_current_image_to_queue(&mut self) {
        let path = self
            .image_entries
            .get(self.current_index)
            .and_then(entry_path)
            .cloned();
        if let Some(path) = path {
            let added = self.add_queue_path(path);
            self.queue_status = Some(
                if added {
                    "Added current image to queue."
                } else {
                    "Current image is already in the queue."
                }
                .into(),
            );
        } else {
            self.queue_status = Some(
                "Only filesystem image files can be queued; archive entries are not supported yet."
                    .into(),
            );
        }
    }

    pub fn add_current_folder_images(&mut self) {
        let paths: Vec<PathBuf> = self
            .image_entries
            .iter()
            .filter_map(entry_path)
            .cloned()
            .collect();
        let mut count = 0;
        for path in paths {
            count += usize::from(self.add_queue_path(path));
        }
        self.queue_status = Some(format!("Added {count} image(s) from the current view."));
    }

    pub(super) fn add_folder_recursively(&mut self, root: &Path) -> usize {
        let mut stack = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(folder) = stack.pop() {
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() && is_supported_image(&path) {
                    found.push(path);
                }
            }
        }
        found.sort();
        let mut added = 0;
        for path in found {
            added += usize::from(self.add_queue_path(path));
        }
        added
    }

    pub(super) fn add_queue_path(&mut self, path: PathBuf) -> bool {
        let normalized = absolute_path(&path);
        if self
            .queue_paths
            .iter()
            .any(|existing| paths_equal(existing, &normalized))
        {
            return false;
        }
        self.queue_paths.push(normalized);
        true
    }

    pub fn open_queue_in_viewer(&mut self, ctx: &egui::Context) -> bool {
        self.open_queue_in_viewer_at(ctx, 0)
    }

    pub(super) fn open_queue_in_viewer_at(&mut self, ctx: &egui::Context, index: usize) -> bool {
        if self.queue_paths.is_empty() {
            return false;
        }
        if self
            .edit_session
            .as_ref()
            .is_some_and(|session| session.dirty)
        {
            self.edit_status = Some("Save or discard unsaved edits before opening a queue.".into());
            self.show_edit_menu = true;
            return false;
        }
        if !self.queue_mode {
            self.queue_previous_entries = Some(self.image_entries.clone());
            self.queue_previous_index = self.current_index;
        }
        self.queue_mode = true;
        let entries = self
            .queue_paths
            .iter()
            .cloned()
            .map(ImageEntry::File)
            .collect();
        self.set_image_entries(entries, index.min(self.queue_paths.len() - 1));
        self.queue_status = Some("Browsing queue order. Missing files remain in the queue and show an unavailable warning.".into());
        ctx.request_repaint();
        true
    }

    pub(super) fn return_to_previous_view(&mut self, ctx: &egui::Context) {
        let Some(entries) = self.queue_previous_entries.take() else {
            self.queue_status = Some("There is no previous view to restore.".into());
            return;
        };
        self.queue_mode = false;
        let index = self
            .queue_previous_index
            .min(entries.len().saturating_sub(1));
        self.set_image_entries(entries, index);
        self.queue_status = Some("Returned to the previous browsing view.".into());
        ctx.request_repaint();
    }

    pub(super) fn save_queue_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG Viewer queue", &["jvq"])
            .set_file_name("image-queue.jvq")
            .save_file()
        else {
            return;
        };
        match write_queue(&path, &self.queue_paths, self.current_directory.as_deref()) {
            Ok(()) => self.queue_status = Some(format!("Saved queue to {}", path.display())),
            Err(error) => self.queue_status = Some(format!("Could not save queue: {error}")),
        }
    }

    pub(super) fn load_queue_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG Viewer queue", &["jvq", "txt"])
            .pick_file()
        else {
            return;
        };
        match read_queue(&path) {
            Ok((paths, root)) => {
                self.queue_paths = paths;
                self.queue_root = root;
                let missing = self
                    .queue_paths
                    .iter()
                    .filter(|path| !path.exists())
                    .count();
                self.queue_status = Some(format!(
                    "Loaded {} item(s); {missing} unavailable. Missing entries were preserved.",
                    self.queue_paths.len()
                ));
            }
            Err(error) => self.queue_status = Some(format!("Could not load queue: {error}")),
        }
    }

    pub(super) fn export_queue_paths(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Path list", &["txt"])
            .set_file_name("image-paths.txt")
            .save_file()
        else {
            return;
        };
        let contents = self
            .queue_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        match fs::write(&path, contents) {
            Ok(()) => {
                self.queue_status = Some(format!("Exported {} path(s).", self.queue_paths.len()))
            }
            Err(error) => self.queue_status = Some(format!("Could not export paths: {error}")),
        }
    }

    pub(super) fn remap_missing_queue_root(&mut self) {
        let Some(old_root) = self.queue_root.clone() else {
            self.queue_status =
                Some("Load a saved queue first; it must contain a root path to remap.".into());
            return;
        };
        let Some(new_root) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let remapped = remap_paths(&mut self.queue_paths, &old_root, &new_root);
        self.queue_root = Some(new_root);
        let still_missing = self
            .queue_paths
            .iter()
            .filter(|path| !path.exists())
            .count();
        self.queue_status = Some(format!(
            "Remapped {remapped} item(s); {still_missing} remain unavailable."
        ));
    }
}
