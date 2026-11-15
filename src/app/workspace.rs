//! Workspace tabs, Masonry browsing, and image comparison surfaces.
//!
//! The original ViewerApp remains the renderer/decoder for the primary image
//! viewer. This module owns the independent workspace state and only swaps the
//! active viewer collection when an image tab is selected.

use crate::imaging::entry::ImageEntry;
use crate::app::types::LoadedImage;
use crate::app::virtual_texture::VirtualTexture;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceTabKind {
    Viewer,
    Image,
    Masonry,
    Compare,
    Welcome,
}

pub(crate) struct WorkspaceTab {
    pub id: u64,
    pub title: String,
    pub kind: WorkspaceTabKind,
    /// Viewer/Image tabs store their collection snapshot here. Masonry and
    /// Compare tabs store their own entries here.
    pub entries: Vec<ImageEntry>,
    pub current_index: usize,
    pub directory: Option<PathBuf>,
    pub folder_masonry: bool,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub fit_to_window: bool,
    pub zoom_used: bool,

    // Masonry state.
    pub recursive: bool,
    pub selection_mode: bool,
    pub selected: HashSet<String>,
    pub search: String,
    pub extension_filter: String,
    /// A right-click highlight is visual context only; it is not part of the
    /// multi-selection set and does not activate selection mode.
    pub context_highlight: Option<String>,
    /// Preferred Masonry tile width in logical points; columns are auto-filled.
    pub masonry_tile_size: f32,

    // Compare state. Anchor coordinates are normalized image coordinates.
    pub anchors: Vec<Option<[f32; 2]>>,
    pub anchors_second: Vec<Option<[f32; 2]>>,
    pub offsets: Vec<[f32; 2]>,
    /// Pane currently awaiting anchor placement.
    pub anchor_editing: Option<usize>,
    /// 0 = first anchor, 1 = second anchor.
    pub anchor_editing_point: usize,
    pub show_anchors: bool,
    pub sync_pan: bool,
    pub sync_zoom: bool,
    pub sync_navigation: bool,
    pub pane_zoom: Vec<f32>,
    /// Whether each pane participates as a target in synchronized pan/zoom.
    pub pane_sync: Vec<bool>,
    /// Per-pane width/height factors; 1.0 means the adaptive default size.
    pub pane_size: Vec<[f32; 2]>,
    /// Legacy saved column count retained for old session files.
    pub compare_columns: usize,
    /// Compare panes are arranged either as side-by-side columns or full-width rows.
    pub compare_rows: bool,
}

impl WorkspaceTab {
    fn viewer() -> Self {
        Self {
            id: 1,
            title: "Viewer".to_owned(),
            kind: WorkspaceTabKind::Viewer,
            entries: Vec::new(),
            current_index: 0,
            directory: None,
            folder_masonry: false,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            fit_to_window: true,
            zoom_used: false,
            recursive: false,
            selection_mode: false,
            selected: HashSet::new(),
            search: String::new(),
            extension_filter: String::new(),
            context_highlight: None,
            masonry_tile_size: 220.0,
            anchors: Vec::new(),
            anchors_second: Vec::new(),
            offsets: Vec::new(),
            anchor_editing: None,
            anchor_editing_point: 0,
            show_anchors: true,
            sync_pan: true,
            sync_zoom: true,
            sync_navigation: false,
            pane_zoom: Vec::new(),
            pane_sync: Vec::new(),
            pane_size: Vec::new(),
            compare_columns: 0,
            compare_rows: false,
        }
    }

    fn welcome(id: u64) -> Self {
        Self::new(id, "Welcome".to_owned(), WorkspaceTabKind::Welcome)
    }

    fn new(id: u64, title: String, kind: WorkspaceTabKind) -> Self {
        let mut tab = Self::viewer();
        tab.id = id;
        tab.title = title;
        tab.kind = kind;
        tab
    }
}

pub(crate) struct WorkspaceManager {
    pub tabs: Vec<WorkspaceTab>,
    pub active_index: usize,
    pub next_id: u64,
    pub last_image_tab_id: Option<u64>,
    /// Existing comparison workspace awaiting additional entries from Masonry.
    pending_compare_target_id: Option<u64>,
    pub delete_confirmation: Option<(u64, Vec<ImageEntry>, usize)>,
    pending_file_operation: Option<PendingFileOperation>,
    pub session_checked: bool,
    pub session_prompt: bool,
    pub pending_session: Option<RestorableSession>,
    /// Tab currently being dragged in the browser-style workspace strip.
    dragging_tab_id: Option<u64>,
    /// Full-resolution comparison images rendered through the same tiled texture system as the main viewer.
    compare_images: HashMap<String, VirtualTexture>,
    compare_pending: HashSet<String>,
    compare_failed: HashSet<String>,
    compare_receivers: Vec<Receiver<(String, Result<VirtualTexture, String>)>>,
    /// Recently opened filesystem images and folders displayed on Welcome tabs.
    pub(crate) welcome_recent: Vec<PathBuf>,
    /// User-pinned images, folders, and .jvq playlists displayed on Welcome tabs.
    pub(crate) welcome_pinned: Vec<PathBuf>,
    welcome_data_loaded: bool,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self {
            tabs: vec![WorkspaceTab::viewer(), WorkspaceTab::welcome(2)],
            active_index: 1,
            next_id: 3,
            last_image_tab_id: None,
            pending_compare_target_id: None,
            delete_confirmation: None,
            pending_file_operation: None,
            session_checked: false,
            session_prompt: false,
            pending_session: None,
            dragging_tab_id: None,
            compare_images: HashMap::new(),
            compare_pending: HashSet::new(),
            compare_failed: HashSet::new(),
            compare_receivers: Vec::new(),
            welcome_recent: Vec::new(),
            welcome_pinned: Vec::new(),
            welcome_data_loaded: false,
        }
    }
}

impl WorkspaceManager {
    pub fn active_kind(&self) -> WorkspaceTabKind {
        self.tabs
            .get(self.active_index)
            .map(|tab| tab.kind)
            .unwrap_or(WorkspaceTabKind::Viewer)
    }

    pub fn should_preload(&self) -> bool {
        matches!(self.active_kind(), WorkspaceTabKind::Viewer | WorkspaceTabKind::Image)
    }

    fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tabs.get(self.active_index)
    }

    fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.tabs.get_mut(self.active_index)
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(2);
        id
    }

    pub(crate) fn active_tab_title(&self) -> Option<&str> {
        self.active_tab().map(|tab| tab.title.as_str())
    }
}

impl crate::app::types::ViewerApp {
    /// Snapshot the collection-based viewer before switching to another tab.
    fn snapshot_active_viewer_tab(&mut self) {
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

    fn open_masonry_collection(
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

    fn stop_preloading_for_workspace(&mut self) {
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

    /// Switch ordinary file-open actions back to the collection-based Viewer tab.
    /// Explicitly opening a new path must take precedence over a restored Masonry
    /// or comparison workspace rather than silently changing an invisible viewer.
    pub(crate) fn activate_primary_viewer_tab(&mut self) {
        let Some(id) = self.workspace.tabs.iter()
            .find(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| self.workspace.tabs.iter().find(|tab| tab.kind == WorkspaceTabKind::Viewer))
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
                && tab.entries.first().is_some_and(|existing| existing.get_id() == entry_id)
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
                WorkspaceTabKind::Masonry | WorkspaceTabKind::Compare => self.workspace.last_image_tab_id,
                WorkspaceTabKind::Viewer | WorkspaceTabKind::Welcome => None,
            }
        } else {
            None
        };

        if let Some(id) = reuse_id {
            if let Some(index) = self.workspace.tabs.iter().position(|tab| {
                tab.id == id && tab.kind == WorkspaceTabKind::Image
            }) {
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
            if let Some(index) = self.workspace.tabs.iter().position(|tab| {
                tab.id == target_id && tab.kind == WorkspaceTabKind::Compare
            }) {
                if entries.is_empty() {
                    self.queue_status = Some("Select an image in Masonry to add it to Compare.".into());
                    self.workspace.active_index = index;
                    self.stop_preloading_for_workspace();
                    return;
                }
                let tab = &mut self.workspace.tabs[index];
                let mut known = tab.entries.iter().map(ImageEntry::get_id).collect::<HashSet<_>>();
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
        let mut tab = WorkspaceTab::new(id, format!("Compare ({})", entries.len()), WorkspaceTabKind::Compare);
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
        if self.workspace.tabs[index].kind == WorkspaceTabKind::Viewer && self.workspace.tabs[index].id == 1 {
            return;
        }
        let closing_active_tab = self.workspace.active_index == index;
        if self.workspace.pending_compare_target_id == Some(id) {
            self.workspace.pending_compare_target_id = None;
        }
        self.workspace.tabs.remove(index);
        if self.workspace.last_image_tab_id == Some(id) {
            self.workspace.last_image_tab_id = self.workspace.tabs.iter().rev()
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

    pub(crate) fn render_workspace_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.handle_workspace_tab_shortcuts(ctx);
        let only_viewer_tab = self.workspace.tabs.len() == 1
            && self.workspace.tabs.first().is_some_and(|tab| tab.kind == WorkspaceTabKind::Viewer);
        if only_viewer_tab {
            return;
        }
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        if !fullscreen {
            self.render_workspace_tab_strip(ui, ctx, self.main_view_width(ctx));
        }
    }

    /// In fullscreen, tabs float above the image and other viewer overlays. They
    /// reserve no central-panel height and disappear together with hidden overlays.
    pub(crate) fn render_workspace_tabs_overlay(&mut self, ctx: &egui::Context) {
        let only_viewer_tab = self.workspace.tabs.len() == 1
            && self.workspace.tabs.first().is_some_and(|tab| tab.kind == WorkspaceTabKind::Viewer);
        if only_viewer_tab
            || !ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
            || !self.overlay_visible
        {
            return;
        }
        let central_rect = self.main_view_rect(ctx);
        let strip_width = (self.main_view_width(ctx) - 16.0).max(0.0);
        if strip_width < 96.0 {
            return;
        }
        egui::Area::new(egui::Id::new("workspace_tabs_fullscreen_overlay"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::pos2(central_rect.left() + 8.0, central_rect.top() + 8.0))
            .show(ctx, |ui| {
                ui.set_min_width(strip_width);
                ui.set_max_width(strip_width);
                self.render_workspace_tab_strip(ui, ctx, strip_width);
            });
    }

    fn handle_workspace_tab_shortcuts(&mut self, ctx: &egui::Context) {
        // Browser-style keyboard navigation/reordering. Do not steal shortcuts
        // while the user is typing in a search field or another text editor.
        if ctx.wants_keyboard_input() {
            return;
        }
        let open_welcome = ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::T));
        if open_welcome {
            self.open_new_welcome_tab();
            return;
        }
        let tab_action = ctx.input(|input| {
            let ctrl = input.modifiers.ctrl;
            let shift = input.modifiers.shift;
            if ctrl && shift && (input.key_pressed(egui::Key::PageUp)
                || input.key_pressed(egui::Key::ArrowLeft)) {
                Some(-2_i8)
            } else if ctrl && shift && (input.key_pressed(egui::Key::PageDown)
                || input.key_pressed(egui::Key::ArrowRight)) {
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

    fn render_workspace_tab_strip(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, strip_width: f32) {
        // Allocate exactly the CentralPanel width. In particular, do not ask the
        // Frame's inner UI for the full outer width: its horizontal margins would
        // otherwise make the tab row wider than the space between sidebars.
        let strip_width = strip_width.min(ui.available_width().max(0.0)).max(0.0);
        let descriptors = self.workspace.tabs.iter()
            .map(|tab| (tab.id, tab.title.clone(), tab.kind, tab.id == self.workspace.tabs[self.workspace.active_index.min(self.workspace.tabs.len() - 1)].id))
            .collect::<Vec<_>>();
        let mut activate = None;
        let mut close = None;
        let mut open_welcome = false;
        let mut open_local = false;
        let mut open_folder = false;
        let mut tab_rects: Vec<(u64, egui::Rect)> = Vec::with_capacity(descriptors.len());

        let settings = self.settings_manager.get().clone();
        let viewer_surface = crate::ui_theme::main_view_surface_color(
            ctx,
            &settings.main_view_background,
            settings.main_view_background_opacity,
        );
        let look_profile = crate::ui_theme::current_look_profile(ctx);
        let tab_rounding = if look_profile == "win98" {
            egui::CornerRadius::ZERO
        } else if look_profile == "cosmic" || look_profile == "gnome" {
            egui::CornerRadius::same(9)
        } else {
            egui::CornerRadius::same(6)
        };
        let strip_height = 40.0;
        ui.allocate_ui_with_layout(
            egui::vec2(strip_width, strip_height),
            egui::Layout::top_down(egui::Align::Min),
            |slot| {
                slot.set_min_width(strip_width);
                slot.set_max_width(strip_width);
                egui::Frame::new()
                    .fill(viewer_surface)
                    .inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 4 })
                    .show(slot, |body| {
                        let body_width = (strip_width - 12.0).max(0.0);
                        body.set_min_width(body_width);
                        body.set_max_width(body_width);
                        body.horizontal(|row| {
                            let available = row.available_width().max(0.0);
                            // Keep creation controls in a dedicated, clipped region
                            // at the right. Only the tab list scrolls; actions never
                            // expand the strip beyond the width left by docked panels.
                            let action_width = 76.0 + 58.0 + 64.0 + 4.0 * 2.0;
                            let has_tab_viewport = available > action_width + 4.0;
                            let actions_viewport_width = if has_tab_viewport { action_width } else { available };
                            let tab_scroll_width = if has_tab_viewport { available - action_width - 4.0 } else { 0.0 };
                            row.spacing_mut().item_spacing.x = if has_tab_viewport { 4.0 } else { 0.0 };

                            row.allocate_ui_with_layout(
                                egui::vec2(tab_scroll_width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |scroll_host| {
                                    scroll_host.set_min_width(tab_scroll_width);
                                    scroll_host.set_max_width(tab_scroll_width);
                                    egui::ScrollArea::horizontal()
                                        .id_salt("workspace_firefox_tab_strip")
                                        .auto_shrink([false, true])
                                        .max_width(tab_scroll_width)
                                        .max_height(32.0)
                                        .show(scroll_host, |tabs_ui| {
                                            tabs_ui.spacing_mut().item_spacing.x = 4.0;
                                            tabs_ui.horizontal(|tabs_ui| {
                                                for (id, title, kind, active) in descriptors {
                                                    let icon = match kind {
                                                        WorkspaceTabKind::Viewer => "▣",
                                                        WorkspaceTabKind::Image => "▧",
                                                        WorkspaceTabKind::Masonry => "▦",
                                                        WorkspaceTabKind::Compare => "⇄",
                                                        WorkspaceTabKind::Welcome => "✦",
                                                    };
                                                    let display_title = if title.chars().count() > 20 {
                                                        format!("{}…", title.chars().take(19).collect::<String>())
                                                    } else {
                                                        title.clone()
                                                    };
                                                    let width = (display_title.chars().count() as f32 * 7.2 + 64.0)
                                                        .clamp(112.0, 210.0);
                                                    let (rect, response) = tabs_ui.allocate_exact_size(
                                                        egui::vec2(width, 32.0),
                                                        egui::Sense::click_and_drag(),
                                                    );
                                                    let visuals = tabs_ui.visuals();
                                                    let fill = if active {
                                                        visuals.selection.bg_fill
                                                    } else if response.hovered() {
                                                        visuals.widgets.hovered.bg_fill
                                                    } else {
                                                        viewer_surface
                                                    };
                                                    let stroke = if active {
                                                        visuals.selection.stroke
                                                    } else {
                                                        visuals.widgets.inactive.bg_stroke
                                                    };
                                                    tabs_ui.painter().rect_filled(rect, tab_rounding, fill);
                                                    tabs_ui.painter().rect_stroke(
                                                        rect,
                                                        tab_rounding,
                                                        stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    let close_rect = egui::Rect::from_center_size(
                                                        egui::pos2(rect.right() - 16.0, rect.center().y),
                                                        egui::vec2(22.0, 22.0),
                                                    );
                                                    tabs_ui.painter().text(
                                                        egui::pos2(rect.left() + 10.0, rect.center().y),
                                                        egui::Align2::LEFT_CENTER,
                                                        format!("{icon}  {display_title}"),
                                                        egui::FontId::proportional(13.0),
                                                        if active { visuals.text_color() } else { visuals.weak_text_color() },
                                                    );
                                                    if kind != WorkspaceTabKind::Viewer || id != 1 {
                                                        let close_hovered = response.hovered()
                                                            || ctx.pointer_hover_pos().is_some_and(|pos| close_rect.contains(pos));
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
                                                        if (kind != WorkspaceTabKind::Viewer || id != 1)
                                                            && response.interact_pointer_pos().is_some_and(|pos| close_rect.contains(pos))
                                                        {
                                                            close = Some(id);
                                                        } else {
                                                            activate = Some(id);
                                                        }
                                                    }
                                                    if response.drag_started()
                                                        && response.interact_pointer_pos().is_some_and(|pos| !close_rect.contains(pos))
                                                    {
                                                        self.workspace.dragging_tab_id = Some(id);
                                                    }
                                                    if response.hovered() && self.workspace.dragging_tab_id.is_some() {
                                                        tabs_ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::Grabbing);
                                                    }
                                                    tab_rects.push((id, rect));
                                                }
                                            });
                                        });
                                },
                            );

                            row.allocate_ui_with_layout(
                                egui::vec2(actions_viewport_width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |actions_host| {
                                    actions_host.set_min_width(actions_viewport_width);
                                    actions_host.set_max_width(actions_viewport_width);
                                    egui::ScrollArea::horizontal()
                                        .id_salt("workspace_tab_creation_actions")
                                        .auto_shrink([false, true])
                                        .max_width(actions_viewport_width)
                                        .max_height(32.0)
                                        .show(actions_host, |actions| {
                                            actions.horizontal(|actions| {
                                                actions.spacing_mut().item_spacing.x = 4.0;
                                                if actions.add_sized([76.0, 28.0], egui::Button::new("＋ Welcome"))
                                                    .on_hover_text("Open a new Welcome tab")
                                                    .clicked()
                                                {
                                                    open_welcome = true;
                                                }
                                                if actions.add_sized([58.0, 28.0], egui::Button::new("＋ Local"))
                                                    .on_hover_text("Open Local Masonry")
                                                    .clicked()
                                                {
                                                    open_local = true;
                                                }
                                                if actions.add_sized([64.0, 28.0], egui::Button::new("＋ Folder"))
                                                    .on_hover_text("Open Folder Masonry")
                                                    .clicked()
                                                {
                                                    open_folder = true;
                                                }
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
                    if let Some((target_id, target_rect)) = tab_rects.iter()
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
        } else if open_local {
            self.open_local_masonry();
        } else if open_folder {
            self.open_folder_masonry();
        }
    }

    fn move_workspace_tab_before(&mut self, moving_id: u64, target_id: u64, before: bool) {
        let Some(from) = self.workspace.tabs.iter().position(|tab| tab.id == moving_id) else { return; };
        let Some(target) = self.workspace.tabs.iter().position(|tab| tab.id == target_id) else { return; };
        let active_id = self.workspace.tabs.get(self.workspace.active_index).map(|tab| tab.id);
        let tab = self.workspace.tabs.remove(from);
        let target_after_remove = self.workspace.tabs.iter().position(|candidate| candidate.id == target_id).unwrap_or(target);
        let insert_at = if before { target_after_remove } else { target_after_remove + 1 };
        self.workspace.tabs.insert(insert_at.min(self.workspace.tabs.len()), tab);
        if let Some(active_id) = active_id {
            self.workspace.active_index = self.workspace.tabs.iter().position(|candidate| candidate.id == active_id).unwrap_or(0);
        }
        // Retain the moving ID as a drag source across frames.
        self.workspace.dragging_tab_id = Some(moving_id);
    }

    fn move_active_workspace_tab(&mut self, delta: isize) {
        let Some(tab) = self.workspace.tabs.get(self.workspace.active_index).map(|tab| tab.id) else { return; };
        let target_index = self.workspace.active_index.saturating_add_signed(delta)
            .min(self.workspace.tabs.len().saturating_sub(1));
        let Some(target_id) = self.workspace.tabs.get(target_index).map(|tab| tab.id) else { return; };
        self.move_workspace_tab_before(tab, target_id, delta < 0);
    }

    fn welcome_data_path(&self) -> PathBuf {
        self.settings_manager.path().with_file_name("welcome_links.txt")
    }

    fn ensure_welcome_data_loaded(&mut self) {
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
        self.workspace.welcome_recent.truncate(12);
        self.workspace.welcome_pinned.truncate(24);
    }

    fn save_welcome_data(&self) {
        let path = self.welcome_data_path();
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("Could not create Welcome data directory: {error}");
                return;
            }
        }
        let mut output = String::new();
        for item in self.workspace.welcome_recent.iter().take(12) {
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
        self.ensure_welcome_data_loaded();
        if self.workspace.welcome_recent.first().is_some_and(|first| welcome_paths_equal(first, &path)) {
            return;
        }
        self.workspace.welcome_recent.retain(|existing| !welcome_paths_equal(existing, &path));
        self.workspace.welcome_recent.insert(0, path);
        self.workspace.welcome_recent.truncate(12);
        self.save_welcome_data();
    }

    pub(crate) fn add_welcome_pin(&mut self, path: PathBuf) {
        self.ensure_welcome_data_loaded();
        if !self.workspace.welcome_pinned.iter().any(|existing| welcome_paths_equal(existing, &path)) {
            self.workspace.welcome_pinned.push(path);
            self.workspace.welcome_pinned.truncate(24);
            self.save_welcome_data();
        }
    }

    pub(crate) fn remove_welcome_pin(&mut self, path: &Path) {
        self.ensure_welcome_data_loaded();
        self.workspace.welcome_pinned.retain(|existing| !welcome_paths_equal(existing, path));
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
            self.queue_status = Some("Welcome shortcuts support image files, folders, and .jvq playlists.".into());
            return;
        }
        let paths = match crate::app::queues::read_queue(&path) {
            Ok((paths, _)) => paths,
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

    pub(crate) fn load_favorites_from_welcome(&mut self, ctx: &egui::Context) {
        self.ensure_favorites_loaded();
        let paths = self.favorite_paths.clone();
        if paths.is_empty() {
            self.queue_status = Some("Your Favorites list is empty.".into());
            return;
        }
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            let expanded = if path.is_dir() {
                crate::app::queues::collect_folder_images(&path)
            } else if path.is_file() && crate::imaging::helpers::is_supported_image(&path) {
                vec![path]
            } else {
                Vec::new()
            };
            for image in expanded {
                let entry = ImageEntry::File(image);
                if seen.insert(entry.get_id()) { entries.push(entry); }
            }
        }
        if entries.is_empty() {
            self.queue_status = Some("No available supported images were found in Favorites.".into());
            return;
        }
        if !self.prepare_for_image_change("Save or discard unsaved edits before opening Favorites.") {
            return;
        }
        self.activate_primary_viewer_tab();
        let directory = entries.first().and_then(entry_parent);
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

    pub(crate) fn open_new_welcome_tab(&mut self) {
        self.snapshot_active_viewer_tab();
        let id = self.workspace.allocate_id();
        self.workspace.tabs.push(WorkspaceTab::welcome(id));
        self.workspace.active_index = self.workspace.tabs.len() - 1;
        self.stop_preloading_for_workspace();
    }

    pub(crate) fn active_workspace_is_masonry(&self) -> bool {
        self.workspace.active_kind() == WorkspaceTabKind::Masonry
    }

    pub(crate) fn active_workspace_hides_image_navigation(&self) -> bool {
        matches!(
            self.workspace.active_kind(),
            WorkspaceTabKind::Masonry | WorkspaceTabKind::Compare | WorkspaceTabKind::Welcome
        )
    }

    /// Returns true when a non-viewer workspace rendered the central surface.
    pub(crate) fn render_active_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        match self.workspace.active_kind() {
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image => false,
            WorkspaceTabKind::Masonry => {
                self.render_masonry_workspace(ui, ctx);
                true
            }
            WorkspaceTabKind::Compare => {
                self.render_compare_workspace(ui, ctx);
                true
            }
            WorkspaceTabKind::Welcome => {
                if !self.workspace.session_prompt {
                    self.render_welcome_ui(ctx, ui);
                }
                true
            }
        }
    }

    fn render_masonry_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.collect_sidebar_thumbnails(ctx);
        let Some(tab_index) = self.workspace.tabs.get(self.workspace.active_index)
            .map(|tab| self.workspace.tabs.iter().position(|candidate| candidate.id == tab.id).unwrap_or(self.workspace.active_index))
        else { return; };

        let mut action: Option<MasonryAction> = None;
        let mut reload_folder = false;
        let mut selected_sort_method = self.sort_method;
        let previous_tile_size = self.workspace.tabs[tab_index].masonry_tile_size.clamp(76.0, 512.0);
        let mut tile_size = previous_tile_size;
        let mut persist_tile_size = false;
        let tab_id = self.workspace.tabs[tab_index].id;
        let selection_mode = self.workspace.tabs[tab_index].selection_mode;
        let has_pending_compare_target = self.workspace.pending_compare_target_id.is_some();

        // Masonry is a bottom-docked workspace: the image grid owns the flexible
        // area and scrolls independently, while the themed two/three-row toolbar
        // stays anchored to the bottom edge of the central view.
        let layout_rect = ui.available_rect_before_wrap()
            .intersect(ui.clip_rect())
            .intersect(self.main_view_rect(ctx));
        if layout_rect.width() < 80.0 || layout_rect.height() < 40.0 {
            return;
        }
        let footer_height = if selection_mode { 112.0 } else { 82.0 };
        let footer_bottom = layout_rect.bottom() - 8.0;
        let footer_top = (footer_bottom - footer_height).max(layout_rect.top() + 24.0);
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, footer_top),
            egui::pos2((layout_rect.right() - 8.0).max(layout_rect.left() + 8.0), footer_bottom),
        );
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(layout_rect.left() + 8.0, layout_rect.top() + 6.0),
            egui::pos2(
                (layout_rect.right() - 8.0).max(layout_rect.left() + 8.0),
                (footer_top - 6.0).max(layout_rect.top() + 6.0),
            ),
        );

        // The content scroll area is constrained to the actual remaining CentralPanel
        // rect, which already accounts for both docked sidebars. The scrollbar is
        // widened and kept inside that rect; no outer-window subtraction is needed.
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |content_ui| {
                {
                    let scroll_style = &mut content_ui.spacing_mut().scroll;
                    scroll_style.floating = false;
                    scroll_style.bar_width = 14.0;
                    scroll_style.handle_min_length = 32.0;
                    scroll_style.bar_inner_margin = 4.0;
                    scroll_style.bar_outer_margin = 1.0;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("masonry_scroll", tab_id))
                    .auto_shrink([false, false])
                    .show(content_ui, |scroll_ui| {
                        let viewport_width = scroll_ui.available_width().max(1.0);
                        scroll_ui.set_min_width(viewport_width);
                        scroll_ui.set_max_width(viewport_width);
                        scroll_ui.add_space(16.0);
                        scroll_ui.horizontal(|outer| {
                            outer.spacing_mut().item_spacing.x = 0.0;
                            outer.add_space(16.0);
                            outer.vertical(|grid_ui| {
                                let content_width = (viewport_width - 32.0).max(1.0);
                                grid_ui.set_min_width(content_width);
                                grid_ui.set_max_width(content_width);

                                let (entries, selected, search, extension_filter, folder_masonry, recursive, directory, preferred_size) = {
                                    let tab = &self.workspace.tabs[tab_index];
                                    (
                                        tab.entries.clone(),
                                        tab.selected.clone(),
                                        tab.search.to_lowercase(),
                                        tab.extension_filter.to_lowercase(),
                                        tab.folder_masonry,
                                        tab.recursive,
                                        tab.directory.clone(),
                                        tab.masonry_tile_size,
                                    )
                                };
                                let filtered = entries.into_iter().filter(|entry| {
                                    let name = entry_display_name(entry).to_lowercase();
                                    let ext = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
                                    (search.is_empty() || name.contains(&search))
                                        && (extension_filter.is_empty() || ext == extension_filter)
                                }).collect::<Vec<_>>();

                                if filtered.is_empty() {
                                    grid_ui.allocate_ui_with_layout(
                                        egui::vec2(content_width, (content_rect.height() - 32.0).max(60.0)),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |empty_ui| {
                                            empty_ui.add_space((content_rect.height() * 0.25).max(20.0));
                                            empty_ui.label(if self.workspace.tabs[tab_index].entries.is_empty() {
                                                "No supported images in this collection."
                                            } else {
                                                "No images match the current filters."
                                            });
                                        },
                                    );
                                } else {
                                    let headers_enabled = self.settings_manager.get().masonry_show_folder_headers;
                                    // Recursive Folder Masonry always benefits from folder sections.
                                    // Local collections also get grouped when they contain images
                                    // from more than one source folder (or archive subfolder), so
                                    // the user can tell where each entry belongs while scrolling.
                                    let distinct_folders = filtered
                                        .iter()
                                        .map(|entry| entry_breadcrumb(entry, directory.as_deref()))
                                        .collect::<HashSet<_>>();
                                    let show_folder_headers = headers_enabled
                                        && ((folder_masonry && recursive) || distinct_folders.len() > 1);
                                    let mut groups: Vec<(String, Vec<ImageEntry>)> = Vec::new();
                                    if show_folder_headers {
                                        for entry in filtered {
                                            let label = entry_breadcrumb(&entry, directory.as_deref());
                                            if let Some((_, group_entries)) = groups.iter_mut().find(|(name, _)| *name == label) {
                                                group_entries.push(entry);
                                            } else {
                                                groups.push((label, vec![entry]));
                                            }
                                        }
                                    } else {
                                        groups.push((String::new(), filtered));
                                    }

                                    for (folder_label, group_entries) in groups {
                                        if !folder_label.is_empty() {
                                            let header_width = grid_ui.available_width();
                                            egui::Frame::group(grid_ui.style())
                                                .fill(grid_ui.visuals().faint_bg_color)
                                                .inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 })
                                                .show(grid_ui, |header_ui| {
                                                    header_ui.set_min_width((header_width - 20.0).max(0.0));
                                                    header_ui.horizontal(|row| {
                                                        row.label(egui::RichText::new(format!("📁 {folder_label}")).strong());
                                                        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
                                                            row.label(egui::RichText::new(format!("{} images", group_entries.len())).small().weak());
                                                        });
                                                    });
                                                });
                                            grid_ui.add_space(8.0);
                                        }
                                        self.render_masonry_group(
                                            grid_ui,
                                            tab_index,
                                            tab_id,
                                            &group_entries,
                                            &selected,
                                            preferred_size,
                                            ctx,
                                            &mut action,
                                        );
                                        grid_ui.add_space(14.0);
                                    }
                                }
                            });
                            outer.add_space(16.0);
                        });
                        scroll_ui.add_space(16.0);
                    });
            },
        );

        // Look-aware bottom dock. Normal mode is exactly two rows; selection mode
        // adds only one third row for bulk selection and file-management actions.
        let settings = self.settings_manager.get().clone();
        let surface = crate::ui_theme::main_view_surface_color(
            ctx,
            &settings.main_view_background,
            settings.main_view_background_opacity,
        );
        let profile = crate::ui_theme::current_look_profile(ctx);
        let radius = match profile {
            "win98" => 0,
            "cosmic" | "gnome" => 10,
            _ => 14,
        };
        let dock_stroke = if matches!(profile, "cosmic" | "gnome" | "win98") {
            ui.visuals().widgets.noninteractive.bg_stroke
        } else {
            egui::Stroke::NONE
        };

        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(footer_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |footer_ui| {
                egui::Frame::new()
                    .fill(surface)
                    .stroke(dock_stroke)
                    .corner_radius(egui::CornerRadius::same(radius))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(footer_ui, |dock| {
                        dock.spacing_mut().item_spacing.y = 3.0;
                        let tab = &mut self.workspace.tabs[tab_index];

                        // Row 1: search/filter/sort/size controls and the Select
                        // toggle share one line. The toggle stays pinned at the right;
                        // controls scroll horizontally when the central view is narrow.
                        dock.horizontal(|row| {
                            let select_width = 116.0;
                            let controls_width = (row.available_width() - select_width - 8.0).max(0.0);
                            egui::ScrollArea::horizontal()
                                .id_salt(("masonry_controls_row", tab.id))
                                .auto_shrink([false, true])
                                .max_width(controls_width)
                                .max_height(28.0)
                                .show(row, |controls| {
                                    controls.horizontal(|controls| {
                                        controls.add_sized(
                                            egui::vec2(150.0, 22.0),
                                            egui::TextEdit::singleline(&mut tab.search).hint_text("Search images"),
                                        );
                                        egui::ComboBox::from_id_salt(("masonry_format", tab.id))
                                            .width(100.0)
                                            .selected_text(if tab.extension_filter.is_empty() { "All formats" } else { &tab.extension_filter })
                                            .show_ui(controls, |menu| {
                                                menu.selectable_value(&mut tab.extension_filter, String::new(), "All formats");
                                                for ext in crate::imaging::core::ImageFormat::all_extensions() {
                                                    menu.selectable_value(&mut tab.extension_filter, ext.clone(), ext.to_uppercase());
                                                }
                                            });
                                        controls.separator();
                                        egui::ComboBox::from_id_salt(("masonry_sort", tab.id))
                                            .width(132.0)
                                            .selected_text(sort_method_label(selected_sort_method))
                                            .show_ui(controls, |menu| {
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::Natural, "Name (natural)");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::DateNewest, "Newest first");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::DateOldest, "Oldest first");
                                                menu.selectable_value(&mut selected_sort_method, crate::imaging::sort::SortMethod::Shuffle, "Random");
                                            });
                                        controls.separator();
                                        controls.label("Size");
                                        let slider_width = (controls.available_width() * 0.20).clamp(90.0, 150.0);
                                        let size_response = controls.add_sized(
                                            egui::vec2(slider_width, 20.0),
                                            egui::Slider::new(&mut tile_size, 76.0..=512.0).step_by(4.0).show_value(true),
                                        );
                                        persist_tile_size = size_response.drag_stopped()
                                            || (size_response.changed() && !size_response.dragged());
                                        if tab.folder_masonry {
                                            controls.checkbox(&mut tab.recursive, "Subfolders");
                                            if controls.small_button("↻").on_hover_text("Refresh folder contents").clicked() {
                                                reload_folder = true;
                                            }
                                        }
                                    });
                                });
                            row.add_space(4.0);
                            if row.add_sized(
                                [select_width, 26.0],
                                egui::Button::new(if tab.selection_mode { "Finish selecting" } else { "Select" }),
                            ).clicked() {
                                tab.selection_mode = !tab.selection_mode;
                                if !tab.selection_mode {
                                    tab.selected.clear();
                                }
                            }
                        });

                        // Row 2: workspace/folder context on the left and the
                        // selected count directly below the Select button on the right.
                        dock.horizontal(|row| {
                            row.label(egui::RichText::new(tab.title.clone()).strong());
                            if let Some(directory_name) = tab.directory.as_ref().and_then(|path| path.file_name()) {
                                row.label(egui::RichText::new(directory_name.to_string_lossy()).small().weak());
                            }
                            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                                if selection_mode {
                                    right.strong(format!("{} selected", tab.selected.len()));
                                } else {
                                    right.label(format!("{} images", tab.entries.len()));
                                }
                            });
                        });

                        // Row 3 exists only while selection mode is active. Use the
                        // frame-start state so the footer height and row count always
                        // agree; toggles take effect on the immediately following frame.
                        if selection_mode {
                            dock.horizontal(|row| {
                                egui::ScrollArea::horizontal()
                                    .id_salt(("masonry_selection_actions", tab.id))
                                    .auto_shrink([false, true])
                                    .max_height(28.0)
                                    .show(row, |actions| {
                                        actions.horizontal(|actions| {
                                            if actions.button("Select all visible").clicked() {
                                                let search = tab.search.to_lowercase();
                                                let extension_filter = tab.extension_filter.to_lowercase();
                                                tab.selected = tab.entries.iter().filter(|entry| {
                                                    let name = entry_display_name(entry).to_lowercase();
                                                    let extension = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
                                                    (search.is_empty() || name.contains(&search))
                                                        && (extension_filter.is_empty() || extension == extension_filter)
                                                }).map(ImageEntry::get_id).collect();
                                            }
                                            if actions.button("Clear").clicked() { tab.selected.clear(); }
                                            if actions.button("View selected").clicked() { action = Some(MasonryAction::ViewSelected); }
                                            if actions.button("Open in new tabs").clicked() { action = Some(MasonryAction::OpenSelected); }
                                            let compare_minimum = if has_pending_compare_target { 1 } else { 2 };
                                            let compare_label = if has_pending_compare_target { "Add to Compare" } else { "Compare" };
                                            if actions.add_enabled(tab.selected.len() >= compare_minimum, egui::Button::new(compare_label)).clicked() {
                                                action = Some(MasonryAction::Compare);
                                            }
                                            if actions.add_enabled(!tab.selected.is_empty(), egui::Button::new("Copy / Extract…")).clicked() {
                                                action = Some(MasonryAction::Copy);
                                            }
                                            let all_files = !tab.selected.is_empty() && tab.entries.iter()
                                                .filter(|entry| tab.selected.contains(&entry.get_id()))
                                                .all(|entry| matches!(entry, ImageEntry::File(_)));
                                            if actions.add_enabled(all_files, egui::Button::new("Move…")).clicked() {
                                                action = Some(MasonryAction::Move);
                                            }
                                            if actions.add_enabled(!tab.selected.is_empty(), egui::Button::new("Delete…")).clicked() {
                                                action = Some(MasonryAction::Delete);
                                            }
                                        });
                                    });
                            });
                        }
                    });
            },
        );

        // The tile-size slider updates every open Masonry tab immediately, but
        // writes settings.ini only after the user finishes the adjustment.
        if (tile_size - previous_tile_size).abs() > f32::EPSILON {
            for tab in &mut self.workspace.tabs {
                if tab.kind == WorkspaceTabKind::Masonry {
                    tab.masonry_tile_size = tile_size;
                }
            }
            if persist_tile_size {
                let _ = self.settings_manager.update(|settings| {
                    settings.masonry_thumbnail_size = tile_size;
                });
            }
        }

        if selected_sort_method != self.sort_method {
            self.sort_method = selected_sort_method;
            crate::imaging::sort::sort_images(
                &mut self.workspace.tabs[tab_index].entries,
                self.sort_method,
            );
        }

        if reload_folder {
            let (directory, recursive) = {
                let tab = &self.workspace.tabs[tab_index];
                (tab.directory.clone(), tab.recursive)
            };
            if let Some(directory) = directory.filter(|_| self.workspace.tabs[tab_index].folder_masonry) {
                let paths = if recursive {
                    let mut paths = crate::app::queues::collect_folder_images(&directory);
                    crate::imaging::sort::sort_images(&mut paths, self.sort_method);
                    paths
                } else {
                    crate::imaging::loader::load_directory_images(&directory, self.sort_method)
                };
                let refreshed = paths.into_iter().map(ImageEntry::File).collect::<Vec<_>>();
                let valid_ids = refreshed.iter().map(ImageEntry::get_id).collect::<HashSet<_>>();
                let tab = &mut self.workspace.tabs[tab_index];
                tab.entries = refreshed;
                tab.selected.retain(|id| valid_ids.contains(id));
            }
        }

        if let Some(action) = action {
            self.handle_masonry_action(tab_index, action, ctx);
        }
        self.render_delete_confirmation(ctx);
        self.render_file_conflict_dialog(ctx);
    }

    fn render_masonry_group(
        &mut self,
        ui: &mut egui::Ui,
        tab_index: usize,
        tab_id: u64,
        entries: &[ImageEntry],
        selected: &HashSet<String>,
        preferred_tile_width: f32,
        ctx: &egui::Context,
        action: &mut Option<MasonryAction>,
    ) {
        let available_width = ui.available_width().max(1.0);
        ui.set_min_width(available_width);
        ui.set_max_width(available_width);
        let gap = ui.spacing().item_spacing.x.max(0.0);
        let preferred_tile_width = preferred_tile_width.clamp(76.0, 512.0);
        let columns = (((available_width + gap) / (preferred_tile_width + gap.max(0.001))).floor() as usize)
            .max(1);
        let tile_width = ((available_width - gap * columns.saturating_sub(1) as f32) / columns as f32)
            .max(1.0);
        const TILE_MARGIN: f32 = 6.0;
        let physical_image_width = (tile_width - TILE_MARGIN * 2.0).max(1.0) * ctx.pixels_per_point();
        let target_thumbnail_size = [64_u32, 128, 256, 512]
            .into_iter()
            .find(|size| *size as f32 >= physical_image_width)
            .unwrap_or(512);

        ui.columns(columns, |column_uis| {
            let mut column_heights = vec![0.0_f32; columns];
            for (index, entry) in entries.iter().enumerate() {
                let key = entry.get_id();
                let thumbnail_source_key = crate::app::queues::source_thumbnail_key(entry);
                let mut texture: Option<egui::TextureHandle> = None;
                let mut has_sufficient_resolution = false;
                // Prefer the smallest cached size that satisfies the current view.
                // If none does, keep the best lower-resolution preview visible while
                // the requested upgrade is generated asynchronously.
                for candidate_size in [64_u32, 128, 256, 512] {
                    let candidate_key = format!("thumb:{candidate_size}:{thumbnail_source_key}");
                    if let Some(candidate) = self.sidebar_thumbnails.get(&candidate_key).cloned() {
                        if candidate_size >= target_thumbnail_size {
                            texture = Some(candidate);
                            has_sufficient_resolution = true;
                            break;
                        }
                        texture = Some(candidate);
                    }
                }
                // Pick the shortest column first, then size the tile to that
                // column's actual width. `ui.columns` divides the full Masonry
                // viewport evenly, so this fills the available width without
                // leaving a trailing blank strip.
                let column = column_heights.iter().enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(column_index, _)| column_index)
                    .unwrap_or(index % columns);
                let actual_tile_width = column_uis[column].available_width()
                    .min(tile_width.max(1.0))
                    .max(1.0);
                let image_width = (actual_tile_width - TILE_MARGIN * 2.0).max(1.0);
                let (image_height, image_size) = if let Some(texture) = &texture {
                    let size = texture.size_vec2();
                    let aspect = if size.y > 0.0 { size.x / size.y } else { 1.0 };
                    let height = image_width / aspect.max(0.001);
                    (height, egui::vec2(image_width, height))
                } else {
                    (image_width, egui::vec2(image_width, image_width))
                };
                let outer_height = image_height + TILE_MARGIN * 2.0;
                let (rect, _) = column_uis[column].allocate_exact_size(
                    egui::vec2(actual_tile_width, outer_height.max(64.0)),
                    egui::Sense::hover(),
                );
                column_heights[column] += outer_height.max(64.0) + gap;
                let image_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(TILE_MARGIN, TILE_MARGIN),
                    image_size,
                );

                if !has_sufficient_resolution
                    && image_rect.intersects(column_uis[column].clip_rect())
                {
                    // Always give the shared scheduler a chance to promote a
                    // matching pending Browse/Queue/Favorites request. The
                    // request method itself deduplicates completed/queued jobs.
                    self.request_masonry_thumbnail(
                        entry.clone(),
                        target_thumbnail_size,
                        ctx,
                    );
                }

                if let Some(texture) = texture {
                    column_uis[column].painter().image(
                        texture.id(),
                        image_rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    column_uis[column].painter().rect_filled(
                        image_rect,
                        6.0,
                        column_uis[column].visuals().faint_bg_color,
                    );
                    column_uis[column].painter().text(
                        image_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if self.shared_thumbnail_failed(&thumbnail_source_key, target_thumbnail_size) {
                            "Preview unavailable"
                        } else {
                            "Loading preview…"
                        },
                        egui::FontId::proportional(12.0),
                        column_uis[column].visuals().text_color(),
                    );
                }

                let response = column_uis[column].interact(
                    rect,
                    egui::Id::new(("masonry_entry", tab_id, key.clone())),
                    egui::Sense::click(),
                ).on_hover_text(format!(
                    "{}\n{}",
                    entry_display_name(entry),
                    entry_breadcrumb(entry, self.workspace.tabs[tab_index].directory.as_deref()),
                ));
                if selected.contains(&key) {
                    let visuals = column_uis[column].visuals().clone();
                    let selection_fill = visuals.selection.bg_fill;
                    let tint = egui::Color32::from_rgba_unmultiplied(
                        selection_fill.r(), selection_fill.g(), selection_fill.b(), 64,
                    );
                    let selection_color = visuals.selection.stroke.color;
                    column_uis[column].painter().rect_filled(rect.shrink(1.0), 6.0, tint);
                    column_uis[column].painter().rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        egui::Stroke::new(2.0_f32, selection_color),
                        egui::StrokeKind::Inside,
                    );
                    let check_center = image_rect.right_top() + egui::vec2(-16.0, 16.0);
                    column_uis[column].painter().circle_filled(check_center, 10.0, selection_fill);
                    column_uis[column].painter().text(
                        check_center,
                        egui::Align2::CENTER_CENTER,
                        "✓",
                        egui::FontId::proportional(14.0),
                        crate::ui_theme::contrast_foreground(selection_fill),
                    );
                } else if self.workspace.tabs[tab_index].context_highlight.as_deref() == Some(key.as_str()) {
                    // Context highlighting is deliberately separate from the
                    // selection set: right-clicking never changes multi-selection.
                    let visuals = column_uis[column].visuals();
                    let tint = egui::Color32::from_rgba_unmultiplied(
                        visuals.widgets.hovered.bg_fill.r(),
                        visuals.widgets.hovered.bg_fill.g(),
                        visuals.widgets.hovered.bg_fill.b(),
                        88,
                    );
                    column_uis[column].painter().rect_filled(rect.shrink(1.0), 6.0, tint);
                    column_uis[column].painter().rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        visuals.widgets.hovered.fg_stroke,
                        egui::StrokeKind::Inside,
                    );
                }

                let right_clicked = response.secondary_clicked();
                if right_clicked {
                    self.workspace.tabs[tab_index].context_highlight = Some(key.clone());
                    ctx.request_repaint();
                }

                let entry_for_menu = entry.clone();
                let selected_for_menu = selected.clone();
                let all_entries_for_menu = self.workspace.tabs[tab_index].entries.clone();
                let pending_compare_target = self.workspace.pending_compare_target_id.is_some();
                response.clone().context_menu(|menu| {
                    if menu.button("Open in Viewer").clicked() {
                        *action = Some(MasonryAction::Open(entry_for_menu.clone()));
                        menu.close();
                    }
                    if menu.button("Open in New Tab").clicked() {
                        *action = Some(MasonryAction::OpenNewTab(entry_for_menu.clone()));
                        menu.close();
                    }
                    menu.separator();
                    let is_selected = selected_for_menu.contains(&key);
                    if menu.button(if is_selected { "Deselect entry" } else { "Select entry" }).clicked() {
                        *action = Some(MasonryAction::ToggleSelection(key.clone()));
                        menu.close();
                    }
                    let mut compare_entries = all_entries_for_menu.iter()
                        .filter(|candidate| selected_for_menu.contains(&candidate.get_id()))
                        .cloned().collect::<Vec<_>>();
                    if !compare_entries.iter().any(|candidate| candidate.get_id() == key) {
                        compare_entries.push(entry_for_menu.clone());
                    }
                    let compare_minimum = if pending_compare_target { 1 } else { 2 };
                    let compare_label = if pending_compare_target { "Add to Compare" } else { "Compare with selection" };
                    if menu.add_enabled(compare_entries.len() >= compare_minimum, egui::Button::new(compare_label)).clicked() {
                        *action = Some(MasonryAction::CompareEntries(compare_entries));
                        menu.close();
                    }
                    menu.separator();
                    if menu.button("Copy / Extract…").clicked() {
                        *action = Some(MasonryAction::CopyEntries(vec![entry_for_menu.clone()]));
                        menu.close();
                    }
                    match &entry_for_menu {
                        ImageEntry::File(path) => {
                            if menu.button("Move…").clicked() {
                                *action = Some(MasonryAction::MoveEntries(vec![entry_for_menu.clone()]));
                                menu.close();
                            }
                            if menu.button("Delete…").clicked() {
                                *action = Some(MasonryAction::DeleteEntries(vec![entry_for_menu.clone()]));
                                menu.close();
                            }
                            menu.separator();
                            if menu.button("Open With…").clicked() {
                                *action = Some(MasonryAction::OpenWith(path.clone()));
                                menu.close();
                            }
                            if menu.button("Locate in File Manager").clicked() {
                                *action = Some(MasonryAction::Reveal(path.clone()));
                                menu.close();
                            }
                            if menu.button("Rename… (F2)").clicked() {
                                *action = Some(MasonryAction::Rename(path.clone()));
                                menu.close();
                            }
                            if menu.button("Add to Queue").clicked() {
                                *action = Some(MasonryAction::AddToQueue(path.clone()));
                                menu.close();
                            }
                            if menu.button("Add to Favorites").clicked() {
                                *action = Some(MasonryAction::AddToFavorites(path.clone()));
                                menu.close();
                            }
                            if menu.button("Copy File Path").clicked() {
                                *action = Some(MasonryAction::CopyText(path.display().to_string()));
                                menu.close();
                            }
                        }
                        ImageEntry::Zip(item) => {
                            if menu.button("Open Containing Archive").clicked() {
                                *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                                menu.close();
                            }
                            if menu.button("Copy Entry Name").clicked() {
                                *action = Some(MasonryAction::CopyText(item.name.clone()));
                                menu.close();
                            }
                        }
                        ImageEntry::S7z(item) => {
                            if menu.button("Open Containing Archive").clicked() {
                                *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                                menu.close();
                            }
                            if menu.button("Copy Entry Name").clicked() {
                                *action = Some(MasonryAction::CopyText(item.name.clone()));
                                menu.close();
                            }
                        }
                        ImageEntry::Rar(item) => {
                            if menu.button("Open Containing Archive").clicked() {
                                *action = Some(MasonryAction::OpenArchive(item.archive_path.clone()));
                                menu.close();
                            }
                            if menu.button("Copy Entry Name").clicked() {
                                *action = Some(MasonryAction::CopyText(item.name.clone()));
                                menu.close();
                            }
                        }
                    }
                });

                if response.clicked() {
                    self.workspace.tabs[tab_index].context_highlight = None;
                    if self.workspace.tabs[tab_index].selection_mode {
                        let tab = &mut self.workspace.tabs[tab_index];
                        if !tab.selected.insert(key.clone()) { tab.selected.remove(&key); }
                    } else {
                        *action = Some(MasonryAction::Open(entry.clone()));
                    }
                }
            }
        });
    }

    fn open_masonry_entry_in_viewer(
        &mut self,
        masonry_tab_index: usize,
        entry: &ImageEntry,
        ctx: &egui::Context,
    ) {
        let Some(masonry_tab) = self.workspace.tabs.get(masonry_tab_index) else { return; };
        let entries = masonry_tab.entries.clone();
        let Some(index) = entries.iter().position(|candidate| candidate.get_id() == entry.get_id()) else { return; };
        let directory = masonry_tab.directory.clone().or_else(|| entry_parent(entry));
        if !self.prepare_for_image_change(
            "Save or discard unsaved edits before opening this Masonry collection.",
        ) {
            return;
        }
        self.snapshot_active_viewer_tab();

        let viewer_index = self.workspace.tabs.iter().position(|tab| tab.kind == WorkspaceTabKind::Viewer && tab.id == 1)
            .or_else(|| self.workspace.tabs.iter().position(|tab| tab.kind == WorkspaceTabKind::Viewer));
        let viewer_index = if let Some(index) = viewer_index {
            index
        } else {
            self.workspace.tabs.insert(0, WorkspaceTab::viewer());
            0
        };
        if viewer_index >= self.workspace.tabs.len() { return; }
        {
            let viewer = &mut self.workspace.tabs[viewer_index];
            viewer.title = if viewer.id == 1 { "Viewer".to_owned() } else { viewer.title.clone() };
            viewer.entries = entries.clone();
            viewer.current_index = index;
            viewer.directory = directory.clone();
            viewer.zoom = 1.0;
            viewer.pan = egui::Vec2::ZERO;
            viewer.fit_to_window = true;
            viewer.zoom_used = false;
        }
        self.workspace.active_index = viewer_index;
        self.current_directory = directory;
        self.queue_mode = false;
        self.sidebar_queue_tab = false;
        self.sidebar_favorites_tab = false;
        self.sidebar_info_tab = false;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
        self.b_fit_to_window = true;
        self.b_zoom_used = false;
        self.set_image_entries(entries, index);
        self.sync_current_entry_after_collection_change(ctx);
    }

    fn handle_masonry_action(&mut self, tab_index: usize, action: MasonryAction, ctx: &egui::Context) {
        let selected_entries = {
            let tab = &self.workspace.tabs[tab_index];
            tab.entries.iter().filter(|entry| tab.selected.contains(&entry.get_id())).cloned().collect::<Vec<_>>()
        };
        match action {
            MasonryAction::Open(entry) => self.open_masonry_entry_in_viewer(tab_index, &entry, ctx),
            MasonryAction::OpenNewTab(entry) => self.open_entry_in_image_tab(entry, false),
            MasonryAction::ToggleSelection(entry_id) => {
                let tab = &mut self.workspace.tabs[tab_index];
                tab.selection_mode = true;
                if !tab.selected.insert(entry_id.clone()) {
                    tab.selected.remove(&entry_id);
                }
            }
            MasonryAction::CompareEntries(entries) => self.open_compare_workspace(entries),
            MasonryAction::CopyEntries(entries) => self.copy_or_move_selected(entries, false),
            MasonryAction::MoveEntries(entries) => self.copy_or_move_selected(entries, true),
            MasonryAction::DeleteEntries(entries) => {
                let selected_count = entries.len();
                let files = entries.into_iter().filter(|entry| matches!(entry, ImageEntry::File(_))).collect::<Vec<_>>();
                if files.is_empty() {
                    self.queue_status = Some("Archive entries cannot be deleted from inside an archive.".into());
                } else {
                    let archive_count = selected_count.saturating_sub(files.len());
                    let tab_id = self.workspace.tabs[tab_index].id;
                    self.workspace.delete_confirmation = Some((tab_id, files, archive_count));
                }
            }
            MasonryAction::AddToQueue(path) => {
                let added = self.add_queue_path(path);
                self.queue_status = Some(if added { "Added image to Queue.".into() } else { "Image is already in Queue.".into() });
            }
            MasonryAction::AddToFavorites(path) => self.add_favorite_path(path),
            MasonryAction::Rename(path) => self.begin_rename(&path),
            MasonryAction::Reveal(path) => {
                if let Err(error) = crate::app::external_open::reveal_in_file_manager(&path) {
                    self.queue_status = Some(format!("Could not locate file: {error}"));
                }
            }
            MasonryAction::OpenWith(path) => {
                if let Err(error) = crate::app::external_open::open_with(&path) {
                    self.queue_status = Some(format!("Could not open external application: {error}"));
                }
            }
            MasonryAction::OpenArchive(path) => {
                if let Err(error) = open::that(&path) {
                    self.queue_status = Some(format!("Could not open archive: {error}"));
                }
            }
            MasonryAction::CopyText(text) => {
                self.queue_status = Some(match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
                    Ok(()) => "Copied entry information to clipboard.".into(),
                    Err(error) => format!("Could not copy entry information: {error}"),
                });
            }
            MasonryAction::OpenSelected => {
                for entry in selected_entries { self.open_entry_in_image_tab(entry, false); }
                self.queue_status = Some("Selected images opened in background tabs.".into());
            }
            MasonryAction::ViewSelected => {
                if selected_entries.is_empty() || !self.prepare_for_image_change(
                    "Save or discard unsaved edits before opening the selected collection.",
                ) {
                    return;
                }
                self.snapshot_active_viewer_tab();
                self.sidebar_queue_tab = false;
                self.sidebar_favorites_tab = false;
                self.sidebar_info_tab = false;
                self.queue_mode = false;
                let id = self.workspace.allocate_id();
                let directory = self.workspace.tabs[tab_index].directory.clone();
                let mut tab = WorkspaceTab::new(
                    id,
                    format!("Selected ({})", selected_entries.len()),
                    WorkspaceTabKind::Viewer,
                );
                tab.entries = selected_entries;
                tab.directory = directory;
                tab.current_index = 0;
                self.workspace.tabs.push(tab);
                self.activate_workspace_tab(id);
                self.sync_current_entry_after_collection_change(ctx);
            }
            MasonryAction::Compare => self.open_compare_workspace(selected_entries),
            MasonryAction::Copy => self.copy_or_move_selected(selected_entries, false),
            MasonryAction::Move => self.copy_or_move_selected(selected_entries, true),
            MasonryAction::Delete => {
                let selected_count = selected_entries.len();
                let files = selected_entries.into_iter().filter(|entry| matches!(entry, ImageEntry::File(_))).collect::<Vec<_>>();
                if files.is_empty() {
                    self.queue_status = Some("Archive entries cannot be deleted from inside an archive.".into());
                } else {
                    let archive_count = selected_count.saturating_sub(files.len());
                    let tab_id = self.workspace.tabs[tab_index].id;
                    self.workspace.delete_confirmation = Some((tab_id, files, archive_count));
                }
            }
        }
    }

    fn copy_or_move_selected(&mut self, entries: Vec<ImageEntry>, move_files: bool) {
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

    fn render_file_conflict_dialog(&mut self, ctx: &egui::Context) {
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

    fn execute_file_operation(&mut self, pending: PendingFileOperation, resolution: ConflictResolution) {
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

    fn render_delete_confirmation(&mut self, ctx: &egui::Context) {
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

    /// Load the last workspace snapshot once per application launch.
    pub(crate) fn check_workspace_session_restore(&mut self) {
        if self.workspace.session_checked {
            return;
        }
        self.workspace.session_checked = true;
        // A command-line launch with an explicit file or folder is an intentional
        // request and takes precedence over the previous session. This also covers
        // the eager first image while Explorer/IPC multi-open grouping is pending.
        if !self.image_entries.is_empty() || self.indexing_receiver.is_some() {
            return;
        }
        let preference = self.settings_manager.get().session_restore_preference.as_str();
        if preference == "never" {
            return;
        }
        let path = self.settings_manager.path().with_file_name("workspace_session.txt");
        let Some(session) = load_workspace_session(&path) else {
            return;
        };
        if preference == "ask" {
            self.workspace.pending_session = Some(session);
            self.workspace.session_prompt = true;
        } else {
            self.apply_workspace_session(session);
        }
    }

    pub(crate) fn render_workspace_session_prompt(&mut self, ctx: &egui::Context) {
        if !self.workspace.session_prompt {
            return;
        }
        let mut restore = false;
        let mut discard = false;
        egui::Window::new("Restore previous session?")
            .id(egui::Id::new("workspace_session_restore_prompt"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("A previous JPEG Viewer workspace session is available.");
                ui.label("Restore its tabs, Masonry collections, comparison workspaces, and viewing positions?");
                ui.horizontal(|ui| {
                    if ui.button("Don't restore").clicked() { discard = true; }
                    if ui.button("Restore session").clicked() { restore = true; }
                });
            });
        if restore {
            if let Some(session) = self.workspace.pending_session.take() {
                self.apply_workspace_session(session);
            }
            self.workspace.session_prompt = false;
        } else if discard {
            self.workspace.pending_session = None;
            self.workspace.session_prompt = false;
        }
    }

    fn apply_workspace_session(&mut self, session: RestorableSession) {
        let (mut tabs, active_id, next_id, last_image_tab_id) = session;
        // Welcome pages are transient launch surfaces, not part of a restored
        // work session. Restoring must go directly to the saved workspaces.
        tabs.retain(|tab| tab.kind != WorkspaceTabKind::Welcome);
        if tabs.iter().all(|tab| tab.kind != WorkspaceTabKind::Viewer) {
            tabs.insert(0, WorkspaceTab::viewer());
        }
        // Folder workspaces are refreshed from disk. Local workspaces retain
        // their explicit entry list, as selected files must not expand to a folder.
        for tab in &mut tabs {
            if tab.kind == WorkspaceTabKind::Masonry {
                tab.masonry_tile_size = self.settings_manager.get().masonry_thumbnail_size.clamp(76.0, 512.0);
            }
            if tab.kind == WorkspaceTabKind::Masonry && tab.folder_masonry {
                if let Some(directory) = tab.directory.as_ref() {
                    let paths = if tab.recursive {
                        let mut paths = crate::app::queues::collect_folder_images(directory);
                        crate::imaging::sort::sort_images(&mut paths, self.sort_method);
                        paths
                    } else {
                        crate::imaging::loader::load_directory_images(directory, self.sort_method)
                    };
                    tab.entries = paths.into_iter().map(ImageEntry::File).collect();
                }
            }
            if tab.kind == WorkspaceTabKind::Compare {
                let old_entries = std::mem::take(&mut tab.entries);
                let old_anchors = std::mem::take(&mut tab.anchors);
                let old_anchors_second = std::mem::take(&mut tab.anchors_second);
                let old_offsets = std::mem::take(&mut tab.offsets);
                let old_zoom = std::mem::take(&mut tab.pane_zoom);
                let old_sync = std::mem::take(&mut tab.pane_sync);
                let old_size = std::mem::take(&mut tab.pane_size);
                for (index, entry) in old_entries.into_iter().enumerate() {
                    if !entry_source_available(&entry) { continue; }
                    tab.entries.push(entry);
                    tab.anchors.push(old_anchors.get(index).copied().flatten());
                    tab.anchors_second.push(old_anchors_second.get(index).copied().flatten());
                    tab.offsets.push(old_offsets.get(index).copied().unwrap_or([0.0, 0.0]));
                    tab.pane_zoom.push(old_zoom.get(index).copied().unwrap_or(1.0));
                    tab.pane_sync.push(old_sync.get(index).copied().unwrap_or(true));
                    tab.pane_size.push(old_size.get(index).copied().unwrap_or([1.0, 1.0]));
                }
            } else {
                tab.entries.retain(entry_source_available);
                tab.current_index = tab.current_index.min(tab.entries.len().saturating_sub(1));
            }
            let valid_ids = tab.entries.iter().map(ImageEntry::get_id).collect::<HashSet<_>>();
            tab.selected.retain(|id| valid_ids.contains(id));
        }
        // Drop restored one-image tabs whose source no longer exists; Compare
        // panes have already been pruned in lockstep with their anchor settings.
        tabs.retain(|tab| tab.kind != WorkspaceTabKind::Image || !tab.entries.is_empty());
        let last_image_tab_id = last_image_tab_id.filter(|id| tabs.iter().any(|tab| tab.id == *id && tab.kind == WorkspaceTabKind::Image));
        self.workspace.tabs = tabs;
        self.workspace.active_index = usize::MAX;
        self.workspace.next_id = next_id.max(self.workspace.tabs.iter().map(|tab| tab.id).max().unwrap_or(1) + 1);
        self.workspace.last_image_tab_id = last_image_tab_id;
        let id = if self.workspace.tabs.iter().any(|tab| tab.id == active_id) {
            active_id
        } else {
            self.workspace.tabs[0].id
        };
        self.activate_workspace_tab(id);
    }

    /// Persist tab/workspace metadata next to settings.ini. Texture handles and
    /// worker state are deliberately reconstructed on the next launch.
    pub(crate) fn save_workspace_session(&mut self) {
        self.snapshot_active_viewer_tab();
        let path = self.settings_manager.path().with_file_name("workspace_session.txt");
        if self.settings_manager.get().session_restore_preference == "never" {
            let _ = fs::remove_file(&path);
            return;
        }
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("Could not create workspace session directory: {error}");
                return;
            }
        }
        let mut output = String::new();
        let active_tab = self.workspace.tabs.get(self.workspace.active_index);
        let active_id = active_tab
            .filter(|tab| tab.kind != WorkspaceTabKind::Welcome)
            .map(|tab| tab.id)
            .or_else(|| self.workspace.tabs.iter().find(|tab| tab.kind == WorkspaceTabKind::Viewer).map(|tab| tab.id))
            .unwrap_or(1);
        output.push_str(&format!("ACTIVE|{active_id}\nNEXT|{}\nLASTIMAGE|{}\n", self.workspace.next_id, self.workspace.last_image_tab_id.map(|id| id.to_string()).unwrap_or_default()));
        for tab in &self.workspace.tabs {
            if tab.kind == WorkspaceTabKind::Welcome {
                continue;
            }
            output.push_str(&format!(
                "T|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                tab.id,
                tab_kind_code(tab.kind),
                encode_field(&tab.title),
                tab.current_index,
                tab.directory.as_ref().map(|path| encode_field(&path.to_string_lossy())).unwrap_or_default(),
                tab.zoom,
                tab.pan.x,
                tab.pan.y,
                tab.fit_to_window,
                tab.zoom_used,
                tab.recursive,
                tab.folder_masonry,
                tab.masonry_tile_size,
                tab.selection_mode,
                encode_field(&tab.search),
                encode_field(&tab.extension_filter),
                tab.show_anchors,
                tab.sync_pan,
                tab.sync_zoom,
                tab.sync_navigation,
                tab.compare_columns,
                tab.compare_rows,
            ));
            for entry in &tab.entries {
                match entry {
                    ImageEntry::File(path) => output.push_str(&format!("E|{}|F|{}\n", tab.id, encode_field(&path.to_string_lossy()))),
                    ImageEntry::Zip(item) => output.push_str(&format!("E|{}|Z|{}|{}|{}\n", tab.id, encode_field(&item.archive_path.to_string_lossy()), item.entry_index, encode_field(&item.name))),
                    ImageEntry::S7z(item) => output.push_str(&format!("E|{}|7|{}|{}\n", tab.id, encode_field(&item.archive_path.to_string_lossy()), encode_field(&item.name))),
                    ImageEntry::Rar(item) => output.push_str(&format!("E|{}|R|{}|{}\n", tab.id, encode_field(&item.archive_path.to_string_lossy()), encode_field(&item.name))),
                }
            }
            for selected in &tab.selected {
                output.push_str(&format!("S|{}|{}\n", tab.id, encode_field(selected)));
            }
            for (index, zoom) in tab.pane_zoom.iter().enumerate() {
                output.push_str(&format!("Z|{}|{}|{}\n", tab.id, index, zoom));
            }
            for (index, participates) in tab.pane_sync.iter().enumerate() {
                output.push_str(&format!("P|{}|{}|{}\n", tab.id, index, participates));
            }
            for (index, size) in tab.pane_size.iter().enumerate() {
                output.push_str(&format!("Q|{}|{}|{}|{}\n", tab.id, index, size[0], size[1]));
            }
            for index in 0..tab.anchors.len().max(tab.offsets.len()) {
                let anchor = tab.anchors.get(index).copied().flatten();
                let offset = tab.offsets.get(index).copied().unwrap_or([0.0, 0.0]);
                let (ax, ay) = anchor.map(|point| (point[0].to_string(), point[1].to_string())).unwrap_or_else(|| ("-".into(), "-".into()));
                output.push_str(&format!("A|{}|{}|{}|{}|{}|{}\n", tab.id, index, ax, ay, offset[0], offset[1]));
            }
        }
        let temporary = path.with_extension("txt.tmp");
        if let Err(error) = fs::write(&temporary, output) {
            eprintln!("Could not write workspace session: {error}");
            return;
        }
        if let Err(error) = fs::rename(&temporary, &path) {
            // Windows cannot rename over an existing file. Keep a backup so a
            // failed replacement never destroys the last known-good session.
            let backup = path.with_extension("txt.bak");
            let had_previous = path.exists();
            if had_previous {
                if let Err(backup_error) = fs::rename(&path, &backup) {
                    eprintln!("Could not back up previous workspace session ({error}; {backup_error})");
                    let _ = fs::remove_file(&temporary);
                    return;
                }
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => { if had_previous { let _ = fs::remove_file(backup); } }
                Err(retry_error) => {
                    if had_previous { let _ = fs::rename(&backup, &path); }
                    eprintln!("Could not replace workspace session ({error}; {retry_error})");
                }
            }
        }
    }

    fn collect_compare_images(&mut self) {
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
                    if self.workspace.compare_images.len() >= 12 {
                        if let Some(evict) = self.workspace.compare_images.keys().next().cloned() {
                            self.workspace.compare_images.remove(&evict);
                        }
                    }
                    self.workspace.compare_images.insert(key, image);
                }
                Err(error) => {
                    self.workspace.compare_failed.insert(key);
                    self.queue_status = Some(format!("Could not prepare comparison image: {error}"));
                }
            }
        }
    }

    fn request_compare_image(&mut self, entry: ImageEntry, key: String, ctx: &egui::Context) {
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
                        let frame = animation.get_current_frame_ref()
                            .ok_or_else(|| "Animated image contains no displayable frame.".to_owned())?;
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
                    .ok_or_else(|| "Comparison image decoded size overflows address space.".to_owned())?;
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

    fn return_to_masonry_from_compare(&mut self, compare_tab_id: u64) {
        self.workspace.pending_compare_target_id = Some(compare_tab_id);
        if let Some(id) = self.workspace.tabs.iter().rev()
            .find(|tab| tab.kind == WorkspaceTabKind::Masonry)
            .map(|tab| tab.id)
        {
            self.activate_workspace_tab(id);
        } else {
            self.open_local_masonry();
        }
    }

    fn render_compare_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.collect_compare_images();
        let Some(tab_index) = (self.workspace.active_index < self.workspace.tabs.len())
            .then_some(self.workspace.active_index)
        else {
            return;
        };
        let tab_id = self.workspace.tabs[tab_index].id;
        let compare_central_rect = ui.available_rect_before_wrap()
            .intersect(ui.clip_rect())
            .intersect(self.main_view_rect(ctx));
        let mut close = false;
        let mut remove_pane: Option<usize> = None;
        let mut align_anchors_requested = false;
        let mut open_masonry_from_compare = false;
        {
            let tab = &mut self.workspace.tabs[tab_index];
            ui.horizontal_wrapped(|ui| {
                ui.heading("Compare");
                ui.separator();
                let mut sync_all = tab.sync_pan && tab.sync_zoom;
                if ui.checkbox(&mut sync_all, "Sync all").changed() {
                    tab.sync_pan = sync_all;
                    tab.sync_zoom = sync_all;
                }
                ui.checkbox(&mut tab.sync_pan, "Sync pan");
                ui.checkbox(&mut tab.sync_zoom, "Sync zoom");
                ui.checkbox(&mut tab.show_anchors, "Show anchors");
                ui.separator();
                ui.selectable_value(&mut tab.compare_rows, false, "Columns");
                ui.selectable_value(&mut tab.compare_rows, true, "Rows");
                if ui.button("Align anchors").on_hover_text(
                    "Use the two corresponding points in each pane to calculate matching zoom and pan. Images should have the same orientation.",
                ).clicked() {
                    align_anchors_requested = true;
                }
                if ui.button("Clear all anchors").clicked() {
                    tab.anchors.fill(None);
                    tab.anchors_second.fill(None);
                    tab.offsets.fill([0.0, 0.0]);
                    tab.anchor_editing = None;
                    tab.anchor_editing_point = 0;
                }
                if ui.button("Reset view").clicked() {
                    tab.offsets.fill([0.0, 0.0]);
                    tab.pane_zoom.fill(1.0);
                    tab.anchors.fill(None);
                    tab.anchors_second.fill(None);
                    tab.anchor_editing = None;
                    tab.anchor_editing_point = 0;
                }
                if ui.button("Close comparison").clicked() {
                    close = true;
                }
            });
        }

        let entries = self.workspace.tabs[tab_index].entries.clone();
        if entries.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No comparison images remain. Close this workspace or add images from Masonry.");
            });
        } else {
            let actual_entry_count = entries.len();
            // A one-image comparison always keeps a second, intentionally empty
            // slot so it is obvious where another image can be added.
            let pane_count = actual_entry_count.max(2);
            let compare_rows = self.workspace.tabs[tab_index].compare_rows;
            if pane_count > 4 {
                ui.label(egui::RichText::new(format!(
                    "{} panes are open. Four is recommended for smoother comparison.",
                    pane_count
                )).weak());
            }
            if self.workspace.tabs[tab_index].pane_zoom.len() != pane_count {
                self.workspace.tabs[tab_index].pane_zoom.resize(pane_count, 1.0);
            }
            if self.workspace.tabs[tab_index].pane_sync.len() != pane_count {
                self.workspace.tabs[tab_index].pane_sync.resize(pane_count, true);
            }
            if self.workspace.tabs[tab_index].pane_size.len() != pane_count {
                self.workspace.tabs[tab_index].pane_size.resize(pane_count, [1.0, 1.0]);
            }
            if self.workspace.tabs[tab_index].offsets.len() != pane_count {
                self.workspace.tabs[tab_index].offsets.resize(pane_count, [0.0, 0.0]);
            }
            if self.workspace.tabs[tab_index].anchors.len() != pane_count {
                self.workspace.tabs[tab_index].anchors.resize(pane_count, None);
            }
            if self.workspace.tabs[tab_index].anchors_second.len() != pane_count {
                self.workspace.tabs[tab_index].anchors_second.resize(pane_count, None);
            }
            let mut placed_anchor: Option<(usize, usize, [f32; 2])> = None;
            let mut pan_delta: Option<(usize, [f32; 2])> = None;
            let mut resize_delta: Option<(usize, [f32; 2])> = None;
            // Per-pane image-to-viewport scale factors are used to calculate
            // normalized alignment offsets when anchor points are placed.
            let mut anchor_metrics: Vec<Option<[f32; 2]>> = vec![None; pane_count];
            let mut anchor_image_sizes: Vec<Option<[f32; 2]>> = vec![None; pane_count];
            let mut pan_metrics: Vec<Option<[f32; 2]>> = vec![None; pane_count];

            // Measure the central view's clipped width. `available_width()` can
            // shrink after the toolbar row and produce an undersized comparison grid.
            let compare_viewport_rect = ui.available_rect_before_wrap()
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
                                        let mut participates = self.workspace.tabs[tab_index].pane_sync[index];
                                        if ui.checkbox(&mut participates, "Sync").changed() {
                                            self.workspace.tabs[tab_index].pane_sync[index] = participates;
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
                                        let fit_scale = (pane_width / size.x.max(1.0))
                                            .min(pane_height / size.y.max(1.0));
                                        let zoom = self.workspace.tabs[tab_index].pane_zoom[index].clamp(0.01, 100.0);
                                        let image_zoom = fit_scale * zoom;
                                        let image_size = size * image_zoom;
                                        let offset = self.workspace.tabs[tab_index].offsets
                                            .get(index).copied().unwrap_or([0.0, 0.0]);
                                        // Offsets are normalized image coordinates. Convert them
                                        // back to source-image pixels before passing them to the
                                        // same VirtualTexture renderer used by the main viewer.
                                        let image_pan = egui::vec2(
                                            offset[0] * size.x,
                                            offset[1] * size.y,
                                        );
                                        let image_rect = egui::Rect::from_center_size(
                                            rect.center() + image_pan * image_zoom,
                                            image_size,
                                        );
                                        let scale_x = image_rect.width() / rect.width().max(1.0);
                                        let scale_y = image_rect.height() / rect.height().max(1.0);
                                        anchor_metrics[index] = Some([scale_x, scale_y]);
                                        anchor_image_sizes[index] = Some([image_rect.width().max(1.0), image_rect.height().max(1.0)]);
                                        pan_metrics[index] = anchor_image_sizes[index];
                                        let painter = pane_painter.clone();
                                        let texture_options = self.get_texture_options();
                                        if let Some(image) = self.workspace.compare_images.get_mut(&key) {
                                            image.render(
                                                ctx,
                                                &painter,
                                                image_zoom,
                                                image_pan,
                                                rect,
                                                texture_options,
                                            );
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
                                                    let point = egui::pos2(
                                                        image_rect.left() + image_rect.width() * anchor[0],
                                                        image_rect.top() + image_rect.height() * anchor[1],
                                                    );
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

                                        if editing && response.clicked() && !resize_response.hovered() {
                                            let point = response.interact_pointer_pos().unwrap_or(rect.center());
                                            if image_rect.contains(point) {
                                                placed_anchor = Some((index, editing_point, [
                                                    ((point.x - image_rect.left()) / image_rect.width()).clamp(0.0, 1.0),
                                                    ((point.y - image_rect.top()) / image_rect.height()).clamp(0.0, 1.0),
                                                ]));
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
                            if actual_entry_count == 1 {
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
                                        let (rect, _) = pane.allocate_exact_size(
                                            egui::vec2(base_width, placeholder_height),
                                            egui::Sense::hover(),
                                        );
                                        let painter = pane.painter().with_clip_rect(rect.intersect(pane.clip_rect()));
                                        painter.rect_filled(rect, 0.0, pane.visuals().faint_bg_color);
                                        let message_rect = egui::Rect::from_min_size(
                                            rect.min + egui::vec2(16.0, (rect.height() * 0.5 - 32.0).max(8.0)),
                                            egui::vec2((rect.width() - 32.0).max(1.0), 42.0),
                                        );
                                        pane.put(
                                            message_rect,
                                            egui::Label::new(
                                                egui::RichText::new("No comparison image. add more images from Masonry.")
                                                    .color(pane.visuals().weak_text_color()),
                                            ).wrap(),
                                        );
                                        let button_rect = egui::Rect::from_center_size(
                                            egui::pos2(rect.center().x, rect.center().y + 28.0),
                                            egui::vec2(126.0, 28.0),
                                        );
                                        if pane.put(button_rect, egui::Button::new("Open Masonry")).clicked() {
                                            open_masonry_from_compare = true;
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                            });
                        });
                },
            );

            if let Some(index) = remove_pane {
                let removed = {
                    let tab = &mut self.workspace.tabs[tab_index];
                    if index < tab.entries.len() {
                        let removed = tab.entries.remove(index).get_id();
                        if index < tab.anchors.len() { tab.anchors.remove(index); }
                        if index < tab.anchors_second.len() { tab.anchors_second.remove(index); }
                        if index < tab.offsets.len() { tab.offsets.remove(index); }
                        if index < tab.pane_zoom.len() { tab.pane_zoom.remove(index); }
                        if index < tab.pane_sync.len() { tab.pane_sync.remove(index); }
                        if index < tab.pane_size.len() { tab.pane_size.remove(index); }
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
                            && candidate.entries.iter().any(|entry| entry.get_id() == removed)
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

            if let Some((index, screen_delta)) = pan_delta {
                let tab = &mut self.workspace.tabs[tab_index];
                let sync_source = tab.sync_pan && tab.pane_sync.get(index).copied().unwrap_or(true);
                let sync_targets = tab.pane_sync.clone();
                for (pane_index, offset) in tab.offsets.iter_mut().enumerate() {
                    if pane_index != index && (!sync_source || !sync_targets.get(pane_index).copied().unwrap_or(true)) {
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
            if let Some((index, delta)) = resize_delta {
                if let Some(size) = self.workspace.tabs[tab_index].pane_size.get_mut(index) {
                    size[0] = (size[0] + delta[0]).clamp(0.5, 1.0);
                    size[1] = (size[1] + delta[1]).clamp(0.5, 2.0);
                }
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
                    align_compare_anchor_pairs(tab, &anchor_metrics, &anchor_image_sizes);
                }
            } else if align_anchors_requested {
                let tab = &mut self.workspace.tabs[tab_index];
                align_compare_anchor_pairs(tab, &anchor_metrics, &anchor_image_sizes);
            }
        }
        if close {
            self.close_workspace_tab(tab_id);
        } else if open_masonry_from_compare {
            self.return_to_masonry_from_compare(tab_id);
        }
    }

}

/// Align panes from two corresponding image points. The first point establishes
/// translation; the displayed distance between both points establishes relative zoom.
/// `image_sizes` contains the rendered image rectangle dimensions, not source pixels,
/// so the distance includes each pane's current fit scale and zoom. Images are assumed
/// to have the same orientation (rotation is not changed here).
fn align_compare_anchor_pairs(
    tab: &mut WorkspaceTab,
    metrics: &[Option<[f32; 2]>],
    image_sizes: &[Option<[f32; 2]>],
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
        .find(|index| reference_eligible(*index)
            && tab.anchors_second.get(*index).copied().flatten().is_some())
        .or_else(|| (0..tab.entries.len()).find(|index| reference_eligible(*index)));
    let Some(reference_index) = reference_index else { return; };
    let Some(reference_first) = tab.anchors[reference_index] else { return; };
    let Some(reference_metrics) = metrics.get(reference_index).copied().flatten() else { return; };
    let reference_offset = tab.offsets.get(reference_index).copied().unwrap_or([0.0, 0.0]);
    let target_first = [
        0.5 + (reference_first[0] - 0.5 + reference_offset[0]) * reference_metrics[0],
        0.5 + (reference_first[1] - 0.5 + reference_offset[1]) * reference_metrics[1],
    ];

    let reference_distance = tab.anchors_second.get(reference_index).copied().flatten()
        .zip(image_sizes.get(reference_index).copied().flatten())
        .map(|(second, size)| {
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
        let Some(first) = tab.anchors.get(pane_index).copied().flatten() else { continue; };
        let Some(mut pane_metrics) = metrics.get(pane_index).copied().flatten() else { continue; };

        // If both anchors are present, adjust this pane's zoom so the distance
        // between corresponding points matches the reference pane. For panes with
        // only one anchor, align position while retaining their current zoom.
        if let (Some(reference_distance), Some(second), Some(size)) = (
            reference_distance,
            tab.anchors_second.get(pane_index).copied().flatten(),
            image_sizes.get(pane_index).copied().flatten(),
        ) {
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

pub(crate) type RestorableSession = (Vec<WorkspaceTab>, u64, u64, Option<u64>);

fn tab_kind_code(kind: WorkspaceTabKind) -> &'static str {
    match kind {
        WorkspaceTabKind::Viewer => "V",
        WorkspaceTabKind::Image => "I",
        WorkspaceTabKind::Masonry => "M",
        WorkspaceTabKind::Compare => "C",
        WorkspaceTabKind::Welcome => "W",
    }
}

fn parse_tab_kind(code: &str) -> Option<WorkspaceTabKind> {
    match code {
        "V" => Some(WorkspaceTabKind::Viewer),
        "I" => Some(WorkspaceTabKind::Image),
        "M" => Some(WorkspaceTabKind::Masonry),
        "C" => Some(WorkspaceTabKind::Compare),
        "W" => Some(WorkspaceTabKind::Welcome),
        _ => None,
    }
}

fn encode_field(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn decode_field(value: &str) -> String {
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let Ok(hex) = std::str::from_utf8(pair) else { return String::new(); };
        let Ok(byte) = u8::from_str_radix(hex, 16) else { return String::new(); };
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn load_workspace_session(path: &Path) -> Option<RestorableSession> {
    let contents = fs::read_to_string(path).ok()?;
    let mut tabs: Vec<WorkspaceTab> = Vec::new();
    let mut active_id = 1_u64;
    let mut next_id = 2_u64;
    let mut last_image_tab_id = None;
    for line in contents.lines() {
        let fields = line.split('|').collect::<Vec<_>>();
        match fields.first().copied().unwrap_or_default() {
            "ACTIVE" => active_id = fields.get(1)?.parse().ok()?,
            "NEXT" => next_id = fields.get(1).and_then(|value| value.parse().ok()).unwrap_or(2),
            "LASTIMAGE" => last_image_tab_id = fields.get(1).and_then(|value| value.parse().ok()),
            "T" if fields.len() >= 21 => {
                let id = fields[1].parse::<u64>().ok()?;
                let kind = parse_tab_kind(fields[2])?;
                if kind == WorkspaceTabKind::Welcome { continue; }
                let title = decode_field(fields[3]);
                let mut tab = WorkspaceTab::new(id, title, kind);
                tab.current_index = fields[4].parse().unwrap_or(0);
                tab.directory = if fields[5].is_empty() { None } else { Some(PathBuf::from(decode_field(fields[5]))) };
                tab.zoom = fields[6].parse::<f32>().unwrap_or(1.0).clamp(0.01, 1000.0);
                tab.pan = egui::vec2(fields[7].parse().unwrap_or(0.0), fields[8].parse().unwrap_or(0.0));
                tab.fit_to_window = fields[9] == "true";
                tab.zoom_used = fields[10] == "true";
                tab.recursive = fields[11] == "true";
                tab.folder_masonry = fields[12] == "true";
                tab.masonry_tile_size = fields[13].parse::<f32>()
                    .ok()
                    .filter(|size| (76.0..=512.0).contains(size))
                    .unwrap_or(220.0);
                tab.selection_mode = fields[14] == "true";
                tab.search = decode_field(fields[15]);
                tab.extension_filter = decode_field(fields[16]);
                tab.show_anchors = fields[17] != "false";
                tab.sync_pan = fields[18] != "false";
                tab.sync_zoom = fields[19] != "false";
                tab.sync_navigation = fields[20] == "true";
                tab.compare_columns = fields.get(21).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0).min(4);
                tab.compare_rows = fields.get(22).is_some_and(|value| *value == "true");
                tabs.push(tab);
            }
            "E" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let entry = match fields[2] {
                    "F" => ImageEntry::File(PathBuf::from(decode_field(fields[3]))),
                    "Z" if fields.len() >= 6 => ImageEntry::Zip(crate::imaging::entry::ArchiveImage {
                        archive_path: PathBuf::from(decode_field(fields[3])),
                        entry_index: fields[4].parse().unwrap_or(0),
                        name: decode_field(fields[5]),
                    }),
                    "7" if fields.len() >= 5 => ImageEntry::S7z(crate::imaging::entry::S7ArchiveImage {
                        archive_path: PathBuf::from(decode_field(fields[3])),
                        name: decode_field(fields[4]),
                    }),
                    "R" if fields.len() >= 5 => ImageEntry::Rar(crate::imaging::entry::RarArchiveImage {
                        archive_path: PathBuf::from(decode_field(fields[3])),
                        name: decode_field(fields[4]),
                    }),
                    _ => continue,
                };
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) { tab.entries.push(entry); }
            }
            "S" if fields.len() >= 3 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) { tab.selected.insert(decode_field(fields[2])); }
            }
            "Z" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_zoom.len() <= index { tab.pane_zoom.resize(index + 1, 1.0); }
                    tab.pane_zoom[index] = fields[3].parse::<f32>().unwrap_or(1.0).clamp(0.01, 100.0);
                }
            }
            "P" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_sync.len() <= index { tab.pane_sync.resize(index + 1, true); }
                    tab.pane_sync[index] = fields[3] != "false";
                }
            }
            "Q" if fields.len() >= 5 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_size.len() <= index { tab.pane_size.resize(index + 1, [1.0, 1.0]); }
                    tab.pane_size[index] = [
                        fields[3].parse::<f32>().unwrap_or(1.0).clamp(0.5, 1.0),
                        fields[4].parse::<f32>().unwrap_or(1.0).clamp(0.5, 2.0),
                    ];
                }
            }
            "A" if fields.len() >= 7 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.anchors.len() <= index { tab.anchors.resize(index + 1, None); }
                    if tab.offsets.len() <= index { tab.offsets.resize(index + 1, [0.0, 0.0]); }
                    if fields[3] != "-" && fields[4] != "-" {
                        tab.anchors[index] = Some([fields[3].parse().unwrap_or(0.5), fields[4].parse().unwrap_or(0.5)]);
                    }
                    tab.offsets[index] = [fields[5].parse().unwrap_or(0.0), fields[6].parse().unwrap_or(0.0)];
                }
            }
            "B" if fields.len() >= 5 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.anchors_second.len() <= index { tab.anchors_second.resize(index + 1, None); }
                    tab.anchors_second[index] = Some([
                        fields[3].parse().unwrap_or(0.5),
                        fields[4].parse().unwrap_or(0.5),
                    ]);
                }
            }
            _ => {}
        }
    }
    if tabs.is_empty() { return None; }
    if tabs.iter().all(|tab| tab.kind != WorkspaceTabKind::Viewer) { tabs.insert(0, WorkspaceTab::viewer()); }
    Some((tabs, active_id, next_id, last_image_tab_id))
}

struct PendingFileOperation {
    entries: Vec<ImageEntry>,
    destination: PathBuf,
    move_files: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConflictResolution {
    Replace,
    Skip,
    KeepBoth,
    Cancel,
}

fn operation_has_conflicts(operation: &PendingFileOperation) -> bool {
    let mut seen_targets = HashSet::new();
    operation.entries.iter().any(|entry| {
        let target = operation.destination.join(entry_display_name(entry));
        let duplicate_in_batch = !seen_targets.insert(target.clone());
        let source_is_target = matches!(entry, ImageEntry::File(path) if operation.move_files && *path == target);
        (target.exists() && !source_is_target) || duplicate_in_batch
    })
}

#[derive(Clone)]
enum MasonryAction {
    Open(ImageEntry),
    OpenNewTab(ImageEntry),
    ToggleSelection(String),
    CompareEntries(Vec<ImageEntry>),
    CopyEntries(Vec<ImageEntry>),
    MoveEntries(Vec<ImageEntry>),
    DeleteEntries(Vec<ImageEntry>),
    AddToQueue(PathBuf),
    AddToFavorites(PathBuf),
    Rename(PathBuf),
    Reveal(PathBuf),
    OpenWith(PathBuf),
    OpenArchive(PathBuf),
    CopyText(String),
    OpenSelected,
    ViewSelected,
    Compare,
    Copy,
    Move,
    Delete,
}

fn entry_source_available(entry: &ImageEntry) -> bool {
    match entry {
        ImageEntry::File(path) => path.is_file(),
        ImageEntry::Zip(item) => item.archive_path.is_file(),
        ImageEntry::S7z(item) => item.archive_path.is_file(),
        ImageEntry::Rar(item) => item.archive_path.is_file(),
    }
}

fn welcome_paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn sort_method_label(method: crate::imaging::sort::SortMethod) -> &'static str {
    match method {
        crate::imaging::sort::SortMethod::Natural => "Name (natural)",
        crate::imaging::sort::SortMethod::DateNewest => "Newest first",
        crate::imaging::sort::SortMethod::DateOldest => "Oldest first",
        crate::imaging::sort::SortMethod::Shuffle => "Random",
    }
}

fn entry_display_name(entry: &ImageEntry) -> String {
    match entry {
        ImageEntry::File(path) => path.file_name().and_then(|name| name.to_str()).unwrap_or("Image").to_owned(),
        ImageEntry::Zip(item) => Path::new(&item.name).file_name().and_then(|name| name.to_str()).unwrap_or(item.name.as_str()).to_owned(),
        ImageEntry::S7z(item) => Path::new(&item.name).file_name().and_then(|name| name.to_str()).unwrap_or(item.name.as_str()).to_owned(),
        ImageEntry::Rar(item) => Path::new(&item.name).file_name().and_then(|name| name.to_str()).unwrap_or(item.name.as_str()).to_owned(),
    }
}

fn entry_parent(entry: &ImageEntry) -> Option<PathBuf> {
    match entry {
        ImageEntry::File(path) => path.parent().map(Path::to_path_buf),
        ImageEntry::Zip(item) => item.archive_path.parent().map(Path::to_path_buf),
        ImageEntry::S7z(item) => item.archive_path.parent().map(Path::to_path_buf),
        ImageEntry::Rar(item) => item.archive_path.parent().map(Path::to_path_buf),
    }
}

fn entry_breadcrumb(entry: &ImageEntry, root: Option<&Path>) -> String {
    let (container, inner) = match entry {
        ImageEntry::File(path) => (path.parent(), None),
        ImageEntry::Zip(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
        ImageEntry::S7z(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
        ImageEntry::Rar(item) => (Some(item.archive_path.as_path()), Some(item.name.as_str())),
    };
    let mut parts = Vec::new();
    if let Some(container) = container {
        let display = root.and_then(|root| container.strip_prefix(root).ok()).unwrap_or(container);
        let value = display.to_string_lossy();
        if !value.is_empty() && value != "." { parts.push(value.into_owned()); }
    }
    if let Some(inner) = inner {
        if let Some(parent) = Path::new(inner).parent() {
            if !parent.as_os_str().is_empty() { parts.push(parent.to_string_lossy().into_owned()); }
        }
    }
    if parts.is_empty() { "Current folder".to_owned() } else { parts.join(" / ") }
}

fn unique_destination_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("image");
    let extension = path.extension().and_then(|value| value.to_str());
    for suffix in 2..10_000u32 {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() { return candidate; }
    }
    parent.join(format!("{stem}-copy"))
}

fn unique_sidecar_path(path: &Path, purpose: &str) -> PathBuf {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("image");
    for _ in 0..10_000 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.jpeg-viewer-{purpose}-{}-{id}", std::process::id()));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(".{name}.jpeg-viewer-{purpose}-fallback"))
}

fn stage_entry_to_file(entry: &ImageEntry, staged: &Path) -> Result<(), String> {
    match entry {
        ImageEntry::File(source) => fs::copy(source, staged)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        archive_entry => {
            let bytes = read_archive_entry(archive_entry)?;
            fs::write(staged, bytes).map_err(|error| error.to_string())
        }
    }
}

fn commit_staged_file(staged: &Path, destination: &Path, allow_replace: bool) -> Result<(), String> {
    let backup = if destination.exists() {
        if !allow_replace {
            return Err("the destination appeared during the operation; no file was overwritten".into());
        }
        if !destination.is_file() {
            return Err("the destination exists and is not a regular file".into());
        }
        let backup = unique_sidecar_path(destination, "backup");
        fs::rename(destination, &backup)
            .map_err(|error| format!("could not preserve the existing destination: {error}"))?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = fs::rename(staged, destination) {
        if let Some(backup) = backup.as_ref() {
            if let Err(restore_error) = fs::rename(backup, destination) {
                return Err(format!("could not install replacement ({error}) or restore the original ({restore_error}); backup remains at {}", backup.display()));
            }
        }
        return Err(format!("could not install staged file: {error}"));
    }

    if let Some(backup) = backup {
        if let Err(error) = fs::remove_file(&backup) {
            // The new destination is committed. Preserve the backup and surface
            // its path instead of reporting the replacement as a total failure.
            return Err(format!("destination was replaced, but backup cleanup failed: {} ({error})", backup.display()));
        }
    }
    Ok(())
}

fn read_archive_entry(entry: &ImageEntry) -> Result<Vec<u8>, String> {
    match entry {
        ImageEntry::File(path) => fs::read(path).map_err(|error| error.to_string()),
        ImageEntry::Zip(item) => {
            let file = fs::File::open(&item.archive_path).map_err(|error| error.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
            let mut source = archive.by_index(item.entry_index).map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
            Ok(bytes)
        }
        ImageEntry::S7z(item) => {
            let mut archive = sevenz_rust2::ArchiveReader::open(&item.archive_path, sevenz_rust2::Password::empty()).map_err(|error| error.to_string())?;
            archive.read_file(&item.name).map_err(|error| error.to_string())
        }
        ImageEntry::Rar(item) => {
            let archive = unrar::Archive::new(&item.archive_path).open_for_processing().map_err(|error| error.to_string())?;
            let mut archive = archive;
            loop {
                let header = archive.read_header().map_err(|error| error.to_string())?.ok_or_else(|| format!("{} not found in archive", item.name))?;
                let name = header.entry().filename.to_string_lossy().to_string();
                if name == item.name {
                    let (bytes, _) = header.read().map_err(|error| error.to_string())?;
                    return Ok(bytes);
                }
                archive = header.skip().map_err(|error| error.to_string())?;
            }
        }
    }
}
