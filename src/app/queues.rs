use super::types::ViewerApp;
use crate::helpers::is_supported_image;
use crate::image_entry::ImageEntry;
use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};

const QUEUE_MAGIC: &str = "JPEG_VIEWER_QUEUE\t1";

impl ViewerApp {
    pub fn render_queue_manager(&mut self, ctx: &egui::Context) {
        if !self.show_queue_manager {
            return;
        }

        let mut open = self.show_queue_manager;
        let mut close_requested = false;
        egui::Window::new("Selection & Queue")
            .open(&mut open)
            .default_pos(egui::pos2(128.0, 128.0))
            .default_width(520.0)
            .min_width(420.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Add files…").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().pick_files() {
                            let mut added = 0;
                            for path in paths { added += usize::from(self.add_queue_path(path)); }
                            self.queue_status = Some(format!("Added {added} file(s)."));
                        }
                    }
                    if ui.button("Add current image").clicked() {
                        self.add_current_image_to_queue();
                    }
                    if ui.button("Add current folder images").clicked() {
                        self.add_current_folder_images();
                    }
                    if ui.button("Add folder recursively…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            let added = self.add_folder_recursively(&path);
                            self.queue_status = Some(format!("Added {added} image(s) from folder tree."));
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open queue").clicked() {
                        close_requested = self.open_queue_in_viewer(ctx);
                    }
                    if ui.button("Return to previous view").clicked() {
                        self.return_to_previous_view(ctx);
                    }
                    if ui.button("Save queue…").clicked() {
                        self.save_queue_dialog();
                    }
                    if ui.button("Load queue…").clicked() {
                        self.load_queue_dialog();
                    }
                    if ui.button("Export paths…").clicked() {
                        self.export_queue_paths();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Remap missing root…").clicked() {
                        self.remap_missing_queue_root();
                    }
                    if ui.button("Remove unavailable").clicked() {
                        let before = self.queue_paths.len();
                        self.queue_paths.retain(|path| path.exists());
                        self.queue_status = Some(format!("Removed {} unavailable queue item(s).", before - self.queue_paths.len()));
                    }
                    if ui.button("Clear queue").clicked() {
                        if self.queue_mode { self.return_to_previous_view(ctx); }
                        self.queue_paths.clear();
                        self.queue_mode = false;
                        self.queue_status = Some("Queue cleared.".into());
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.queue_shuffle, "Shuffle next");
                    ui.checkbox(&mut self.queue_repeat, "Repeat queue");
                    ui.label(format!("{} item(s)", self.queue_paths.len()));
                });
                ui.separator();
                if self.queue_paths.is_empty() {
                    ui.label("Queue is empty. Add the current image, current folder, or another folder tree.");
                } else {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        let mut remove_index = None;
                        let mut move_action: Option<(usize, isize)> = None;
                        let mut open_index = None;
                        for (index, path) in self.queue_paths.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let unavailable = !path.exists();
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                let label = if unavailable { format!("⚠ {name}") } else { name.into_owned() };
                                if ui.selectable_label(false, label).on_hover_text(path.display().to_string()).clicked() {
                                    open_index = Some(index);
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("×").on_hover_text("Remove from queue").clicked() { remove_index = Some(index); }
                                    if ui.small_button("↓").on_hover_text("Move down").clicked() { move_action = Some((index, 1)); }
                                    if ui.small_button("↑").on_hover_text("Move up").clicked() { move_action = Some((index, -1)); }
                                });
                            });
                        }
                        if let Some(index) = remove_index {
                            self.queue_paths.remove(index);
                        }
                        if let Some((index, delta)) = move_action {
                            let target = index as isize + delta;
                            if target >= 0 && (target as usize) < self.queue_paths.len() {
                                self.queue_paths.swap(index, target as usize);
                            }
                        }
                        if let Some(index) = open_index {
                            close_requested = self.open_queue_in_viewer_at(ctx, index);
                        }
                    });
                }
                if let Some(status) = self.queue_status.as_deref() {
                    ui.separator();
                    ui.label(status);
                }
            });
        self.show_queue_manager = open && !close_requested;
    }

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

    fn add_folder_recursively(&mut self, root: &Path) -> usize {
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

    fn add_queue_path(&mut self, path: PathBuf) -> bool {
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

    fn open_queue_in_viewer_at(&mut self, ctx: &egui::Context, index: usize) -> bool {
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
        self.show_queue_manager = false;
        ctx.request_repaint();
        true
    }

    fn return_to_previous_view(&mut self, ctx: &egui::Context) {
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

    fn save_queue_dialog(&mut self) {
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

    fn load_queue_dialog(&mut self) {
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

    fn export_queue_paths(&mut self) {
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

    fn remap_missing_queue_root(&mut self) {
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

fn entry_path(entry: &ImageEntry) -> Option<&PathBuf> {
    if let ImageEntry::File(path) = entry {
        Some(path)
    } else {
        None
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn encode_hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(text: &str) -> Result<String, String> {
    if text.len() % 2 != 0 {
        return Err("invalid hex field length".into());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex field".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| "queue path is not valid UTF-8".into())
}

fn write_queue(path: &Path, items: &[PathBuf], root: Option<&Path>) -> Result<(), String> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let root = root
        .filter(|candidate| items.iter().all(|item| item.starts_with(candidate)))
        .map(Path::to_path_buf)
        .or_else(|| common_parent(items));
    let mut output = format!("{QUEUE_MAGIC}\n");
    if let Some(root) = &root {
        output.push_str(&format!("ROOT\t{}\n", encode_hex(&root.to_string_lossy())));
    }
    for item in items {
        if let Ok(relative) = item.strip_prefix(base) {
            output.push_str(&format!(
                "ITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ));
        } else {
            output.push_str(&format!(
                "ITEM\tA\t{}\n",
                encode_hex(&item.to_string_lossy())
            ));
        }
    }
    let temp = path.with_extension("jvq.tmp");
    let backup = path.with_extension("jvq.bak");
    fs::write(&temp, output).map_err(|e| e.to_string())?;
    let had_existing = path.exists();
    if had_existing {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(path, &backup)
            .map_err(|e| format!("cannot protect existing queue file: {e}"))?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot commit queue file: {error}"));
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn read_queue(path: &Path) -> Result<(Vec<PathBuf>, Option<PathBuf>), String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut lines = contents.lines();
    if lines.next() != Some(QUEUE_MAGIC) {
        // Plain-text path lists are accepted as an import format. Missing paths
        // are retained just like missing entries in native queue files.
        let mut imported = Vec::new();
        for line in contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let item = PathBuf::from(line);
            imported.push(if item.is_absolute() {
                item
            } else {
                absolute_path(&base.join(item))
            });
        }
        let imported_root = common_parent(&imported);
        return Ok((imported, imported_root));
    }
    let mut root = None;
    let mut items = Vec::new();
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["ROOT", encoded] => root = Some(PathBuf::from(decode_hex(encoded)?)),
            ["ITEM", kind @ ("A" | "R"), encoded] => {
                let value = PathBuf::from(decode_hex(encoded)?);
                let resolved = if *kind == "R" {
                    absolute_path(&base.join(value))
                } else {
                    value
                };
                items.push(resolved);
            }
            [""] => {}
            _ => return Err("malformed queue entry".into()),
        }
    }
    Ok((items, root))
}

fn remap_paths(items: &mut [PathBuf], old_root: &Path, new_root: &Path) -> usize {
    let mut remapped = 0;
    for path in items {
        if path.exists() {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(old_root) {
            let candidate = new_root.join(relative);
            if candidate.exists() {
                *path = candidate;
                remapped += 1;
            }
        }
    }
    remapped
}

fn common_parent(items: &[PathBuf]) -> Option<PathBuf> {
    let first = items.first()?;
    let mut candidate = first.parent()?.to_path_buf();
    for path in items.iter().skip(1) {
        while !path.starts_with(&candidate) {
            if !candidate.pop() {
                return None;
            }
        }
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("jpeg-viewer-queue-{stamp}-{name}"))
    }

    #[test]
    fn queue_roundtrip_preserves_missing_absolute_paths() {
        let dir = temp_path("roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("sample.jvq");
        let missing = dir.join("not-here.jpg");
        write_queue(&queue_file, std::slice::from_ref(&missing), Some(&dir)).unwrap();
        let (loaded, root) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![missing]);
        assert_eq!(root, Some(dir.clone()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hex_roundtrip_supports_unicode_and_delimiters() {
        let original = "фото folder/一枚 image.jpg";
        assert_eq!(decode_hex(&encode_hex(original)).unwrap(), original);
    }

    #[test]
    fn relative_paths_resolve_from_queue_file_directory() {
        let dir = temp_path("relative");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("portable.jvq");
        let relative = PathBuf::from("sub/photo.jpg");
        fs::write(
            &queue_file,
            format!(
                "{QUEUE_MAGIC}\nITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ),
        )
        .unwrap();
        let (loaded, _) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![absolute_path(&dir.join(relative))]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remapping_preserves_unresolved_items_and_repairs_moved_root() {
        let base = temp_path("remap");
        let old_root = base.join("old");
        let new_root = base.join("new");
        fs::create_dir_all(new_root.join("sub")).unwrap();
        fs::write(new_root.join("sub/photo.jpg"), b"test").unwrap();
        let mut items = vec![old_root.join("sub/photo.jpg"), old_root.join("other.jpg")];
        let remapped = remap_paths(&mut items, &old_root, &new_root);
        assert_eq!(remapped, 1);
        assert_eq!(items[0], new_root.join("sub/photo.jpg"));
        assert_eq!(items[1], old_root.join("other.jpg"));
        fs::remove_dir_all(base).unwrap();
    }
}
