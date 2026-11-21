//! Workspace session serialization, restoration, and restore prompts.

use super::helpers::*;
use super::*;

impl crate::app::types::ViewerApp {
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
        let preference = self
            .settings_manager
            .get()
            .session_restore_preference
            .as_str();
        if preference == "never" {
            return;
        }
        let path = self
            .settings_manager
            .path()
            .with_file_name("workspace_session.txt");
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

    pub(super) fn apply_workspace_session(&mut self, session: RestorableSession) {
        let (mut tabs, active_id, next_id, last_image_tab_id) = session;
        // Home is a transient launch surface, not part of a restored
        // work session. Restoring must go directly to the saved workspaces.
        tabs.retain(|tab| tab.kind != WorkspaceTabKind::Welcome);
        // Older sessions could contain multiple Masonry tabs. Keep the active
        // one when possible, otherwise the first, to migrate to the singleton model.
        let keep_masonry_id = tabs
            .iter()
            .find(|tab| tab.kind == WorkspaceTabKind::Masonry && tab.id == active_id)
            .or_else(|| {
                tabs.iter()
                    .find(|tab| tab.kind == WorkspaceTabKind::Masonry)
            })
            .map(|tab| tab.id);
        tabs.retain(|tab| tab.kind != WorkspaceTabKind::Masonry || Some(tab.id) == keep_masonry_id);
        if tabs.iter().all(|tab| tab.kind != WorkspaceTabKind::Viewer) {
            tabs.insert(0, WorkspaceTab::viewer());
        }
        // Folder workspaces are refreshed from disk. Local workspaces retain
        // their explicit entry list, as selected files must not expand to a folder.
        for tab in &mut tabs {
            if tab.kind == WorkspaceTabKind::Masonry {
                tab.masonry_tile_size = self
                    .settings_manager
                    .get()
                    .masonry_thumbnail_size
                    .clamp(76.0, 512.0);
            }
            if tab.kind == WorkspaceTabKind::Masonry
                && tab.folder_masonry
                && let Some(directory) = tab.directory.as_ref()
            {
                let paths = if tab.recursive {
                    let mut paths = crate::app::queues::collect_folder_images(directory);
                    crate::imaging::sort::sort_images(&mut paths, self.sort_method);
                    paths
                } else {
                    crate::imaging::loader::load_directory_images(directory, self.sort_method)
                };
                tab.entries = paths.into_iter().map(ImageEntry::File).collect();
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
                    if !entry_source_available(&entry) {
                        continue;
                    }
                    tab.entries.push(entry);
                    tab.anchors.push(old_anchors.get(index).copied().flatten());
                    tab.anchors_second
                        .push(old_anchors_second.get(index).copied().flatten());
                    tab.offsets
                        .push(old_offsets.get(index).copied().unwrap_or([0.0, 0.0]));
                    tab.pane_zoom
                        .push(old_zoom.get(index).copied().unwrap_or(1.0));
                    tab.pane_sync
                        .push(old_sync.get(index).copied().unwrap_or(true));
                    tab.pane_size
                        .push(old_size.get(index).copied().unwrap_or([1.0, 1.0]));
                }
            } else {
                tab.entries.retain(entry_source_available);
                tab.current_index = tab.current_index.min(tab.entries.len().saturating_sub(1));
            }
            let valid_ids = tab
                .entries
                .iter()
                .map(ImageEntry::get_id)
                .collect::<HashSet<_>>();
            tab.selected.retain(|id| valid_ids.contains(id));
        }
        // Drop restored one-image tabs whose source no longer exists; Compare
        // panes have already been pruned in lockstep with their anchor settings.
        tabs.retain(|tab| tab.kind != WorkspaceTabKind::Image || !tab.entries.is_empty());
        let last_image_tab_id = last_image_tab_id.filter(|id| {
            tabs.iter()
                .any(|tab| tab.id == *id && tab.kind == WorkspaceTabKind::Image)
        });
        self.workspace.tabs = tabs;
        self.workspace.home_page_visible = false;
        self.workspace.active_index = usize::MAX;
        self.workspace.next_id = next_id.max(
            self.workspace
                .tabs
                .iter()
                .map(|tab| tab.id)
                .max()
                .unwrap_or(1)
                + 1,
        );
        self.workspace.last_image_tab_id = last_image_tab_id;
        // The Masonry return target is intentionally transient; establish it from
        // the next navigation action instead of retaining an ID from an old session.
        self.workspace.masonry_return_tab_id = None;
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
        let path = self
            .settings_manager
            .path()
            .with_file_name("workspace_session.txt");
        if self.settings_manager.get().session_restore_preference == "never" {
            let _ = fs::remove_file(&path);
            return;
        }
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("Could not create workspace session directory: {error}");
            return;
        }
        let mut output = String::new();
        let active_tab = self.workspace.tabs.get(self.workspace.active_index);
        let active_id = active_tab
            .filter(|tab| tab.kind != WorkspaceTabKind::Welcome)
            .map(|tab| tab.id)
            .or_else(|| {
                self.workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.kind == WorkspaceTabKind::Viewer)
                    .map(|tab| tab.id)
            })
            .unwrap_or(1);
        output.push_str(&format!(
            "ACTIVE|{active_id}\nNEXT|{}\nLASTIMAGE|{}\n",
            self.workspace.next_id,
            self.workspace
                .last_image_tab_id
                .map(|id| id.to_string())
                .unwrap_or_default()
        ));
        for tab in &self.workspace.tabs {
            if tab.kind == WorkspaceTabKind::Welcome {
                continue;
            }
            output.push_str(&format!(
                "T|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                tab.id,
                tab_kind_code(tab.kind),
                encode_field(&tab.title),
                tab.current_index,
                tab.directory
                    .as_ref()
                    .map(|path| encode_field(&path.to_string_lossy()))
                    .unwrap_or_default(),
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
                tab.compare_empty_panes,
            ));
            for entry in &tab.entries {
                match entry {
                    ImageEntry::File(path) => output.push_str(&format!(
                        "E|{}|F|{}\n",
                        tab.id,
                        encode_field(&path.to_string_lossy())
                    )),
                    ImageEntry::Zip(item) => output.push_str(&format!(
                        "E|{}|Z|{}|{}|{}\n",
                        tab.id,
                        encode_field(&item.archive_path.to_string_lossy()),
                        item.entry_index,
                        encode_field(&item.name)
                    )),
                    ImageEntry::S7z(item) => output.push_str(&format!(
                        "E|{}|7|{}|{}\n",
                        tab.id,
                        encode_field(&item.archive_path.to_string_lossy()),
                        encode_field(&item.name)
                    )),
                    ImageEntry::Rar(item) => output.push_str(&format!(
                        "E|{}|R|{}|{}\n",
                        tab.id,
                        encode_field(&item.archive_path.to_string_lossy()),
                        encode_field(&item.name)
                    )),
                }
            }
            for selected in &tab.selected {
                output.push_str(&format!("S|{}|{}\n", tab.id, encode_field(selected)));
            }
            for (index, zoom) in tab.pane_zoom.iter().enumerate() {
                output.push_str(&format!("Z|{}|{}|{}\n", tab.id, index, zoom));
            }
            for (index, rotation) in tab.pane_rotation.iter().enumerate() {
                output.push_str(&format!("R|{}|{}|{}\n", tab.id, index, rotation));
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
                let (ax, ay) = anchor
                    .map(|point| (point[0].to_string(), point[1].to_string()))
                    .unwrap_or_else(|| ("-".into(), "-".into()));
                output.push_str(&format!(
                    "A|{}|{}|{}|{}|{}|{}\n",
                    tab.id, index, ax, ay, offset[0], offset[1]
                ));
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
            if had_previous && let Err(backup_error) = fs::rename(&path, &backup) {
                eprintln!("Could not back up previous workspace session ({error}; {backup_error})");
                let _ = fs::remove_file(&temporary);
                return;
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => {
                    if had_previous {
                        let _ = fs::remove_file(backup);
                    }
                }
                Err(retry_error) => {
                    if had_previous {
                        let _ = fs::rename(&backup, &path);
                    }
                    eprintln!("Could not replace workspace session ({error}; {retry_error})");
                }
            }
        }
    }
}

pub(super) fn tab_kind_code(kind: WorkspaceTabKind) -> &'static str {
    match kind {
        WorkspaceTabKind::Viewer => "V",
        WorkspaceTabKind::Image => "I",
        WorkspaceTabKind::Masonry => "M",
        WorkspaceTabKind::Compare => "C",
        WorkspaceTabKind::Welcome => "W",
    }
}

pub(super) fn parse_tab_kind(code: &str) -> Option<WorkspaceTabKind> {
    match code {
        "V" => Some(WorkspaceTabKind::Viewer),
        "I" => Some(WorkspaceTabKind::Image),
        "M" => Some(WorkspaceTabKind::Masonry),
        "C" => Some(WorkspaceTabKind::Compare),
        "W" => Some(WorkspaceTabKind::Welcome),
        _ => None,
    }
}

pub(super) fn encode_field(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

pub(super) fn decode_field(value: &str) -> String {
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let Ok(hex) = std::str::from_utf8(pair) else {
            return String::new();
        };
        let Ok(byte) = u8::from_str_radix(hex, 16) else {
            return String::new();
        };
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(super) fn load_workspace_session(path: &Path) -> Option<RestorableSession> {
    let contents = fs::read_to_string(path).ok()?;
    let mut tabs: Vec<WorkspaceTab> = Vec::new();
    let mut active_id = 1_u64;
    let mut next_id = 2_u64;
    let mut last_image_tab_id = None;
    for line in contents.lines() {
        let fields = line.split('|').collect::<Vec<_>>();
        match fields.first().copied().unwrap_or_default() {
            "ACTIVE" => active_id = fields.get(1)?.parse().ok()?,
            "NEXT" => {
                next_id = fields
                    .get(1)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(2)
            }
            "LASTIMAGE" => last_image_tab_id = fields.get(1).and_then(|value| value.parse().ok()),
            "T" if fields.len() >= 21 => {
                let id = fields[1].parse::<u64>().ok()?;
                let kind = parse_tab_kind(fields[2])?;
                if kind == WorkspaceTabKind::Welcome {
                    continue;
                }
                let title = decode_field(fields[3]);
                let mut tab = WorkspaceTab::new(id, title, kind);
                tab.current_index = fields[4].parse().unwrap_or(0);
                tab.directory = if fields[5].is_empty() {
                    None
                } else {
                    Some(PathBuf::from(decode_field(fields[5])))
                };
                tab.zoom = fields[6].parse::<f32>().unwrap_or(1.0).clamp(0.01, 1000.0);
                tab.pan = egui::vec2(
                    fields[7].parse().unwrap_or(0.0),
                    fields[8].parse().unwrap_or(0.0),
                );
                tab.fit_to_window = fields[9] == "true";
                tab.zoom_used = fields[10] == "true";
                tab.recursive = fields[11] == "true";
                tab.folder_masonry = fields[12] == "true";
                tab.masonry_tile_size = fields[13]
                    .parse::<f32>()
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
                tab.compare_columns = fields
                    .get(21)
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0)
                    .min(4);
                tab.compare_rows = fields.get(22).is_some_and(|value| *value == "true");
                tab.compare_empty_panes = fields
                    .get(23)
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0)
                    .min(16);
                tabs.push(tab);
            }
            "E" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let entry = match fields[2] {
                    "F" => ImageEntry::File(PathBuf::from(decode_field(fields[3]))),
                    "Z" if fields.len() >= 6 => {
                        ImageEntry::Zip(crate::imaging::entry::ArchiveImage {
                            archive_path: PathBuf::from(decode_field(fields[3])),
                            entry_index: fields[4].parse().unwrap_or(0),
                            name: decode_field(fields[5]),
                        })
                    }
                    "7" if fields.len() >= 5 => {
                        ImageEntry::S7z(crate::imaging::entry::S7ArchiveImage {
                            archive_path: PathBuf::from(decode_field(fields[3])),
                            name: decode_field(fields[4]),
                        })
                    }
                    "R" if fields.len() >= 5 => {
                        ImageEntry::Rar(crate::imaging::entry::RarArchiveImage {
                            archive_path: PathBuf::from(decode_field(fields[3])),
                            name: decode_field(fields[4]),
                        })
                    }
                    _ => continue,
                };
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    tab.entries.push(entry);
                }
            }
            "S" if fields.len() >= 3 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    tab.selected.insert(decode_field(fields[2]));
                }
            }
            "Z" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_zoom.len() <= index {
                        tab.pane_zoom.resize(index + 1, 1.0);
                    }
                    tab.pane_zoom[index] =
                        fields[3].parse::<f32>().unwrap_or(1.0).clamp(0.01, 100.0);
                }
            }
            "R" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_rotation.len() <= index {
                        tab.pane_rotation.resize(index + 1, 0.0);
                    }
                    tab.pane_rotation[index] = fields[3]
                        .parse::<f32>()
                        .unwrap_or(0.0)
                        .clamp(-std::f32::consts::PI, std::f32::consts::PI);
                }
            }
            "P" if fields.len() >= 4 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_sync.len() <= index {
                        tab.pane_sync.resize(index + 1, true);
                    }
                    tab.pane_sync[index] = fields[3] != "false";
                }
            }
            "Q" if fields.len() >= 5 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.pane_size.len() <= index {
                        tab.pane_size.resize(index + 1, [1.0, 1.0]);
                    }
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
                    if tab.anchors.len() <= index {
                        tab.anchors.resize(index + 1, None);
                    }
                    if tab.offsets.len() <= index {
                        tab.offsets.resize(index + 1, [0.0, 0.0]);
                    }
                    if fields[3] != "-" && fields[4] != "-" {
                        tab.anchors[index] = Some([
                            fields[3].parse().unwrap_or(0.5),
                            fields[4].parse().unwrap_or(0.5),
                        ]);
                    }
                    tab.offsets[index] = [
                        fields[5].parse().unwrap_or(0.0),
                        fields[6].parse().unwrap_or(0.0),
                    ];
                }
            }
            "B" if fields.len() >= 5 => {
                let tab_id = fields[1].parse::<u64>().ok()?;
                let index = fields[2].parse::<usize>().unwrap_or(0);
                if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if tab.anchors_second.len() <= index {
                        tab.anchors_second.resize(index + 1, None);
                    }
                    tab.anchors_second[index] = Some([
                        fields[3].parse().unwrap_or(0.5),
                        fields[4].parse().unwrap_or(0.5),
                    ]);
                }
            }
            _ => {}
        }
    }
    if tabs.is_empty() {
        return None;
    }
    if tabs.iter().all(|tab| tab.kind != WorkspaceTabKind::Viewer) {
        tabs.insert(0, WorkspaceTab::viewer());
    }
    Some((tabs, active_id, next_id, last_image_tab_id))
}
