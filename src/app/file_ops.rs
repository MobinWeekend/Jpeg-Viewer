use super::types::{LoadingState, ViewerApp};
use crate::archive::{scan_7z, scan_rar, scan_zip};
use crate::constants::ARCHIVE_EXT;
use crate::helpers::{get_extension, is_supported_image};
use crate::image_core::ImageFormat;
use crate::image_entry::ImageEntry;
use arboard::Clipboard;
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Group closely timed process launches from file managers into one selection.
const OPEN_REQUEST_QUIET_PERIOD: Duration = Duration::from_millis(450);
/// Some Windows shell integrations launch selected files a little apart. Retain
/// the most recent same-folder open briefly so late process activations can join it.
const OPEN_REQUEST_GROUP_WINDOW: Duration = Duration::from_millis(1_500);

impl ViewerApp {
    pub fn open_path(&mut self, path: PathBuf) {
        if self
            .edit_session
            .as_ref()
            .is_some_and(|session| session.dirty)
        {
            self.edit_status = Some(
                "Save or discard the unsaved working copy before opening another path.".into(),
            );
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        }
        self.queue_mode = false;
        self.queue_previous_entries = None;
        if self.edit_session.is_some() {
            self.edit_session = None;
            self.show_edit_menu = false;
            self.crop_tool_active = false;
            self.crop_drag_start_image = None;
            self.crop_selection = None;
        }
        self.b_zoom_used = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.set_file_type_detection(None); // Clear file extension detection
        self.set_loading_state(LoadingState::Idle);

        // Reset preload state – discard all pending tasks and invalidate results.
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false; // ensure caching is re-enabled

        if path.is_dir() {
            self.load_directory(&path, self.sort_method);
            return;
        }

        match get_extension(&path).as_deref() {
            Some("zip") => {
                self.set_image_entries(scan_zip(&path), 0);
            }
            Some("7z") => {
                self.set_image_entries(scan_7z(&path), 0);
            }
            Some("rar") => {
                self.set_image_entries(scan_rar(&path), 0);
            }
            Some(_) if is_supported_image(&path) => {
                self.load_image(path, self.sort_method);
            }
            _ => {
                println!("Unsupported file: {:?}", path);
            }
        }
    }

    fn stop_slideshow(&mut self) {
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
        }
    }

    pub fn open_file_dialog(&mut self) {
        self.stop_slideshow();

        let mut extensions = ImageFormat::all_extensions();

        extensions.extend(ARCHIVE_EXT.iter().map(|ext| (*ext).to_string()));

        let dialog = rfd::FileDialog::new().add_filter("Images and Archives", &extensions);

        self.dialog_open = true;
        let path = dialog.pick_file();
        self.dialog_open = false;

        if let Some(path) = path {
            self.open_path(path);
        }
    }

    pub fn open_folder_dialog(&mut self) {
        self.stop_slideshow();

        self.dialog_open = true;
        let path = rfd::FileDialog::new().pick_folder();
        self.dialog_open = false;

        if let Some(path) = path {
            self.open_path(path);
        }
    }

    pub fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
    }

    pub fn save_window_state(&mut self, ctx: &egui::Context) {
        // ViewportInfo geometry is reported in egui UI points, which include the
        // global egui zoom factor. ViewportBuilder::with_position/with_inner_size
        // expects native logical points (excluding that egui zoom). Normalize here
        // before persisting, otherwise a UI scale other than 1.0 compounds the
        // size and position error every time the app is restarted.
        let current_zoom = ctx.zoom_factor();
        let zoom = if current_zoom.is_finite() && current_zoom > 0.0 {
            current_zoom
        } else {
            1.0
        };
        let (pos, size) = ctx.input(|i| {
            let outer_pos = i.viewport().outer_rect.map(|rect| rect.min * zoom);
            let inner_size = i.viewport().inner_rect.map(|rect| rect.size() * zoom);
            (outer_pos, inner_size)
        });

        // Preserve the last normal geometry. Maximized/fullscreen dimensions should
        // never replace the user's normal client size.
        let (is_fullscreen, is_maximized) = ctx.input(|i| {
            (i.viewport().fullscreen.unwrap_or(false), i.viewport().maximized.unwrap_or(false))
        });
        let mut geometry_was_saved = false;
        if !is_fullscreen {
            if !self.startup_restore_maximize_pending {
                self.settings_manager.settings.window_maximized = is_maximized;
            }
            if !is_maximized && !self.startup_restore_maximize_pending {
                if let (Some(pos), Some(size)) = (pos, size) {
                    if size.x > 100.0 && size.y > 100.0 {
                        let pos_arr = [pos.x, pos.y];
                        let size_arr = [size.x, size.y];
                        if let Err(e) = self.settings_manager.update_window_state(pos_arr, size_arr) {
                            eprintln!("Failed to save window state: {}", e);
                        } else {
                            geometry_was_saved = true;
                        }
                    }
                }
            }
        }
        // If maximized (or closing from fullscreen), preserve the last normal
        // geometry and persist the maximize flag independently.
        if !geometry_was_saved {
            if let Err(error) = self.settings_manager.save() {
                eprintln!("Failed to save window maximize state: {}", error);
            }
        }
    }

    pub fn delete_current_image(&mut self) {
        if self.image_entries.is_empty() {
            return;
        }

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        let file_path = match entry {
            ImageEntry::File(path) => path,
            _ => {
                println!("Cannot delete images inside archives directly");
                return;
            }
        };

        if !file_path.exists() {
            println!("File no longer exists on disk");
            self.image_entries.remove(self.current_index);
            if self.current_index >= self.image_entries.len() {
                self.current_index = 0;
            }
            if !self.image_entries.is_empty() {
                self.load_current_image();
            } else {
                self.texture = None;
            }
            return;
        }

        match trash::delete(&file_path) {
            Ok(_) => {
                println!("Moved to trash: {:?}", file_path);
                self.image_entries.remove(self.current_index);
                if self.image_entries.is_empty() {
                    self.texture = None;
                    self.current_index = 0;
                } else if self.current_index >= self.image_entries.len() {
                    self.current_index = self.image_entries.len() - 1;
                    self.load_current_image();
                } else {
                    self.load_current_image();
                }
                self.b_fit_to_window = true;
                self.image_rect = None;
                self.b_zoom_used = false;

                // Reset preload state
                self.image_cache.clear();
                self.preloading_indices.clear();
                self.preload_tasks.clear();
                self.preload_workers = 0;
                self.preload_generation = self.preload_generation.wrapping_add(1);
                self.should_stop_caching = false;
                self.file_type_detection = None; // Clear file extension detection
            }
            Err(e) => {
                eprintln!("Failed to move to trash: {}", e);
            }
        }
    }

    pub fn load_dropped_files(&mut self, paths: Vec<PathBuf>) {
        if self
            .edit_session
            .as_ref()
            .is_some_and(|session| session.dirty)
        {
            self.edit_status = Some(
                "Save or discard the unsaved working copy before opening dropped files.".into(),
            );
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        }
        if self.edit_session.is_some() {
            self.edit_session = None;
            self.show_edit_menu = false;
            self.crop_tool_active = false;
            self.crop_drag_start_image = None;
            self.crop_selection = None;
        }
        if paths.is_empty() {
            return;
        }

        self.b_zoom_used = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.set_loading_state(LoadingState::Idle);
        // A preceding single-file launch may have started an asynchronous scan of
        // its containing directory. Explicit file selections replace that scan;
        // dropping its receiver prevents the old directory result from replacing
        // the selected-only image list when it finishes later.
        self.indexing_receiver = None;

        // Reset preload state
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
        self.set_file_type_detection(None); // Clear file extension detection

        // Filter to only supported images
        let image_paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| {
                if path.is_file() {
                    crate::helpers::is_supported_image(path)
                } else {
                    false
                }
            })
            .collect();

        if image_paths.is_empty() {
            println!("No supported images found in dropped files");
            return;
        }

        // Store the current directory from the first file
        if let Some(parent) = image_paths.first().and_then(|p| p.parent()) {
            self.current_directory = Some(parent.to_path_buf());
        }

        // Convert to ImageEntries
        let mut entries: Vec<ImageEntry> = image_paths.into_iter().map(ImageEntry::File).collect();
        // Apply the user's normal ordering to the selected set without indexing
        // any neighboring files from the containing directory.
        crate::sort::sort_images(&mut entries, self.sort_method);

        // Set image entries starting at index 0
        self.set_image_entries(entries, 0);
        self.zoom = 1.0;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        // Cache already cleared above; no need to clear again.

        println!("Loaded {} dropped image(s)", self.image_entries.len());
    }


    /// Queue paths from a process launch. A quiet period lets file-manager
    /// one-process-per-file activations arrive as one multi-selection.
    pub fn enqueue_external_open_paths(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        // Restore and activate the primary window, including an empty launch.
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();

        if paths.is_empty() {
            return;
        }

        self.stop_slideshow();

        let now = Instant::now();
        let incoming_paths = paths;

        // Some shell integrations start one process per selected file and do not
        // launch all helper processes at exactly the same time. If a prior batch
        // from the same folder was just displayed, prepend it so the final result
        // remains the exact Explorer selection rather than a directory listing or
        // only the late-arriving tail of that selection.
        if incoming_paths.len() == 1 && self.pending_open_paths.is_empty() {
            if let Some((recent_paths, expires_at)) = self.recent_external_open_batch.take() {
                let same_directory = recent_paths.first()
                    .and_then(|path| path.parent())
                    == incoming_paths.first().and_then(|path| path.parent());
                if now <= expires_at && same_directory {
                    for path in recent_paths {
                        if !incoming_paths.contains(&path) {
                            self.pending_open_paths.push(path);
                        }
                    }
                }
            }
        } else if incoming_paths.len() > 1 {
            // A complete multi-selection arrived in one activation; it is already
            // authoritative and must not be mixed with a previous independent open.
            self.recent_external_open_batch = None;
        }

        for path in incoming_paths {
            // Keep first-seen order while avoiding duplicate command-line paths.
            if !self.pending_open_paths.contains(&path) {
                self.pending_open_paths.push(path);
            }
        }
        self.pending_open_deadline = Some(now + OPEN_REQUEST_QUIET_PERIOD);
        ctx.request_repaint_after(OPEN_REQUEST_QUIET_PERIOD);
    }

    /// Open the collected paths once no additional launch requests have arrived.
    pub fn process_pending_open_paths(&mut self, ctx: &egui::Context) {
        let Some(deadline) = self.pending_open_deadline else {
            return;
        };
        let now = Instant::now();
        if now < deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(now));
            return;
        }

        self.pending_open_deadline = None;
        let paths = std::mem::take(&mut self.pending_open_paths);
        if paths.is_empty() {
            return;
        }

        let is_queue_file = paths.len() == 1
            && paths[0].extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jvq"));
        let can_replace_current_view = !self.edit_session.as_ref().is_some_and(|session| session.dirty);

        if is_queue_file {
            self.open_queue_file(&paths[0], ctx, false);
            self.recent_external_open_batch = None;
        } else if paths.len() == 1 {
            self.open_path(paths[0].clone());
            if can_replace_current_view && paths[0].is_file() && is_supported_image(&paths[0]) {
                self.recent_external_open_batch = Some((
                    paths.clone(),
                    Instant::now() + OPEN_REQUEST_GROUP_WINDOW,
                ));
            } else {
                self.recent_external_open_batch = None;
            }
        } else {
            // Explicit multi-file opens must contain only the selected files;
            // never index the directory containing those files.
            self.load_dropped_files(paths.clone());

            let only_same_folder_images = paths.first()
                .and_then(|path| path.parent())
                .is_some_and(|parent| {
                    paths.iter().all(|path| {
                        path.parent() == Some(parent)
                            && path.is_file()
                            && is_supported_image(path)
                    })
                });
            if can_replace_current_view && only_same_folder_images {
                self.recent_external_open_batch = Some((
                    paths,
                    Instant::now() + OPEN_REQUEST_GROUP_WINDOW,
                ));
            } else {
                self.recent_external_open_batch = None;
            }
        }

        // Repeat activation after the new selection has been installed. This is
        // intentional: the initial focus request wakes/restores the window early,
        // while this one makes the completed open visible after the UI state changes.
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    pub fn handle_forwarded_open_request(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        self.enqueue_external_open_paths(paths, ctx);
    }

    pub fn copy_path_to_clipboard(&self) {
        if let Some(path) = &self.current_image_path {
            let path_str = path.display().to_string();

            let mut clipboard = Clipboard::new().expect("Failed to open clipboard");

            println!("Copied path: {}", path_str);

            if let Err(e) = clipboard.set_text(path_str) {
                eprintln!("Failed to copy path to clipboard: {}", e);
            }
        } else {
            println!("No image path to copy");
        }
    }

    pub fn copy_image_to_clipboard(&mut self) {
        let Some(path) = &self.current_image_path else {
            println!("No image file to copy");
            return;
        };

        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(e) => {
                eprintln!("Failed to open clipboard: {}", e);
                return;
            }
        };

        match clipboard.set().file_list(std::slice::from_ref(path)) {
            Ok(()) => {
                println!("Image file copied to clipboard");
            }
            Err(e) => {
                eprintln!("Failed to copy image file to clipboard: {}", e);
            }
        }
    }

    /// Rebuild the image index from the current directory
    pub fn rebuild_image_index(&mut self) {
        use std::fs;
        if let Some(directory) = &self.current_directory {
            // Read directory and get image files
            let mut files: Vec<PathBuf> = fs::read_dir(directory)
                .ok()
                .into_iter()
                .flat_map(|entries| {
                    entries
                        .filter_map(|entry| {
                            let entry = entry.ok()?;
                            let path = entry.path();
                            if path.is_file() && crate::helpers::is_supported_image(&path) {
                                Some(path)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            // Sort images using current sort method
            crate::sort::sort_images(&mut files, self.sort_method);
        }
    }
}
