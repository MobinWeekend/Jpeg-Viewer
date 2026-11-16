//! New Tab shortcuts, recently opened paths, pinning, and persisted link data.

use super::*;
use super::helpers::*;
use super::session::*;

impl crate::app::types::ViewerApp {
    pub(super) fn welcome_data_path(&self) -> PathBuf {
        self.settings_manager.path().with_file_name("welcome_links.txt")
    }

    pub(super) fn ensure_welcome_data_loaded(&mut self) {
        if self.workspace.welcome_data_loaded {
            return;
        }
        self.workspace.welcome_data_loaded = true;
        let path = self.welcome_data_path();
        let Ok(contents) = fs::read_to_string(path) else { return; };
        for line in contents.lines() {
            let Some((kind, encoded)) = line.split_once('|') else { continue; };
            let decoded = decode_field(encoded);
            if decoded.is_empty() { continue; }
            let item = PathBuf::from(decoded);
            match kind {
                "R" if !self.workspace.welcome_recent.iter().any(|p| welcome_paths_equal(p, &item)) => {
                    self.workspace.welcome_recent.push(item);
                }
                "P" if !self.workspace.welcome_pinned.iter().any(|p| welcome_paths_equal(p, &item)) => {
                    self.workspace.welcome_pinned.push(item);
                }
                _ => {}
            }
        }
        let original_recent_len = self.workspace.welcome_recent.len();
        self.workspace.welcome_recent.retain(|item| is_welcome_recent_path(item));
        self.workspace.welcome_recent.truncate(WELCOME_RECENT_LIMIT);
        self.workspace.welcome_pinned.truncate(24);
        if self.workspace.welcome_recent.len() != original_recent_len {
            self.save_welcome_data();
        }
    }

    pub(super) fn save_welcome_data(&self) {
        let path = self.welcome_data_path();
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("Could not create New Tab data directory: {error}");
                return;
            }
        }
        let mut output = String::new();
        for item in self.workspace.welcome_recent.iter().take(WELCOME_RECENT_LIMIT) {
            output.push_str(&format!("R|{}\n", encode_field(&item.to_string_lossy())));
        }
        for item in self.workspace.welcome_pinned.iter().take(24) {
            output.push_str(&format!("P|{}\n", encode_field(&item.to_string_lossy())));
        }
        let temporary = path.with_extension("txt.tmp");
        if fs::write(&temporary, output).is_ok() {
            if fs::rename(&temporary, &path).is_err() {
                // Windows does not replace an existing file on rename. Fall back
                // to a direct write so Welcome links remain available next launch.
                let _ = fs::write(&path, fs::read_to_string(&temporary).unwrap_or_default());
                let _ = fs::remove_file(temporary);
            }
        }
    }

    pub(crate) fn welcome_recent_paths(&mut self) -> Vec<PathBuf> {
        self.ensure_welcome_data_loaded();
        self.workspace.welcome_recent.clone()
    }

    pub(crate) fn welcome_pinned_paths(&mut self) -> Vec<PathBuf> {
        self.ensure_welcome_data_loaded();
        self.workspace.welcome_pinned.clone()
    }

    pub(crate) fn record_welcome_recent_path(&mut self, path: PathBuf) {
        if !is_welcome_recent_path(&path) {
            return;
        }
        self.ensure_welcome_data_loaded();
        if self.workspace.welcome_recent.first().is_some_and(|first| welcome_paths_equal(first, &path)) {
            return;
        }
        self.workspace.welcome_recent.retain(|existing| !welcome_paths_equal(existing, &path));
        self.workspace.welcome_thumbnail_sources.remove(&path);
        self.workspace.welcome_recent.insert(0, path);
        self.workspace.welcome_recent.truncate(WELCOME_RECENT_LIMIT);
        self.prune_welcome_thumbnail_sources();
        self.save_welcome_data();
    }

    pub(super) fn prune_welcome_thumbnail_sources(&mut self) {
        let retained_paths = self.workspace.welcome_recent.iter()
            .chain(self.workspace.welcome_pinned.iter())
            .cloned()
            .collect::<HashSet<_>>();
        self.workspace.welcome_thumbnail_sources
            .retain(|path, _| retained_paths.contains(path));
    }

    pub(crate) fn add_welcome_pin(&mut self, path: PathBuf) {
        self.ensure_welcome_data_loaded();
        if !self.workspace.welcome_pinned.iter().any(|existing| welcome_paths_equal(existing, &path)) {
            self.workspace.welcome_thumbnail_sources.remove(&path);
            self.workspace.welcome_pinned.push(path);
            self.workspace.welcome_pinned.truncate(24);
            self.prune_welcome_thumbnail_sources();
            self.save_welcome_data();
        }
    }

    pub(crate) fn remove_welcome_pin(&mut self, path: &Path) {
        self.ensure_welcome_data_loaded();
        self.workspace.welcome_pinned.retain(|existing| !welcome_paths_equal(existing, path));
        self.workspace.welcome_thumbnail_sources.remove(path);
        self.prune_welcome_thumbnail_sources();
        self.save_welcome_data();
    }

    pub(crate) fn open_welcome_shortcut(&mut self, path: PathBuf, ctx: &egui::Context) {
        if path.is_dir()
            || crate::imaging::helpers::is_supported_image(&path)
            || matches!(path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase().as_str(), "zip" | "7z" | "rar")
        {
            self.open_path(path);
            return;
        }

        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
        if extension != "jvq" && extension != "txt" {
            self.queue_status = Some("New Tab shortcuts support image files, folders, and .jvq playlists.".into());
            return;
        }
        let paths = match crate::app::queues::read_queue(&path) {
            Ok((paths, _)) => {
                self.record_welcome_recent_path(path.clone());
                paths
            }
            Err(error) => {
                self.queue_status = Some(format!("Could not open playlist: {error}"));
                return;
            }
        };
        let mut image_paths = Vec::new();
        let mut seen = HashSet::new();
        for item in paths {
            let expanded = if item.is_dir() {
                crate::app::queues::collect_folder_images(&item)
            } else if item.is_file() && crate::imaging::helpers::is_supported_image(&item) {
                vec![item]
            } else {
                Vec::new()
            };
            for image in expanded {
                if seen.insert(image.clone()) { image_paths.push(image); }
            }
        }
        if image_paths.is_empty() {
            self.queue_status = Some("This playlist contains no available supported images.".into());
            return;
        }
        if !self.prepare_for_image_change("Save or discard unsaved edits before opening this playlist.") {
            return;
        }
        let entries = image_paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
        let directory = entries.first().and_then(entry_parent);
        self.activate_primary_viewer_tab();
        self.current_directory = directory.clone();
        self.set_image_entries(entries.clone(), 0);
        if let Some(tab) = self.workspace.tabs.iter_mut().find(|tab| tab.kind == WorkspaceTabKind::Viewer) {
            tab.entries = entries;
            tab.current_index = 0;
            tab.directory = directory;
            tab.zoom = 1.0;
            tab.pan = egui::Vec2::ZERO;
            tab.fit_to_window = true;
            tab.zoom_used = false;
        }
        self.should_stop_caching = false;
        ctx.request_repaint();
    }

    pub(crate) fn load_favorites_from_welcome(&mut self) {
        self.ensure_favorites_loaded();
        let paths = self.favorite_paths.clone();
        if paths.is_empty() {
            self.queue_status = Some("Your Favorites list is empty.".into());
            return;
        }
        self.open_masonry_from_paths(&paths, "Masonry · Favorites");
    }

    pub(crate) fn open_new_welcome_tab(&mut self) {
        self.snapshot_active_viewer_tab();
        let id = self.workspace.allocate_id();
        self.workspace.tabs.push(WorkspaceTab::welcome(id));
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
    }

    pub(crate) fn active_workspace_hides_image_navigation(&self) -> bool {
        matches!(
            self.workspace.active_kind(),
            WorkspaceTabKind::Masonry | WorkspaceTabKind::Compare | WorkspaceTabKind::Welcome
        )
    }
}
