//! Firefox-style Home surface.
use crate::app::types::ViewerApp;
use crate::imaging::entry::ImageEntry;
use crate::input::shortcuts::ViewerCommand;
use eframe::egui;
use std::path::{Path, PathBuf};

struct NewTabCard {
    path: PathBuf,
    label: String,
    kind: &'static str,
    thumbnail: Option<egui::TextureHandle>,
}

enum WelcomeAction {
    OpenFile,
    OpenFolder,
    Favorites,
    OpenPath(PathBuf),
    OpenMasonryPath(PathBuf),
    RefreshThumbnail(PathBuf),
    RemoveRecent(PathBuf),
    RemovePin(PathBuf),
    ReorderPins(PathBuf, PathBuf, bool),
    AddImages,
    AddFolder,
    AddQueues,
}

impl ViewerApp {
    /// Render the Home workspace. The new-tab content is deliberately not
    /// used as the empty-state UI for ordinary Viewer tabs.
    pub fn render_welcome_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.workspace_session_prompt_active() {
            return;
        }

        // Let the first Home shell reach the platform before even starting
        // background workers. Saved links and any slow path checks are loaded
        // from the next frame onward, without delaying initial window display.
        if self.workspace.welcome_first_frame_rendered {
            self.request_welcome_data_load(ctx);
            self.collect_welcome_data_load();
        } else {
            self.workspace.welcome_first_frame_rendered = true;
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // Home is a launch surface rather than a document editor: prevent
        // accidental text selection, and use otherwise-empty page space as a
        // native window drag region. Interactive cards/buttons are registered
        // later and therefore keep priority over this background response.
        ui.style_mut().interaction.selectable_labels = false;
        let background_drag = ui.interact(
            ui.max_rect(),
            egui::Id::new("home_background_window_drag"),
            egui::Sense::click_and_drag(),
        );
        if background_drag.double_clicked() {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        } else if background_drag.drag_started_by(egui::PointerButton::Primary)
            && !self.window_mouse_operation_active
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            #[cfg(target_os = "linux")]
            crate::platform::wayland_pointer_recovery::after_native_window_operation(ctx);
        }

        // Keep background Masonry dimension results current, while the shared
        // thumbnail collector returns immediately when no thumbnail jobs exist.
        self.collect_home_preview_jobs(ctx);
        self.collect_sidebar_thumbnails(ctx);
        let new_tab_settings = self.settings_manager.get().clone();
        let recent_rows = new_tab_settings.new_tab_recent_rows.clamp(0, 2);
        let show_recent = recent_rows > 0;
        let recent_limit = (recent_rows * 6).min(8);
        let recent_paths = if show_recent {
            self.welcome_recent_paths()
        } else {
            Vec::new()
        };
        let pinned_paths = self.welcome_pinned_paths();
        let favorites_label = if self.favorites_loaded {
            format!("Favorites · {}", self.favorite_paths.len())
        } else {
            "Favorites".to_owned()
        };
        // Pass this into the free-standing card renderers so they can avoid
        // initiating shortcut drags during a global window move/resize gesture.
        let window_mouse_operation_active = self.window_mouse_operation_active;
        // Match the target to the actual new-tab preview viewport (six-column
        // card width and the 82pt preview height), converted to physical pixels.
        let page_width = ui.available_width().min(900.0).max(1.0);
        let card_gap = 10.0_f32.min((page_width / 12.0).max(0.0));
        let card_width = ((page_width - card_gap * 5.0) / 6.0).clamp(1.0, 174.0);
        let preview_side = (card_width - 8.0).max(1.0).max(82.0);
        let target_thumbnail_size = (preview_side * ctx.pixels_per_point())
            .ceil()
            .max(1.0)
            .min(u32::MAX as f32) as u32;
        let mut action: Option<WelcomeAction> = None;

        egui::ScrollArea::vertical()
            .id_salt("welcome_new_tab_content")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available_width = ui.available_width().max(1.0);
                let content_width = available_width.min(900.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(available_width, ui.available_height().max(0.0)),
                    egui::Layout::top_down(egui::Align::Center),
                    |container| {
                        container.set_min_width(available_width);
                        container.set_max_width(available_width);
                        container.allocate_ui_with_layout(
                            egui::vec2(content_width, container.available_height().max(0.0)),
                            egui::Layout::top_down(egui::Align::Min),
                            |page| {
                                page.set_min_width(content_width);
                                page.set_max_width(content_width);
                                let settings = self.settings_manager.get();
                                let glass = crate::ui_theme::os_background_blur_enabled(ctx);
                                let hero_surface = if settings.tricks_backdrop_enabled {
                                    let mut color = crate::ui_theme::main_view_background_color(
                                        &settings.main_view_background,
                                        ctx.style().visuals.dark_mode,
                                    );
                                    if glass {
                                        let [r, g, b, _] = color.to_srgba_unmultiplied();
                                        let opacity = (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0)
                                            .round() as u8;
                                        color = egui::Color32::from_rgba_unmultiplied(r, g, b, opacity);
                                    }
                                    color
                                } else {
                                    crate::ui_theme::title_bar_surface_color(
                                        ctx,
                                        if !glass {
                                            255
                                        } else if settings.title_bar_color_mode == "main_view" {
                                            (settings.main_view_background_opacity.clamp(0.15, 1.0) * 255.0).round() as u8
                                        } else {
                                            match crate::ui_theme::current_look_profile(ctx) {
                                                "gnome" => 222,
                                                "cosmic" => 218,
                                                _ => 198,
                                            }
                                        },
                                        &settings.title_bar_color_mode,
                                        &settings.main_view_background,
                                    )
                                };
                                let frame_stroke = if glass {
                                    egui::Stroke::new(1.0_f32, page.visuals().window_stroke.color)
                                } else {
                                    page.visuals().widgets.noninteractive.bg_stroke
                                };

                                page.add_space(42.0);
                                egui::Frame::new()
                                    .fill(hero_surface)
                                    .stroke(frame_stroke)
                                    .corner_radius(egui::CornerRadius::same(16))
                                    .inner_margin(egui::Margin::same(20))
                                    .show(page, |hero| {
                                        hero.set_min_width((content_width - 42.0).max(0.0));
                                        hero.horizontal(|hero| {
                                            if let Some(icon) = &self.logo_texture {
                                                hero.image((icon.id(), egui::vec2(160.0, 160.0)));
                                            } else {
                                                let (logo_rect, _) = hero.allocate_exact_size(
                                                    egui::vec2(160.0, 160.0),
                                                    egui::Sense::hover(),
                                                );
                                                hero.painter().text(
                                                    logo_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "▣",
                                                    egui::FontId::proportional(54.0),
                                                    hero.visuals().text_color(),
                                                );
                                            }
                                            hero.add_space(22.0);
                                            hero.vertical(|content| {
                                                content.set_min_width((content.available_width()).max(1.0));
                                                content.horizontal(|heading| {
                                                    let (icon_rect, _) = heading.allocate_exact_size(
                                                        egui::vec2(30.0, 30.0), egui::Sense::hover(),
                                                    );
                                                    crate::app::ui::draw_home_icon(
                                                        heading.painter(), icon_rect,
                                                        heading.visuals().text_color(),
                                                    );
                                                    heading.label(egui::RichText::new("Jpeg Viewer").size(29.0).strong());
                                                    heading.label(
                                                        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                                            .size(11.0)
                                                            .color(heading.visuals().weak_text_color()),
                                                    );
                                                });
                                                content.label(
                                                    egui::RichText::new("Pick up where you left off, or open something new.")
                                                        .color(content.visuals().weak_text_color()),
                                                );
                                                content.add_space(32.0);
                                                content.horizontal_wrapped(|buttons| {
                                                    buttons.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                                                    if buttons.add_sized([150.0, 38.0], egui::Button::new("📄 Open File")).clicked() {
                                                        action = Some(WelcomeAction::OpenFile);
                                                    }
                                                    if buttons.add_sized([150.0, 38.0], egui::Button::new("📁 Open folder")).clicked() {
                                                        action = Some(WelcomeAction::OpenFolder);
                                                    }
                                                    if crate::app::ui::masonry_button(
                                                        buttons,
                                                        &favorites_label,
                                                        true,
                                                        150.0,
                                                        false,
                                                        true,
                                                    ).clicked() {
                                                        action = Some(WelcomeAction::Favorites);
                                                    }
                                                });
                                            });
                                        });
                                    });

                                // Keep the Home hierarchy consistent at every window
                                // size: header first, then full-width shortcuts, then the
                                // optional Recently opened section. Both grids use six-card rows.
                                page.add_space(26.0);
                                let shortcuts_action = render_shortcuts_section(
                                    self,
                                    page,
                                    &pinned_paths,
                                    ctx,
                                    target_thumbnail_size,
                                    window_mouse_operation_active,
                                );

                                let mut recent_action: Option<WelcomeAction> = None;
                                if show_recent {
                                    page.add_space(26.0);
                                    recent_action = render_recent_section(
                                        self,
                                        page,
                                        &recent_paths[..recent_paths.len().min(recent_limit)],
                                        recent_rows,
                                        ctx,
                                        target_thumbnail_size,
                                        window_mouse_operation_active,
                                    );
                                }

                                if recent_action.is_some() {
                                    action = recent_action;
                                }
                                if shortcuts_action.is_some() {
                                    action = shortcuts_action;
                                }

                                page.add_space(20.0);
                                page.separator();
                                page.add_space(8.0);
                                page.label(
                                    egui::RichText::new("Tip: use Home from the application menu to return here at any time.")
                                        .small()
                                        .color(page.visuals().weak_text_color()),
                                );
                                page.add_space(18.0);
                            },
                        );
                    },
                );
            });

        match action {
            Some(WelcomeAction::OpenFile) => {
                self.handle_command(ctx, ViewerCommand::OpenFile);
            }
            Some(WelcomeAction::OpenFolder) => self.open_folder_dialog(),
            Some(WelcomeAction::Favorites) => self.load_favorites_from_welcome(),
            Some(WelcomeAction::OpenPath(path)) => self.open_welcome_shortcut(path, ctx),
            Some(WelcomeAction::OpenMasonryPath(path)) => self.open_welcome_path_in_masonry(path),
            Some(WelcomeAction::RefreshThumbnail(path)) => {
                self.refresh_home_thumbnail(&path, target_thumbnail_size, ctx);
            }
            Some(WelcomeAction::RemoveRecent(path)) => self.remove_welcome_recent(&path),
            Some(WelcomeAction::RemovePin(path)) => self.remove_welcome_pin(&path),
            Some(WelcomeAction::ReorderPins(from, to, before)) => {
                self.reorder_welcome_pins(&from, &to, before)
            }
            Some(WelcomeAction::AddImages) => {
                let extensions = crate::imaging::core::ImageFormat::all_extensions();
                self.dialog_open = true;
                let paths = rfd::FileDialog::new()
                    .add_filter("Supported images", &extensions)
                    .pick_files();
                self.dialog_open = false;
                if let Some(paths) = paths {
                    for path in paths {
                        self.add_welcome_pin(path);
                    }
                }
            }
            Some(WelcomeAction::AddFolder) => {
                self.dialog_open = true;
                let path = rfd::FileDialog::new().pick_folder();
                self.dialog_open = false;
                if let Some(path) = path {
                    self.add_welcome_pin(path);
                }
            }
            Some(WelcomeAction::AddQueues) => {
                self.dialog_open = true;
                let path = rfd::FileDialog::new()
                    .add_filter("JPEG Viewer queues", &["jvq", "txt"])
                    .pick_file();
                self.dialog_open = false;
                if let Some(path) = path {
                    self.add_welcome_pin(path);
                }
            }
            None => {}
        }
    }

    /// Build cards only for a row which intersects the current Home viewport.
    /// Path/queue/archive resolution and thumbnail requests therefore happen
    /// on demand instead of for every pinned/recent entry at startup.
    fn build_new_tab_cards(
        &mut self,
        paths: &[PathBuf],
        ctx: &egui::Context,
        target_thumbnail_size: u32,
    ) -> Vec<NewTabCard> {
        paths
            .iter()
            .map(|path| {
                // Resolving a folder's first image or scanning an archive/queue can
                // involve substantial filesystem work. Start it on a worker and
                // render the card immediately with its lightweight placeholder.
                let preview_entry = if self.workspace.welcome_thumbnail_sources.contains_key(path) {
                    self.workspace
                        .welcome_thumbnail_sources
                        .get(path)
                        .cloned()
                        .flatten()
                } else {
                    if self.workspace.welcome_preview_pending.insert(path.clone()) {
                        self.request_home_preview_job(
                            Some(path.clone()),
                            None,
                            target_thumbnail_size,
                            ctx,
                        );
                    }
                    None
                };
                let thumbnail = preview_entry.and_then(|entry| {
                    self.home_thumbnail_for_entry(entry, target_thumbnail_size, ctx)
                });
                NewTabCard {
                    path: path.clone(),
                    label: welcome_path_label(path),
                    kind: welcome_path_kind(
                        path,
                        self.workspace
                            .welcome_path_is_directory
                            .get(path)
                            .copied(),
                    ),
                    thumbnail,
                }
            })
            .collect()
    }

    fn request_home_preview_job(
        &mut self,
        source_path: Option<PathBuf>,
        known_entry: Option<ImageEntry>,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let request_key = known_entry.as_ref().map(|entry| {
            let source_key = crate::app::queues::source_thumbnail_key(entry);
            crate::app::queues::shared_thumbnail_key(&source_key, target_size)
        });
        if let Some(request_key) = request_key.as_ref() {
            if self.home_thumbnail_cache_checked.contains(request_key)
                || !self.home_thumbnail_cache_pending.insert(request_key.clone())
            {
                return;
            }
            self.home_thumbnail_cache_checked.insert(request_key.clone());
        }

        let settings_path = self.settings_manager.path().to_path_buf();
        let worker_path = source_path.clone();
        let worker_entry = known_entry;
        let repaint_ctx = ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        crate::app::home_thumbnail_cache::spawn(move || {
            // Directory enumeration, archive scanning, cache metadata checks,
            // and cache-file reads are all deliberately outside the UI thread.
            let source_is_directory = worker_path
                .as_deref()
                .is_some_and(|path| path.is_dir());
            let entry = worker_entry.or_else(|| {
                worker_path
                    .as_deref()
                    .and_then(welcome_preview_entry)
            });
            let cache_path = entry.as_ref().and_then(|entry| {
                crate::app::home_thumbnail_cache::cache_path(
                    &settings_path,
                    entry,
                    target_size,
                )
            });
            let cached = cache_path
                .as_deref()
                .and_then(crate::app::home_thumbnail_cache::load);
            let _ = tx.send(crate::app::home_thumbnail_cache::HomePreviewTaskResult {
                source_path: worker_path,
                source_is_directory,
                target_size,
                entry,
                cache_path,
                cached,
            });
            repaint_ctx.request_repaint();
        });
        self.home_preview_receivers
            .push((source_path, request_key, rx));
    }

    /// Apply only a small number of completed Home cache reads per frame. The
    /// worker performs all disk access; this function only creates egui texture
    /// handles for successful cache hits and updates in-memory bookkeeping.
    fn collect_home_preview_jobs(&mut self, ctx: &egui::Context) {
        const MAX_TEXTURE_UPLOADS_PER_FRAME: usize = 2;
        // Move the receiver list out before processing it. This avoids holding
        // a mutable borrow of one ViewerApp field while updating other fields
        // and querying the shared thumbnail cache.
        let receivers = std::mem::take(&mut self.home_preview_receivers);
        let mut remaining = Vec::with_capacity(receivers.len());
        let mut uploads = 0;
        let mut upload_limit_reached = false;
        let mut forced_refreshes = Vec::new();

        for (source_path, request_key, receiver) in receivers {
            if uploads >= MAX_TEXTURE_UPLOADS_PER_FRAME {
                remaining.push((source_path, request_key, receiver));
                upload_limit_reached = true;
                continue;
            }
            match receiver.try_recv() {
                Ok(result) => {
                    if let Some(path) = result.source_path.as_ref() {
                        self.workspace.welcome_preview_pending.remove(path);
                        self.workspace
                            .welcome_path_is_directory
                            .insert(path.clone(), result.source_is_directory);
                        self.workspace
                            .welcome_thumbnail_sources
                            .insert(path.clone(), result.entry.clone());
                    }
                    if let Some(request_key) = request_key.as_ref() {
                        self.home_thumbnail_cache_pending.remove(request_key);
                    }

                    if let Some(entry) = result.entry.as_ref() {
                        let source_key = crate::app::queues::source_thumbnail_key(entry);
                        let cache_key = crate::app::queues::shared_thumbnail_key(
                            &source_key,
                            result.target_size,
                        );
                        self.home_thumbnail_cache_checked.insert(cache_key.clone());
                        if let Some(cache_path) = result.cache_path {
                            self.home_thumbnail_cache_paths
                                .insert(cache_key.clone(), cache_path.clone());

                            if self.home_thumbnail_refresh_requested.contains(&cache_key) {
                                // A forced refresh replaces the old cache atomically
                                // after a successful thumbnail generation.
                                self.home_thumbnail_cache_writes
                                    .insert(cache_key.clone(), cache_path);
                            }
                        }

                        if self.home_thumbnail_refresh_requested.remove(&cache_key) {
                            forced_refreshes.push((entry.clone(), result.target_size));
                        } else if let Some(cached) = result.cached
                            && !self.has_shared_thumbnail_at_least(
                                &source_key,
                                result.target_size,
                            )
                        {
                            let texture_key = crate::app::queues::shared_thumbnail_key(
                                &source_key,
                                cached.resolution,
                            );
                            let color = egui::ColorImage::from_rgba_unmultiplied(
                                [cached.width, cached.height],
                                &cached.pixels,
                            );
                            let texture = ctx.load_texture(
                                format!("home_thumb_{texture_key}"),
                                color,
                                egui::TextureOptions::LINEAR,
                            );
                            if self.sidebar_thumbnails.len() >= 256
                                && let Some((evicted_key, _texture)) =
                                    self.sidebar_thumbnails.pop_lru()
                            {
                                crate::app::queues::remove_attempts_for_cached_source(
                                    &mut self.sidebar_thumbnail_attempted,
                                    &evicted_key,
                                );
                            }
                            self.sidebar_thumbnails.put(texture_key, texture);
                            uploads += 1;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    remaining.push((source_path, request_key, receiver));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(path) = source_path {
                        self.workspace.welcome_preview_pending.remove(&path);
                        // Treat an unexpected worker exit as a failed discovery
                        // instead of repeatedly spawning the same failing scan.
                        self.workspace
                            .welcome_thumbnail_sources
                            .insert(path, None);
                    }
                    if let Some(key) = request_key {
                        self.home_thumbnail_cache_pending.remove(&key);
                        if self.home_thumbnail_refresh_requested.remove(&key) {
                            // Permit a later visible frame to retry the cache lookup
                            // after an unexpected worker exit.
                            self.home_thumbnail_cache_checked.remove(&key);
                        }
                    }
                }
            }
        }

        self.home_preview_receivers = remaining;
        for (entry, target_size) in forced_refreshes {
            let source_key = crate::app::queues::source_thumbnail_key(&entry);
            let cache_key = crate::app::queues::shared_thumbnail_key(&source_key, target_size);
            if let Some(cache_path) = self.home_thumbnail_cache_paths.get(&cache_key).cloned() {
                self.home_thumbnail_cache_writes.insert(cache_key, cache_path);
            }
            match entry {
                ImageEntry::File(path) => {
                    self.request_sidebar_thumbnail(path, source_key, target_size, ctx);
                }
                archive_entry => {
                    self.request_sidebar_archive_thumbnail(archive_entry, source_key, target_size, ctx);
                }
            }
        }
        if upload_limit_reached {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn home_thumbnail_for_entry(
        &mut self,
        entry: ImageEntry,
        target_size: u32,
        ctx: &egui::Context,
    ) -> Option<egui::TextureHandle> {
        let key = crate::app::queues::source_thumbnail_key(&entry);
        let request_key = crate::app::queues::shared_thumbnail_key(&key, target_size);

        // Cache path resolution and reading can block on removable/network drives.
        // Do both on the Home preview worker, then resume thumbnail generation only
        // after the disk-cache lookup has completed.
        if !self.home_thumbnail_cache_checked.contains(&request_key)
            && !self.home_thumbnail_cache_pending.contains(&request_key)
        {
            self.request_home_preview_job(None, Some(entry.clone()), target_size, ctx);
        }

        if self.home_thumbnail_cache_pending.contains(&request_key) {
            return self.get_shared_thumbnail(&key, target_size);
        }

        // Home is intentionally cache-first: once any cached resolution exists,
        // keep it instead of scheduling a background quality upgrade. Missing
        // thumbnails are generated once and then persisted in the Home cache.
        if self.get_shared_thumbnail(&key, target_size).is_none() {
            let cache_path = self.home_thumbnail_cache_paths.get(&request_key).cloned();
            if self.sidebar_thumbnail_pending.contains(&request_key) {
                // Another workspace may already be generating this exact key.
                // Mark it as Home-owned too so its completion is persisted.
                if let Some(cache_path) = cache_path {
                    self.home_thumbnail_cache_writes
                        .insert(request_key.clone(), cache_path);
                }
            } else if !self.sidebar_thumbnail_failed.contains(&request_key)
                && !self.sidebar_thumbnail_attempted.contains(&request_key)
            {
                if let Some(cache_path) = cache_path {
                    self.home_thumbnail_cache_writes
                        .insert(request_key.clone(), cache_path);
                }
                match entry {
                    ImageEntry::File(path) => self.request_sidebar_thumbnail(
                        path,
                        key.clone(),
                        target_size,
                        ctx,
                    ),
                    archive_entry => self.request_sidebar_archive_thumbnail(
                        archive_entry,
                        key.clone(),
                        target_size,
                        ctx,
                    ),
                }
            }
        }

        self.get_shared_thumbnail(&key, target_size)
    }

    /// Explicitly refresh a visible Home card. Normal Home rendering never
    /// regenerates a thumbnail once any suitable cached resolution is available.
    /// A user-requested refresh evicts that source's shared textures and ignores
    /// the old persistent-cache result before queuing one replacement.
    fn refresh_home_thumbnail(
        &mut self,
        path: &Path,
        target_size: u32,
        ctx: &egui::Context,
    ) {
        let Some(entry) = self
            .workspace
            .welcome_thumbnail_sources
            .get(path)
            .cloned()
            .flatten()
        else {
            return;
        };

        let source_key = crate::app::queues::source_thumbnail_key(&entry);
        let request_key = crate::app::queues::shared_thumbnail_key(&source_key, target_size);
        self.invalidate_shared_thumbnails_for_source(&source_key);
        self.home_thumbnail_cache_checked.remove(&request_key);
        self.home_thumbnail_refresh_requested.insert(request_key.clone());

        // If a cache read is already running, collect_home_preview_jobs will
        // ignore the stale cache hit and queue the replacement when it returns.
        // Otherwise start a fresh lookup so it can also resolve the write path.
        if !self.home_thumbnail_cache_pending.contains(&request_key) {
            self.request_home_preview_job(None, Some(entry), target_size, ctx);
        }
        ctx.request_repaint();
    }

    /// The session prompt is intentionally checked before rendering Welcome so
    /// the startup restoration flow does not spend a frame building new-tab UI.
    pub(crate) fn workspace_session_prompt_active(&self) -> bool {
        self.workspace.session_prompt
    }
}

fn render_recent_section(
    app: &mut ViewerApp,
    ui: &mut egui::Ui,
    recent_paths: &[PathBuf],
    recent_rows: usize,
    ctx: &egui::Context,
    target_thumbnail_size: u32,
    window_mouse_operation_active: bool,
) -> Option<WelcomeAction> {
    let mut selected_action = None;
    let recent_limit = (recent_rows.clamp(1, 2) * 6).min(8);
    ui.horizontal(|header| {
        header.heading("Recently opened");
        header.add_space(8.0);
        header.label(
            egui::RichText::new(format!("{} / {}", recent_paths.len(), recent_limit))
                .small()
                .color(header.visuals().weak_text_color()),
        );
    });
    ui.add_space(10.0);
    if recent_paths.is_empty() {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |empty| {
                empty.label(
                    egui::RichText::new(
                        "Folders, saved queues, and archives you open will appear here.",
                    )
                    .color(empty.visuals().weak_text_color()),
                );
            });
    } else {
        let visible_paths = &recent_paths[..recent_paths.len().min(recent_limit)];
        let (card_width, row_gap) = new_tab_card_layout(ui);
        for (row_index, row) in visible_paths.chunks(6).enumerate() {
            if row_index > 0 {
                ui.add_space(10.0);
            }
            let row_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width().max(1.0), 132.0),
            );
            if ui.is_rect_visible(row_rect) {
                let cards = app.build_new_tab_cards(row, ctx, target_thumbnail_size);
                ui.horizontal(|row_ui| {
                    row_ui.spacing_mut().item_spacing = egui::vec2(row_gap, 10.0);
                    for card in &cards {
                        let (opened, _, menu_action) = render_new_tab_card(
                            row_ui,
                            card,
                            false,
                            card_width,
                            window_mouse_operation_active,
                        );
                        if opened {
                            selected_action = Some(WelcomeAction::OpenPath(card.path.clone()));
                        }
                        if menu_action.is_some() {
                            selected_action = menu_action;
                        }
                    }
                });
            } else {
                let row_width = ui.available_width().max(1.0);
                ui.allocate_space(egui::vec2(row_width, 132.0));
            }
        }
    }
    selected_action
}

fn render_shortcuts_section(
    app: &mut ViewerApp,
    ui: &mut egui::Ui,
    pinned_paths: &[PathBuf],
    ctx: &egui::Context,
    target_thumbnail_size: u32,
    window_mouse_operation_active: bool,
) -> Option<WelcomeAction> {
    let mut action = None;
    ui.horizontal_wrapped(|header| {
        header.heading("Your shortcuts");
        if header.button("+ Queues").clicked() {
            action = Some(WelcomeAction::AddQueues);
        }
        if header.button("+ Folder").clicked() {
            action = Some(WelcomeAction::AddFolder);
        }
        if header.button("+ Images").clicked() {
            action = Some(WelcomeAction::AddImages);
        }
    });
    ui.add_space(10.0);
    if pinned_paths.is_empty() {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |empty| {
                empty.label(
                    egui::RichText::new(
                        "Pin images, folders, or .jvq Queues here for quick access.",
                    )
                    .color(empty.visuals().weak_text_color()),
                );
            });
    } else {
        let (card_width, row_gap) = new_tab_card_layout(ui);
        for (row_index, row) in pinned_paths.chunks(6).enumerate() {
            if row_index > 0 {
                ui.add_space(10.0);
            }
            let row_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width().max(1.0), 132.0),
            );
            if ui.is_rect_visible(row_rect) {
                let cards = app.build_new_tab_cards(row, ctx, target_thumbnail_size);
                ui.horizontal(|row_ui| {
                    row_ui.spacing_mut().item_spacing = egui::vec2(row_gap, 10.0);
                    for card in &cards {
                        let (opened, removed, menu_action) = render_new_tab_card(
                            row_ui,
                            card,
                            true,
                            card_width,
                            window_mouse_operation_active,
                        );
                        if removed {
                            action = Some(WelcomeAction::RemovePin(card.path.clone()));
                        } else if opened {
                            action = Some(WelcomeAction::OpenPath(card.path.clone()));
                        }
                        if menu_action.is_some() {
                            action = menu_action;
                        }
                    }
                });
            } else {
                let row_width = ui.available_width().max(1.0);
                ui.allocate_space(egui::vec2(row_width, 132.0));
            }
        }
    }
    if ui.ctx().input(|input| input.pointer.any_released()) {
        ui.ctx().data_mut(|data| {
            data.remove::<PathBuf>(egui::Id::new("home_shortcut_drag_source"));
        });
    }
    action
}

fn welcome_path_matches(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

/// Six cards are laid out in every row. Both card width and inter-card gap
/// contract with the page so the rows stay inside the centered Home column.
fn new_tab_card_layout(ui: &egui::Ui) -> (f32, f32) {
    let available_width = ui.available_width().max(1.0);
    let gap = 10.0_f32.min((available_width / 12.0).max(0.0));
    let card_width = ((available_width - gap * 5.0) / 6.0).clamp(1.0, 174.0);
    (card_width, gap)
}

fn render_new_tab_card(
    ui: &mut egui::Ui,
    card: &NewTabCard,
    removable: bool,
    card_width: f32,
    window_mouse_operation_active: bool,
) -> (bool, bool, Option<WelcomeAction>) {
    let mut removed = false;
    let mut menu_action = None;
    let visuals = ui.visuals().clone();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(card_width, 132.0),
        if removable {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        },
    );
    // Use the same theme-driven hover state and short animation as Masonry's
    // thumbnail cards. The hover fill belongs to the card surface; it must not
    // become a translucent wash over the image or reduce label contrast.
    // `card.path` is not unique: a shortcut can also appear in Recently opened,
    // and history may contain the same folder/queue more than once. Derive the
    // animation ID from this widget's unique egui response ID so cards never
    // drive one another's hover animation.
    let card_id = response.id.with(("new-tab-card", card.path.clone()));
    const SHORTCUT_DRAG_ID: &str = "home_shortcut_drag_source";
    let shortcut_drag_id = egui::Id::new(SHORTCUT_DRAG_ID);
    if removable && response.drag_started() && !window_mouse_operation_active {
        ui.ctx()
            .data_mut(|data| data.insert_temp(shortcut_drag_id, card.path.clone()));
    }
    let dragging_path = if window_mouse_operation_active {
        None
    } else {
        ui.ctx()
            .data(|data| data.get_temp::<PathBuf>(shortcut_drag_id))
    };
    let is_drop_target = removable
        && dragging_path
            .as_ref()
            .is_some_and(|from| !welcome_path_matches(from, &card.path))
        && response.hovered();
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(card_id.with("hover-animation"), response.hovered(), 0.14);
    let card_fill = if hover_t > 0.01 {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.noninteractive.bg_fill
    };
    let painter = ui.painter();
    if hover_t > 0.01 {
        let shadow_alpha = (hover_t * 34.0).round() as u8;
        painter.rect_filled(
            rect.translate(egui::vec2(0.0, 2.0 + hover_t)),
            9.0,
            egui::Color32::from_black_alpha(shadow_alpha),
        );
    }
    painter.rect_filled(rect, egui::CornerRadius::same(9), card_fill);
    let card_text_color = crate::ui_theme::contrast_foreground(card_fill);
    let card_weak_text_color = if crate::ui_theme::contrast_prefers_light_text(card_fill) {
        egui::Color32::from_rgb(220, 225, 232)
    } else {
        egui::Color32::from_rgb(78, 82, 88)
    };

    let preview_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(4.0, 4.0),
        egui::vec2((rect.width() - 8.0).max(1.0), 82.0),
    );
    painter.rect_filled(
        preview_rect,
        egui::CornerRadius::same(7),
        visuals.extreme_bg_color,
    );
    if let Some(texture) = &card.thumbnail {
        let size = texture.size_vec2();
        if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
            let image_rect = crate::app::queues::thumbnail_fit_rect(preview_rect, size.x / size.y);
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    } else {
        let glyph = if card.kind.contains("Folder") {
            "📁"
        } else if card.kind.contains("Queue") {
            "▤"
        } else {
            "▧"
        };
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(34.0),
            card_weak_text_color,
        );
    }

    let max_chars = ((rect.width() / 7.2).floor() as usize).clamp(5, 21);
    let title = truncate_card_label(&card.label, max_chars);
    painter
        .with_clip_rect(egui::Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, preview_rect.bottom() + 6.0),
            egui::pos2(rect.right() - 6.0, rect.bottom() - 20.0),
        ))
        .text(
            egui::pos2(rect.left() + 7.0, preview_rect.bottom() + 7.0),
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::proportional(12.5),
            card_text_color,
        );
    painter.text(
        egui::pos2(rect.left() + 7.0, rect.bottom() - 7.0),
        egui::Align2::LEFT_BOTTOM,
        card.kind,
        egui::FontId::proportional(10.5),
        card_weak_text_color,
    );

    if removable {
        let remove_rect = egui::Rect::from_center_size(
            egui::pos2(preview_rect.right() - 10.0, preview_rect.top() + 10.0),
            egui::vec2(22.0, 22.0),
        );
        let remove_response = ui.interact(
            remove_rect,
            card_id.with("remove-pin-control"),
            egui::Sense::click(),
        );
        painter.circle_filled(
            remove_rect.center(),
            9.5,
            if remove_response.hovered() {
                visuals.widgets.hovered.bg_fill
            } else {
                visuals.panel_fill
            },
        );
        painter.text(
            remove_rect.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::FontId::proportional(15.0),
            visuals.text_color(),
        );
        removed = remove_response.clicked();
    }

    let response = response.on_hover_text(card.path.display().to_string());
    response.clone().context_menu(|menu| {
        if menu.button("Open in Viewer").clicked() {
            menu_action = Some(WelcomeAction::OpenPath(card.path.clone()));
            menu.close();
        }
        if menu.button("Open in Masonry").clicked() {
            menu_action = Some(WelcomeAction::OpenMasonryPath(card.path.clone()));
            menu.close();
        }
        if card.thumbnail.is_some() && menu.button("Refresh thumbnail").clicked() {
            menu_action = Some(WelcomeAction::RefreshThumbnail(card.path.clone()));
            menu.close();
        }
        menu.separator();
        if menu.button("Remove from list").clicked() {
            menu_action = Some(if removable {
                WelcomeAction::RemovePin(card.path.clone())
            } else {
                WelcomeAction::RemoveRecent(card.path.clone())
            });
            menu.close();
        }
    });
    let opened = response.clicked() && card.path.exists();
    let border_color = if is_drop_target {
        crate::ui_theme::main_accent_color(ui.ctx())
    } else if hover_t > 0.01 {
        visuals.widgets.hovered.bg_stroke.color
    } else {
        visuals.widgets.noninteractive.bg_stroke.color
    };
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(9),
        egui::Stroke::new(
            if is_drop_target {
                2.5
            } else {
                1.0 + hover_t * 0.5
            },
            border_color,
        ),
        egui::StrokeKind::Inside,
    );
    if is_drop_target {
        let pointer_x = ui
            .ctx()
            .pointer_interact_pos()
            .map_or(rect.center().x, |pos| pos.x);
        let marker_x = if pointer_x < rect.center().x {
            rect.left() + 3.0
        } else {
            rect.right() - 3.0
        };
        painter.line_segment(
            [
                egui::pos2(marker_x, rect.top() + 10.0),
                egui::pos2(marker_x, rect.bottom() - 10.0),
            ],
            egui::Stroke::new(3.0_f32, crate::ui_theme::main_accent_color(ui.ctx())),
        );
    }

    if removable
        && is_drop_target
        && ui.ctx().input(|input| input.pointer.any_released())
        && let Some(from) = dragging_path
    {
        let pointer_x = ui
            .ctx()
            .pointer_interact_pos()
            .map_or(rect.center().x, |pos| pos.x);
        let before = pointer_x < rect.center().x;
        menu_action = Some(WelcomeAction::ReorderPins(from, card.path.clone(), before));
    }

    (opened && !is_drop_target, removed, menu_action)
}

fn welcome_preview_entry(path: &Path) -> Option<ImageEntry> {
    welcome_preview_entry_at_depth(path, 0)
}

fn welcome_preview_entry_at_depth(path: &Path, depth: u8) -> Option<ImageEntry> {
    if depth > 2 {
        return None;
    }
    if path.is_dir() {
        return crate::app::queues::collect_folder_images(path)
            .into_iter()
            .next()
            .map(ImageEntry::File);
    }
    if crate::imaging::helpers::is_supported_image(path) {
        return Some(ImageEntry::File(path.to_path_buf()));
    }

    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "zip" => crate::imaging::archive::scan_zip(path).into_iter().next(),
        "7z" => crate::imaging::archive::scan_7z(path).into_iter().next(),
        "rar" => crate::imaging::archive::scan_rar(path).into_iter().next(),
        "jvq" | "txt" => {
            let (paths, _) = crate::app::queues::read_queue(path).ok()?;
            paths
                .into_iter()
                .find_map(|item| welcome_preview_entry_at_depth(&item, depth + 1))
        }
        _ => None,
    }
}

fn truncate_card_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        label.to_owned()
    } else {
        format!(
            "{}…",
            label
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

fn welcome_path_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn welcome_path_kind(path: &Path, is_directory: Option<bool>) -> &'static str {
    // Never query the filesystem from card rendering. A Home link can point to
    // a removable/network drive, so path-kind detection is part of the worker
    // that resolves its preview; unknown extensionless links get a folder hint.
    if is_directory == Some(true)
        || (is_directory.is_none() && path.extension().is_none())
    {
        "📁 Folder"
    } else {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "jvq" | "txt" => "▤ Queue",
            "zip" => "▧ ZIP archive",
            "7z" => "▧ 7Z archive",
            "rar" => "▧ RAR archive",
            _ => "▧ Image",
        }
    }
}
