//! Workspace state and the browser-style Viewer, Home, Masonry, and Compare surfaces.
//!
//! The original ViewerApp remains the renderer/decoder for the primary image viewer.
//! This module owns workspace data and delegates each substantial workflow to a
//! focused child module so tab management, browsing, file operations, and comparison
//! behavior can be maintained independently.

use crate::app::types::LoadedImage;
use crate::app::virtual_texture::VirtualTexture;
use crate::imaging::entry::ImageEntry;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;

const WELCOME_RECENT_LIMIT: usize = 8;

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
    /// Folder groups the user has collapsed in this Masonry workspace.
    pub collapsed_folders: HashSet<String>,
    /// Preferred Masonry tile width in logical points; columns are auto-filled.
    pub masonry_tile_size: f32,
    /// Image aspect ratios retained independently of thumbnail texture residency.
    /// LRU eviction must never change a tile's dimensions or Masonry column layout.
    pub masonry_aspect_ratios: HashMap<String, f32>,
    /// Original pixel dimensions discovered in the fast metadata-first pass.
    /// Kept separately from thumbnail texture residency and aspect ratios.
    pub masonry_dimensions: HashMap<String, (u32, u32)>,

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
    /// Per-pane clockwise image rotation in radians, used to align corresponding anchors.
    pub pane_rotation: Vec<f32>,
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
            collapsed_folders: HashSet::new(),
            masonry_tile_size: 220.0,
            masonry_aspect_ratios: HashMap::new(),
            masonry_dimensions: HashMap::new(),
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
            pane_rotation: Vec::new(),
            pane_sync: Vec::new(),
            pane_size: Vec::new(),
            compare_columns: 0,
            compare_rows: false,
        }
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
    /// Workspace to return to after leaving the singleton Masonry view.
    pub masonry_return_tab_id: Option<u64>,
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
    /// Recently opened folders, saved queue lists, and archives displayed on Home pages.
    pub(crate) welcome_recent: Vec<PathBuf>,
    /// User-pinned images, folders, and .jvq playlists displayed on Home pages.
    pub(crate) welcome_pinned: Vec<PathBuf>,
    /// Resolved Masonry entries for Home card previews, cached to avoid rescanning
    /// folders, archives, and queue lists on every immediate-mode UI frame.
    pub(crate) welcome_thumbnail_sources: HashMap<PathBuf, Option<ImageEntry>>,
    welcome_data_loaded: bool,
    /// The Home surface is shown inside the primary Viewer workspace, never as a tab.
    pub(crate) home_page_visible: bool,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self {
            tabs: vec![WorkspaceTab::viewer()],
            active_index: 0,
            next_id: 2,
            last_image_tab_id: None,
            masonry_return_tab_id: None,
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
            welcome_thumbnail_sources: HashMap::new(),
            welcome_data_loaded: false,
            home_page_visible: true,
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
        matches!(
            self.active_kind(),
            WorkspaceTabKind::Viewer | WorkspaceTabKind::Image
        )
    }

    fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tabs.get(self.active_index)
    }

    pub(crate) fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.tabs.get_mut(self.active_index)
    }

    /// Cancel any in-progress tab-strip drag before starting a global window gesture.
    pub(crate) fn cancel_tab_drag(&mut self) {
        self.dragging_tab_id = None;
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
pub(crate) type RestorableSession = (Vec<WorkspaceTab>, u64, u64, Option<u64>);

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

/// Actions emitted by Masonry cards, context menus, and selection controls.
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

// Workspace lifecycle and tab navigation.
mod core;
mod new_tab;
mod session;
mod tabs;

// Masonry presentation and actions.
mod file_operations;
mod masonry;
mod masonry_actions;
mod masonry_group;

// Compare rendering and shared entry/file helpers.
mod compare;
mod helpers;
