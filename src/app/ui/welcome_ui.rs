//! Firefox-style new-tab / Welcome surface.
use crate::app::types::ViewerApp;
use crate::input::shortcuts::ViewerCommand;
use eframe::egui;
use std::path::{Path, PathBuf};

enum WelcomeAction {
    OpenFile,
    OpenFolder,
    Favorites,
    OpenPath(PathBuf),
    RemovePin(PathBuf),
    AddImages,
    AddFolder,
    AddPlaylist,
}

impl ViewerApp {
    /// Render the Welcome workspace. The welcome content is deliberately not
    /// used as the empty-state UI for ordinary Viewer tabs.
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.workspace_session_prompt_active() {
            return;
        }

        let recent = self.welcome_recent_paths();
        let pinned = self.welcome_pinned_paths();
        self.ensure_favorites_loaded();
        let favorites_count = self.favorite_paths.len();
        let mut action: Option<WelcomeAction> = None;

        egui::ScrollArea::vertical()
            .id_salt("welcome_new_tab_content")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available_width = ui.available_width().max(280.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    if let Some(icon) = &self.logo_texture {
                        ui.image((icon.id(), egui::vec2(104.0, 104.0)));
                    }
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new("Welcome to JPEG Viewer").size(27.0));
                    ui.label(
                        egui::RichText::new("A clean place to pick up where you left off.")
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(20.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_sized([142.0, 38.0], egui::Button::new("Open images…")).clicked() {
                            action = Some(WelcomeAction::OpenFile);
                        }
                        if ui.add_sized([142.0, 38.0], egui::Button::new("Open folder…")).clicked() {
                            action = Some(WelcomeAction::OpenFolder);
                        }
                        if ui.add_enabled(
                            favorites_count > 0,
                            egui::Button::new(format!("Favorites · {favorites_count}"))
                                .min_size(egui::vec2(142.0, 38.0)),
                        ).clicked() {
                            action = Some(WelcomeAction::Favorites);
                        }
                    });
                });

                ui.add_space(30.0);
                let content_width = (available_width - 56.0).max(220.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(available_width, ui.available_height().max(0.0)),
                    egui::Layout::top_down(egui::Align::Min),
                    |content| {
                        content.add_space(4.0);
                        content.heading("Recently opened");
                        content.add_space(8.0);
                        if recent.is_empty() {
                            content.label(
                                egui::RichText::new("Images and folders you open will appear here.")
                                    .color(content.visuals().weak_text_color()),
                            );
                        } else {
                            content.horizontal_wrapped(|row| {
                                row.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                                for path in recent.iter().take(12) {
                                    let exists = path.exists();
                                    let label = welcome_path_label(path);
                                    let kind = welcome_path_kind(path);
                                    let card = egui::Frame::group(row.style())
                                        .fill(row.visuals().faint_bg_color)
                                        .inner_margin(egui::Margin::same(10));
                                    card.show(row, |card_ui| {
                                        card_ui.set_min_width(154.0);
                                        card_ui.set_max_width(210.0);
                                        let response = card_ui.add_enabled(
                                            exists,
                                            egui::Button::new(format!("{kind}\n{label}"))
                                                .min_size(egui::vec2(134.0, 56.0)),
                                        );
                                        if response.clicked() {
                                            action = Some(WelcomeAction::OpenPath(path.clone()));
                                        }
                                        response.on_hover_text(path.display().to_string());
                                    });
                                }
                            });
                        }

                        content.add_space(26.0);
                        content.horizontal(|row| {
                            row.heading("Your shortcuts");
                            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                                if right.button("＋ Add playlist").clicked() {
                                    action = Some(WelcomeAction::AddPlaylist);
                                }
                                if right.button("＋ Add folder").clicked() {
                                    action = Some(WelcomeAction::AddFolder);
                                }
                                if right.button("＋ Add images").clicked() {
                                    action = Some(WelcomeAction::AddImages);
                                }
                            });
                        });
                        content.add_space(8.0);
                        if pinned.is_empty() {
                            content.label(
                                egui::RichText::new("Pin images, folders, or .jvq playlists here for quick access.")
                                    .color(content.visuals().weak_text_color()),
                            );
                        } else {
                            content.horizontal_wrapped(|row| {
                                row.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                                for path in &pinned {
                                    let exists = path.exists();
                                    let label = welcome_path_label(path);
                                    let kind = welcome_path_kind(path);
                                    egui::Frame::group(row.style())
                                        .fill(row.visuals().faint_bg_color)
                                        .inner_margin(egui::Margin::same(10))
                                        .show(row, |card_ui| {
                                            card_ui.set_min_width(154.0);
                                            card_ui.set_max_width(220.0);
                                            card_ui.horizontal(|card_row| {
                                                let response = card_row.add_enabled(
                                                    exists,
                                                    egui::Button::new(format!("{kind}  {label}"))
                                                        .min_size(egui::vec2(118.0, 38.0)),
                                                );
                                                if response.clicked() {
                                                    action = Some(WelcomeAction::OpenPath(path.clone()));
                                                }
                                                response.on_hover_text(path.display().to_string());
                                                if card_row.small_button("×").on_hover_text("Remove shortcut").clicked() {
                                                    action = Some(WelcomeAction::RemovePin(path.clone()));
                                                }
                                            });
                                        });
                                }
                            });
                        }

                        content.add_space(24.0);
                        content.separator();
                        content.add_space(8.0);
                        content.label(
                            egui::RichText::new("Tip: use the + button on the tab bar to open another Welcome page.")
                                .small()
                                .color(content.visuals().weak_text_color()),
                        );
                        // Keep a comfortable maximum width on very wide displays.
                        content.set_max_width(content_width);
                    },
                );
            });

        match action {
            Some(WelcomeAction::OpenFile) => {
                self.handle_command(ctx, ViewerCommand::OpenFile);
            }
            Some(WelcomeAction::OpenFolder) => self.open_folder_dialog(),
            Some(WelcomeAction::Favorites) => self.load_favorites_from_welcome(ctx),
            Some(WelcomeAction::OpenPath(path)) => self.open_welcome_shortcut(path, ctx),
            Some(WelcomeAction::RemovePin(path)) => self.remove_welcome_pin(&path),
            Some(WelcomeAction::AddImages) => {
                let extensions = crate::imaging::core::ImageFormat::all_extensions();
                self.dialog_open = true;
                let paths = rfd::FileDialog::new()
                    .add_filter("Supported images", &extensions)
                    .pick_files();
                self.dialog_open = false;
                if let Some(paths) = paths {
                    for path in paths { self.add_welcome_pin(path); }
                }
            }
            Some(WelcomeAction::AddFolder) => {
                self.dialog_open = true;
                let path = rfd::FileDialog::new().pick_folder();
                self.dialog_open = false;
                if let Some(path) = path { self.add_welcome_pin(path); }
            }
            Some(WelcomeAction::AddPlaylist) => {
                self.dialog_open = true;
                let path = rfd::FileDialog::new()
                    .add_filter("JPEG Viewer playlists", &["jvq", "txt"])
                    .pick_file();
                self.dialog_open = false;
                if let Some(path) = path { self.add_welcome_pin(path); }
            }
            None => {}
        }
    }

    /// The session prompt is intentionally checked before rendering Welcome so
    /// the startup restoration flow does not spend a frame building new-tab UI.
    pub(crate) fn workspace_session_prompt_active(&self) -> bool {
        self.workspace.session_prompt
    }
}

fn welcome_path_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn welcome_path_kind(path: &Path) -> &'static str {
    if path.is_dir() {
        "📁 Folder"
    } else if matches!(path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase().as_str(), "jvq" | "txt") {
        "▤ Playlist"
    } else {
        "▧ Image"
    }
}
