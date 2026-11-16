//! Workspace entry points, active-view snapshots, and preload coordination.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    /// Snapshot the collection-based viewer before switching to another tab.
    pub(super) fn snapshot_active_viewer_tab(&mut self) {
        if !matches!(self.workspace.active_kind(), WorkspaceTabKind::Viewer | WorkspaceTabKind::Image) {
            return;
        }
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
        if self.image_entries.is_empty() {
            self.queue_status = Some("There are no current image entries to show in Local Masonry.".into());
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
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            let expanded = if path.is_dir() {
                crate::app::queues::collect_folder_images(path)
            } else if path.is_file() && crate::imaging::helpers::is_supported_image(path) {
                vec![path.clone()]
            } else {
                Vec::new()
            };
            for path in expanded {
                let entry = ImageEntry::File(path);
                if seen.insert(entry.get_id()) {
                    entries.push(entry);
                }
            }
        }
        if entries.is_empty() {
            self.queue_status = Some(format!("{title} contains no available supported image entries."));
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
            self.queue_status = Some("There are no supported image entries to show in Masonry.".into());
            return;
        }
        self.snapshot_active_viewer_tab();
        let ids = entries.iter().map(ImageEntry::get_id).collect::<Vec<_>>();
        if let Some(index) = self.workspace.tabs.iter().position(|tab| {
            tab.kind == WorkspaceTabKind::Masonry
                && !tab.folder_masonry
                && tab.directory == directory
                && tab.entries.iter().map(ImageEntry::get_id).collect::<Vec<_>>() == ids
        }) {
            self.workspace.active_index = index;
            self.stop_preloading_for_workspace();
            return;
        }
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(id, title, WorkspaceTabKind::Masonry);
        tab.entries = entries;
        tab.directory = directory;
        tab.masonry_tile_size = self.settings_manager.get().masonry_thumbnail_size.clamp(76.0, 512.0);
        self.workspace.tabs.push(tab);
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
    }

    pub(crate) fn open_folder_masonry(&mut self) {
        let Some(directory) = self.current_directory.clone() else {
            self.queue_status = Some("Open a folder or image first to use Folder Masonry.".into());
            return;
        };
        self.snapshot_active_viewer_tab();
        if let Some(index) = self.workspace.tabs.iter().position(|tab| {
            tab.kind == WorkspaceTabKind::Masonry && tab.folder_masonry && tab.directory.as_deref() == Some(directory.as_path())
        }) {
            self.workspace.active_index = index;
            self.stop_preloading_for_workspace();
            return;
        }
        let paths = crate::imaging::loader::load_directory_images(&directory, self.sort_method);
        let entries = paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
        // Keep an empty Folder Masonry workspace available so the user can still
        // enable recursive browsing when images exist only in subfolders.
        let id = self.workspace.allocate_id();
        let mut tab = WorkspaceTab::new(id, "Masonry · Folder".into(), WorkspaceTabKind::Masonry);
        tab.entries = entries;
        tab.directory = Some(directory);
        tab.masonry_tile_size = self.settings_manager.get().masonry_thumbnail_size.clamp(76.0, 512.0);
        tab.folder_masonry = true;
        self.workspace.tabs.push(tab);
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
    }

    pub(super) fn stop_preloading_for_workspace(&mut self) {
        self.should_stop_caching = true;
        self.preload_tasks.clear();
        self.preloading_indices.clear();
        self.preload_skipped.clear();
        self.preload_workers = 0;
        self.preload_working = false;
        self.preload_generation = self.preload_generation.wrapping_add(1);
    }

    /// Masonry-specific conventional shortcuts take precedence over the viewer's
    /// global shortcuts while the Masonry workspace is active.
    pub(crate) fn handle_workspace_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.workspace.active_kind() != WorkspaceTabKind::Masonry {
            return false;
        }
        let (escape, select_all, delete) = ctx.input(|input| (
            input.key_pressed(egui::Key::Escape),
            input.key_pressed(egui::Key::A) && input.modifiers.ctrl,
            input.key_pressed(egui::Key::Delete),
        ));
        let Some(index) = self.workspace.tabs.get(self.workspace.active_index)
            .map(|tab| self.workspace.tabs.iter().position(|candidate| candidate.id == tab.id).unwrap_or(self.workspace.active_index))
        else { return false; };

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
            tab.selected = tab.entries.iter().filter(|entry| {
                let name = entry_display_name(entry).to_lowercase();
                let extension = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
                (search.is_empty() || name.contains(&search))
                    && (extension_filter.is_empty() || extension == extension_filter)
            }).map(ImageEntry::get_id).collect();
            return true;
        }
        if delete {
            let selected = self.workspace.tabs[index].selected.len();
            if selected == 0 {
                self.queue_status = Some("Select one or more entries before pressing Delete.".into());
            } else {
                self.handle_masonry_action(index, MasonryAction::Delete, ctx);
            }
            return true;
        }
        false
    }
}
