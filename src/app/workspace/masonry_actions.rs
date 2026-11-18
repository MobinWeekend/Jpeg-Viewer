//! Actions that open Masonry entries or dispatch commands from the grid.

use super::*;
use super::helpers::*;

impl crate::app::types::ViewerApp {
    pub(super) fn open_masonry_entry_in_viewer(
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
        // Opening a Masonry card is an explicit request to view an image. Clear
        // Home mode before the primary Viewer renders, otherwise a previously
        // visible Home page can cover the selected image after the workspace switch.
        self.workspace.home_page_visible = false;
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

    pub(super) fn handle_masonry_action(&mut self, tab_index: usize, action: MasonryAction, ctx: &egui::Context) {
        let selected_entries = {
            let tab = &self.workspace.tabs[tab_index];
            tab.entries.iter().filter(|entry| tab.selected.contains(&entry.get_id())).cloned().collect::<Vec<_>>()
        };
        match action {
            MasonryAction::Open(entry) => self.open_masonry_entry_in_viewer(tab_index, &entry, ctx),
            MasonryAction::OpenNewTab(entry) => self.open_entry_in_image_tab(entry, true),
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
                self.workspace.home_page_visible = false;
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
}
