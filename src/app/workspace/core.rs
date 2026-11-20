//! Workspace entry points, active-view snapshots, and preload coordination.

use super::helpers::*;
use super::*;

impl crate::app::types::ViewerApp {
    /// Snapshot the collection-based viewer before switching to another tab.
    pub(super) fn snapshot_active_viewer_tab(&mut self) {
        if !matches!(
            self.workspace.active_kind(),
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
        ) {
            return;
        }
        // Preserve the currently displayed GPU texture before the shared Viewer
        // renderer is repointed at another workspace's collection.
        self.cache_current_image();
        if let Some(tab) = self.workspace.active_tab_mut() {
            tab.entries = self.image_entries.clone();
            tab.current_index = self.current_index;
            tab.directory = self.current_directory.clone();
            tab.zoom = self.zoom;
            tab.pan = self.pan;
            tab.fit_to_window = self.b_fit_to_window;
            tab.zoom_used = self.b_zoom_used;
        }
    }

    pub(crate) fn open_local_masonry(&mut self) {
        // Returning from an Image tab should restore the existing Masonry
        // collection rather than replace it with the tab's single image.
        if self.workspace.active_kind() == WorkspaceTabKind::Image
            && self.activate_existing_masonry_from_current_workspace()
        {
            return;
        }
        if self.image_entries.is_empty() {
            self.queue_status =
                Some("There are no current image entries to show in Local Masonry.".into());
            return;
        }
        self.open_masonry_collection(
            self.image_entries.clone(),
            "Masonry · Local".to_owned(),
            self.current_directory.clone(),
        );
    }

    /// Open filesystem collections such as Queue or Favorites in Masonry. Folder
    /// entries are expanded to supported images; unsupported files and missing
    /// paths are ignored because Masonry is an image-entry workspace.
    pub(crate) fn open_masonry_from_paths(&mut self, paths: &[PathBuf], title: &str) {
        fn append_path(
            path: &Path,
            entries: &mut Vec<ImageEntry>,
            seen: &mut HashSet<String>,
            depth: usize,
        ) {
            if depth > 8 {
                return;
            }
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let expanded: Vec<ImageEntry> = if path.is_dir() {
                crate::app::queues::collect_folder_images(path)
                    .into_iter()
                    .map(ImageEntry::File)
                    .collect()
            } else if path.is_file() && crate::imaging::helpers::is_supported_image(path) {
                vec![ImageEntry::File(path.to_path_buf())]
            } else if path.is_file() && extension == "zip" {
                crate::imaging::archive::scan_zip(path)
            } else if path.is_file() && extension == "7z" {
                crate::imaging::archive::scan_7z(path)
            } else if path.is_file() && extension == "rar" {
                crate::imaging::archive::scan_rar(path)
            } else if path.is_file() && matches!(extension.as_str(), "jvq" | "txt") {
                match crate::app::queues::read_queue(path) {
                    Ok((nested, _)) => {
                        for item in nested {
                            append_path(&item, entries, seen, depth + 1);
                        }
                        Vec::new()
                    }
                    Err(_) => Vec::new(),
                }
            } else {
                Vec::new()
            };
            for entry in expanded {
                if seen.insert(entry.get_id()) {
                    entries.push(entry);
                }
            }
        }

        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            append_path(path, &mut entries, &mut seen, 0);
        }
        if entries.is_empty() {
            self.queue_status = Some(format!(
                "{title} contains no available supported image entries."
            ));
            return;
        }
        crate::imaging::sort::sort_images(&mut entries, self.sort_method);
        self.open_masonry_collection(entries, title.to_owned(), None);
    }

    pub(super) fn open_masonry_collection(
        &mut self,
        entries: Vec<ImageEntry>,
        title: String,
        directory: Option<PathBuf>,
    ) {
        if entries.is_empty() {
            self.queue_status =
                Some("There are no supported image entries to show in Masonry.".into());
            return;
        }
        if self.workspace.active_kind() != WorkspaceTabKind::Masonry {
            let return_tab_id = self
                .workspace
                .tabs
                .get(self.workspace.active_index)
                .map(|tab| tab.id);
            self.workspace.masonry_return_tab_id = return_tab_id;
        }
        self.snapshot_active_viewer_tab();
        // Masonry is a singleton workspace. Reuse its stable tab ID and visual
        // preferences, replacing the collection whenever another source is opened.
        if let Some(index) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.kind == WorkspaceTabKind::Masonry)
        {
            let valid_ids = entries
                .iter()
                .map(ImageEntry::get_id)
                .collect::<HashSet<_>>();
            let tab = &mut self.workspace.tabs[index];
            tab.title = title;
            tab.entries = entries;
            tab.invalidate_masonry_layout();
            // Keep known tile dimensions for entries that survive a collection
            // switch. A thumbnail LRU eviction or a source switch must not make
            // an already-sized tile collapse to the square loading placeholder.
            tab.masonry_aspect_ratios
                .retain(|id, _| valid_ids.contains(id));
            tab.masonry_dimensions
                .retain(|id, _| valid_ids.contains(id));
            tab.directory = directory;
            tab.folder_masonry = false;
            tab.recursive = false;
            tab.selection_mode = false;
            tab.selected.clear();
            tab.context_highlight = None;
            tab.search.clear();
            tab.extension_filter.clear();
            tab.collapsed_folders.clear();
            self.workspace.active_index = index;
        } else {
            let id = self.workspace.allocate_id();
            let mut tab = WorkspaceTab::new(id, title, WorkspaceTabKind::Masonry);
            tab.entries = entries;
            tab.directory = directory;
            tab.masonry_tile_size = self
                .settings_manager
                .get()
                .masonry_thumbnail_size
                .clamp(76.0, 512.0);
            self.workspace.tabs.push(tab);
            self.workspace.active_index = self.workspace.tabs.len() - 1;
        }
        self.stop_preloading_for_workspace();
    }

    pub(crate) fn open_folder_masonry(&mut self) {
        let Some(directory) = self.current_directory.clone() else {
            self.queue_status = Some("Open a folder or image first to use Folder Masonry.".into());
            return;
        };
        if self.workspace.active_kind() != WorkspaceTabKind::Masonry {
            let return_tab_id = self
                .workspace
                .tabs
                .get(self.workspace.active_index)
                .map(|tab| tab.id);
            self.workspace.masonry_return_tab_id = return_tab_id;
        }
        self.snapshot_active_viewer_tab();
        let paths = crate::imaging::loader::load_directory_images(&directory, self.sort_method);
        let entries = paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
        // Keep an empty Folder Masonry collection available for recursive browsing.
        if let Some(index) = self
            .workspace
            .tabs
            .iter()
            .position(|tab| tab.kind == WorkspaceTabKind::Masonry)
        {
            let valid_ids = entries
                .iter()
                .map(ImageEntry::get_id)
                .collect::<HashSet<_>>();
            let tab = &mut self.workspace.tabs[index];
            tab.title = "Masonry · Folder".into();
            tab.entries = entries;
            tab.invalidate_masonry_layout();
            tab.masonry_aspect_ratios
                .retain(|id, _| valid_ids.contains(id));
            tab.masonry_dimensions
                .retain(|id, _| valid_ids.contains(id));
            tab.directory = Some(directory);
            tab.folder_masonry = true;
            tab.recursive = false;
            tab.selection_mode = false;
            tab.selected.clear();
            tab.context_highlight = None;
            tab.search.clear();
            tab.extension_filter.clear();
            tab.collapsed_folders.clear();
            self.workspace.active_index = index;
        } else {
            let id = self.workspace.allocate_id();
            let mut tab =
                WorkspaceTab::new(id, "Masonry · Folder".into(), WorkspaceTabKind::Masonry);
            tab.entries = entries;
            tab.directory = Some(directory);
            tab.masonry_tile_size = self
                .settings_manager
                .get()
                .masonry_thumbnail_size
                .clamp(76.0, 512.0);
            tab.folder_masonry = true;
            self.workspace.tabs.push(tab);
            self.workspace.active_index = self.workspace.tabs.len() - 1;
        }
        self.stop_preloading_for_workspace();
    }

    pub(super) fn stop_preloading_for_workspace(&mut self) {
        self.should_stop_caching = true;
        self.cancel_all_preload_tasks();
        self.preloading_indices.clear();
        self.preload_skipped.clear();

        self.preload_working = false;
        self.preload_generation = self.preload_generation.wrapping_add(1);
    }

    /// Masonry-specific conventional shortcuts take precedence over the viewer's
    /// global shortcuts while the Masonry workspace is active.
    pub(crate) fn handle_workspace_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.workspace.active_kind() != WorkspaceTabKind::Masonry {
            return false;
        }
        let (escape, select_all, delete) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::A) && input.modifiers.ctrl,
                input.key_pressed(egui::Key::Delete),
            )
        });
        let Some(index) = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| {
                self.workspace
                    .tabs
                    .iter()
                    .position(|candidate| candidate.id == tab.id)
                    .unwrap_or(self.workspace.active_index)
            })
        else {
            return false;
        };

        if escape {
            let tab = &mut self.workspace.tabs[index];
            if tab.selection_mode {
                tab.selection_mode = false;
                tab.selected.clear();
                return true;
            }
        }
        if select_all {
            let tab = &mut self.workspace.tabs[index];
            let search = tab.search.to_lowercase();
            let extension_filter = tab.extension_filter.to_lowercase();
            tab.selection_mode = true;
            tab.selected = tab
                .entries
                .iter()
                .filter(|entry| {
                    let name = entry_display_name(entry).to_lowercase();
                    let extension = Path::new(&name)
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    (search.is_empty() || name.contains(&search))
                        && (extension_filter.is_empty() || extension == extension_filter)
                })
                .map(ImageEntry::get_id)
                .collect();
            return true;
        }
        if delete {
            let selected = self.workspace.tabs[index].selected.len();
            if selected == 0 {
                self.queue_status =
                    Some("Select one or more entries before pressing Delete.".into());
            } else {
                self.handle_masonry_action(index, MasonryAction::Delete, ctx);
            }
            return true;
        }
        false
    }

    /// Activate the existing singleton Masonry workspace from the current
    /// Viewer/Image workspace, remembering where the toggle should return.
    fn activate_existing_masonry_from_current_workspace(&mut self) -> bool {
        if !matches!(
            self.workspace.active_kind(),
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
        ) {
            return false;
        }
        let Some(masonry_id) = self
            .workspace
            .tabs
            .iter()
            .find(|tab| tab.kind == WorkspaceTabKind::Masonry)
            .map(|tab| tab.id)
        else {
            return false;
        };
        let current_id = self
            .workspace
            .tabs
            .get(self.workspace.active_index)
            .map(|tab| tab.id);
        self.workspace.masonry_return_tab_id = current_id;
        let home_was_visible = self.workspace.home_page_visible;
        self.activate_workspace_tab(masonry_id);
        self.workspace.home_page_visible = home_was_visible;
        true
    }

    /// Main navigation action for the Masonry glyph. The primary Viewer owns
    /// the current collection, so opening Masonry from it should reflect that
    /// collection rather than a stale previous Masonry snapshot. Image tabs keep
    /// the singleton Masonry/return-tab behavior.
    pub(crate) fn activate_masonry_from_navigation_button(&mut self) {
        match self.workspace.active_kind() {
            WorkspaceTabKind::Masonry => self.toggle_masonry_workspace(),
            WorkspaceTabKind::Viewer => self.open_local_masonry(),
            WorkspaceTabKind::Image => self.toggle_masonry_workspace(),
            _ => {}
        }
    }

    /// Toggle between the current non-Masonry workspace and the singleton
    /// Masonry workspace. Reuse the last Masonry collection when returning from
    /// an Image tab instead of replacing it with that tab's one-image collection.
    pub(crate) fn toggle_masonry_workspace(&mut self) {
        if self.workspace.active_kind() == WorkspaceTabKind::Masonry {
            let home_was_visible = self.workspace.home_page_visible;
            let remembered_return_id = self.workspace.masonry_return_tab_id.take();
            let return_id = remembered_return_id
                .filter(|id| {
                    self.workspace
                        .tabs
                        .iter()
                        .any(|tab| tab.id == *id && tab.kind != WorkspaceTabKind::Masonry)
                })
                .or_else(|| {
                    self.workspace
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
                });
            if let Some(return_id) = return_id {
                self.activate_workspace_tab(return_id);
                self.workspace.home_page_visible = home_was_visible;
            }
        } else if matches!(
            self.workspace.active_kind(),
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
        ) && !self.image_entries.is_empty()
        {
            // Reuse the previous Masonry collection whenever one exists. Only the
            // first visit seeds a new Masonry collection from the current Viewer.
            if !self.activate_existing_masonry_from_current_workspace() {
                self.open_local_masonry();
            }
        }
    }
}
