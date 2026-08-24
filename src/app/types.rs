use super::virtual_texture::{PreparationProgress, VirtualTexture};
use crate::gif::animation::GifAnimation;
use crate::image_entry::ImageEntry;
use crate::settings::SettingsManager;
use crate::shortcuts::InputBindings;
use eframe::egui;
use image::DynamicImage;
use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

// ====== LOADED IMAGE ======

#[derive(Clone)]
pub enum LoadedImage {
    Static(DynamicImage),
    Animated(GifAnimation, bool),
    VirtualPending(Vec<u8>, u32, u32),
}

// ====== CACHE ======

#[derive(Clone)]
pub struct CachedImage {
    pub texture: egui::TextureHandle,
    pub is_gif: bool,
    pub is_preview: bool,
    pub index: usize,
    pub file_type_detection: Option<FileTypeDetection>,
}

// ====== FILE TYPE DETECTION ======

#[derive(Clone, Debug)]
pub struct FileTypeDetection {
    pub detected_extension: String,
    pub current_extension: Option<String>,
    pub mismatch: bool,
    pub index: usize,
    pub generation: u64,
}

// ====== PRELOAD ======

pub struct PreloadTask {
    pub receiver: Receiver<(usize, Result<LoadedImage, String>, u64)>,
    pub index: usize,
    pub start_time: Instant,
}

// ====== LOADING STATE ======

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Indexing,
    Loading,
    LoadingFullGif,
    VirtualTextureLoading,
}

// ====== VIEWER APP ======

pub struct ViewerApp {
    // ====== IMAGE ======
    pub texture: Option<egui::TextureHandle>,
    pub gif_animation: Option<GifAnimation>,
    pub receiver: Option<Receiver<Result<LoadedImage, String>>>,
    pub full_image_receiver: Option<Receiver<DynamicImage>>,
    pub full_gif_receiver: Option<Receiver<Result<LoadedImage, String>>>,

    pub current_directory: Option<PathBuf>,
    pub image_entries: Vec<ImageEntry>,
    pub current_index: usize,
    pub current_image_path: Option<PathBuf>,

    pub is_gif: bool,
    pub is_preview: bool,
    pub image_error: Option<String>,

    // ====== IMAGE VIEW ======
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub b_fit_to_window: bool,
    pub b_zoom_used: bool,
    pub image_rect: Option<egui::Rect>,
    pub last_window_size: Option<egui::Vec2>,
    pub zoom_drag_start_pos: Option<egui::Pos2>,
    pub zoom_drag_start_zoom: f32,
    pub zoom_drag_start_pan: egui::Vec2,
    pub zoom_drag_start_center: egui::Pos2,

    // ====== CACHE ======
    pub image_cache: LruCache<String, CachedImage>,
    pub max_cache_size: usize,

    // ====== PRELOAD ======
    pub preload_tasks: Vec<PreloadTask>,
    pub preloading_indices: HashSet<usize>,
    pub preload_skipped: HashSet<usize>,

    pub preload_generation: u64,
    pub preload_workers: usize,
    pub preload_working: bool,

    pub cache_radius: usize,
    pub preload_origin: usize,
    pub delta_threshold: usize,
    pub cache_delta_factor: f32,

    pub max_cache_task: u8,
    pub last_preload_start: Option<Instant>,

    pub should_stop_caching: bool,

    pub navigation_timer: Option<Instant>,
    pub navigation_pause_duration: Duration,

    // ====== FRAME LIMITER ======
    pub last_repaint_time: Instant,
    pub last_interaction_time: Instant,
    pub is_idle: bool,

    pub max_fps: f32,
    pub idle_fps_limit: f32,
    pub idle_timeout_ms: u64,

    pub unfocused_idle_timeout_ms: u64,
    pub unfocused_idle_fps_limit: f32,

    pub is_animating: bool,

    // ====== SLIDESHOW ======
    pub slideshow_enabled: bool,
    pub slideshow_interval: Duration,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    pub slideshow_last_advance: Instant,
    pub slideshow_has_advanced: bool,

    // ====== VIRTUAL TEXTURE ======
    pub virtual_texture: Option<VirtualTexture>,
    pub virtual_texture_thread: Option<std::thread::JoinHandle<VirtualTexture>>,
    pub vt_progress: Option<PreparationProgress>,
    pub vt_total_tiles: usize,

    // ====== FILE DETECTION ======
    pub file_type_detection: Option<FileTypeDetection>,

    // ====== SETTINGS ======
    pub settings_manager: SettingsManager,
    pub input_bindings: InputBindings,

    // ====== UI ======
    pub logo_texture: Option<egui::TextureHandle>,

    pub show_delete_confirmation: bool,
    pub delete_key_was_pressed: bool,

    pub show_settings_menu: bool,
    pub show_help_menu: bool,

    pub dialog_open: bool,
    pub hamburger_menu_open: bool,
    pub overlay_visible: bool,

    // ====== WINDOW ======
    pub startup_fullscreen_handled: bool,
    pub window_always_on_top: bool,

    // ====== FPS ======
    pub current_fps: f32,
    pub last_fps_update: Instant,

    // ====== INPUT ======
    pub b_ctrl_invert: bool,

    // ====== LOADING ======
    pub loading_state: LoadingState,
    pub indexing_receiver: Option<Receiver<(Vec<PathBuf>, Option<PathBuf>)>>,
}

impl Default for ViewerApp {
    fn default() -> Self {
        let settings_manager = SettingsManager::new();
        let settings = settings_manager.get().clone();

        // ====== CACHE SIZE ======

        let cache_radius = settings.cache_radius.clamp(1, 100);
        let desired_cache_size = cache_radius * 2 + 2;
        let cache_capacity = desired_cache_size.max(3);

        // ====== TIME ======

        let now = Instant::now();

        Self {
            // ====== IMAGE ======
            texture: None,
            gif_animation: None,
            receiver: None,
            full_image_receiver: None,
            full_gif_receiver: None,

            current_directory: None,
            image_entries: Vec::new(),
            current_index: 0,
            current_image_path: None,

            is_gif: false,
            is_preview: false,
            image_error: None,

            // ====== IMAGE VIEW ======
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            b_fit_to_window: false,
            b_zoom_used: false,
            image_rect: None,
            last_window_size: None,
            zoom_drag_start_pos: None,
            zoom_drag_start_zoom: 1.0,
            zoom_drag_start_pan: egui::Vec2::ZERO,
            zoom_drag_start_center: egui::Pos2::ZERO,

            // ====== CACHE ======
            image_cache: LruCache::new(
                NonZeroUsize::new(cache_capacity)
                    .expect("cache capacity must be greater than zero"),
            ),
            max_cache_size: cache_capacity,

            // ====== PRELOAD ======
            preload_tasks: Vec::new(),
            preloading_indices: HashSet::new(),
            preload_skipped: HashSet::new(),

            preload_generation: 0,
            preload_workers: 0,
            preload_working: false,

            cache_radius,
            preload_origin: 0,

            delta_threshold: ((cache_radius as f32 * settings.cache_delta_factor).round() as usize)
                .max(1),

            cache_delta_factor: settings.cache_delta_factor,

            max_cache_task: settings.max_cache_task,
            last_preload_start: None,

            should_stop_caching: false,

            navigation_timer: None,
            navigation_pause_duration: Duration::from_millis(settings.navigation_pause_ms),

            // ====== FRAME LIMITER ======
            last_repaint_time: now,
            last_interaction_time: now,
            is_idle: false,

            max_fps: settings.max_fps,
            idle_fps_limit: settings.idle_fps_limit,
            idle_timeout_ms: settings.idle_timeout_ms,

            unfocused_idle_timeout_ms: settings.unfocused_idle_timeout_ms,
            unfocused_idle_fps_limit: settings.unfocused_idle_fps_limit,

            is_animating: false,

            // ====== SLIDESHOW ======
            slideshow_enabled: settings.slideshow_enabled,
            slideshow_interval: Duration::from_millis(settings.slideshow_interval_ms),
            slideshow_loop: settings.slideshow_loop,
            slideshow_random: settings.slideshow_random,

            slideshow_last_advance: now,
            slideshow_has_advanced: false,

            // ====== VIRTUAL TEXTURE ======
            virtual_texture: None,
            virtual_texture_thread: None,
            vt_progress: None,
            vt_total_tiles: 0,

            // ====== FILE DETECTION ======
            file_type_detection: None,

            // ====== SETTINGS ======
            settings_manager,
            input_bindings: InputBindings::default(),

            // ====== UI ======
            logo_texture: None,

            show_delete_confirmation: false,
            delete_key_was_pressed: false,

            show_settings_menu: false,
            show_help_menu: false,

            dialog_open: false,
            hamburger_menu_open: false,
            overlay_visible: true,

            // ====== WINDOW ======
            startup_fullscreen_handled: false,
            window_always_on_top: false,

            // ====== FPS ======
            current_fps: 0.0,
            last_fps_update: now,

            // ====== INPUT ======
            b_ctrl_invert: settings.b_ctrl_invert,

            // ====== LOADING ======
            loading_state: LoadingState::Idle,
            indexing_receiver: None,
        }
    }
}

impl ViewerApp {
    // ====== LOADING STATE ======

    pub fn set_loading_state(&mut self, state: LoadingState) {
        self.loading_state = state;
    }

    pub fn is_loading(&self) -> bool {
        !matches!(self.loading_state, LoadingState::Idle)
    }

    pub fn is_loading_virtual(&self) -> bool {
        matches!(self.loading_state, LoadingState::VirtualTextureLoading)
    }

    // ====== IMAGE INFORMATION ======

    pub fn get_current_filename(&self) -> String {
        let Some(entry) = self.image_entries.get(self.current_index) else {
            return "JPEG Viewer".to_string();
        };

        match entry {
            ImageEntry::File(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),

            ImageEntry::Zip(zip) => zip.name.clone(),

            ImageEntry::S7z(s7z) => s7z.name.clone(),

            ImageEntry::Rar(rar) => rar.name.clone(),
        }
    }

    pub fn window_title(&self) -> String {
        let filename = self.get_current_filename();
        let total = self.image_entries.len();

        let slideshow_indicator = if self.slideshow_enabled { " ▶" } else { "" };

        if total == 0 {
            return "JPEG Viewer".to_string();
        }

        format!(
            "{}{} ({}/{}) - JPEG Viewer",
            filename,
            slideshow_indicator,
            self.current_index + 1,
            total
        )
    }

    pub fn update_window_title(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    // ====== TEXTURE ======

    pub fn get_texture_options(&self) -> egui::TextureOptions {
        match self.settings_manager.get().texture_filter.as_str() {
            "nearest" => egui::TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                mipmap_mode: None,
                ..Default::default()
            },

            "mipmap" => egui::TextureOptions {
                magnification: egui::TextureFilter::Linear,
                minification: egui::TextureFilter::Linear,
                mipmap_mode: Some(egui::TextureFilter::Linear),
                ..Default::default()
            },

            _ => egui::TextureOptions::LINEAR,
        }
    }

    // ====== SLIDESHOW ======

    pub fn toggle_slideshow(&mut self) {
        self.slideshow_enabled = !self.slideshow_enabled;

        if self.slideshow_enabled {
            self.slideshow_last_advance = Instant::now();
            self.slideshow_has_advanced = false;
        }

        let enabled = self.slideshow_enabled;

        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_enabled = enabled;
        });
    }

    pub fn slideshow_speed_up(&mut self) {
        let current = self.slideshow_interval.as_millis() as u64;

        let new_interval = (current.max(500) / 2).max(500);

        self.slideshow_interval = Duration::from_millis(new_interval);

        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval;
        });
    }

    pub fn slideshow_speed_down(&mut self) {
        let current = self.slideshow_interval.as_millis() as u64;

        let new_interval = current.saturating_mul(2).min(60_000);

        self.slideshow_interval = Duration::from_millis(new_interval);

        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval;
        });
    }

    pub fn load_slideshow_settings(&mut self) {
        let settings = self.settings_manager.get();

        self.slideshow_enabled = settings.slideshow_enabled;

        self.slideshow_interval = Duration::from_millis(settings.slideshow_interval_ms);

        self.slideshow_loop = settings.slideshow_loop;
        self.slideshow_random = settings.slideshow_random;
    }

    // ====== ASPECT RATIO ======

    /// Detect extremely wide or tall images.
    ///
    /// Kept as a private helper for possible future image-layout logic.
    #[allow(dead_code)]
    pub fn _has_extreme_aspect_ratio(&self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }

        let ratio = if width > height {
            height as f32 / width as f32
        } else {
            width as f32 / height as f32
        };

        ratio < 0.1
    }

    // ====== FULL GIF LOADING ======

    /// Spawn a background task to load all frames of a GIF.
    pub fn spawn_full_gif_loading(&mut self) {
        println!(
            "[spawn_full_gif_loading] called: \
             is_gif={}, is_preview={}, has_receiver={}, is_loading={}",
            self.is_gif,
            self.is_preview,
            self.full_gif_receiver.is_some(),
            self.is_loading(),
        );

        // ====== VALIDATION ======

        if !self.is_gif || !self.is_preview {
            println!("[spawn_full_gif_loading] Not a GIF preview, skipping");
            return;
        }

        if self.full_gif_receiver.is_some() || self.is_loading() {
            println!(
                "[spawn_full_gif_loading] \
                 skipping - conditions not met"
            );
            return;
        }

        // ====== CURRENT ENTRY ======

        let Some(entry) = self.image_entries.get(self.current_index).cloned() else {
            println!(
                "[spawn_full_gif_loading] \
                 no entry found for index {}",
                self.current_index
            );
            return;
        };

        // ====== CHANNEL ======

        let (tx, rx) = std::sync::mpsc::channel();

        self.full_gif_receiver = Some(rx);
        self.set_loading_state(LoadingState::LoadingFullGif);

        let index = self.current_index;

        println!("[spawn_full_gif_loading] spawning task for index {}", index);

        // ====== BACKGROUND TASK ======

        rayon::spawn(move || {
            println!(
                "[spawn_full_gif_loading] \
                 task started, about to call load_full_gif_from_entry"
            );

            let result = std::panic::catch_unwind(|| {
                crate::gif::loader::load_full_gif_from_entry(entry)
                    .map(|gif| LoadedImage::Animated(gif, false))
            });

            match result {
                Ok(Ok(loaded_image)) => {
                    println!(
                        "[spawn_full_gif_loading] \
                         load_full_gif_from_entry succeeded"
                    );

                    let _ = tx.send(Ok(loaded_image));
                }

                Ok(Err(error)) => {
                    eprintln!(
                        "[spawn_full_gif_loading] \
                         load_full_gif_from_entry error: {}",
                        error
                    );

                    let _ = tx.send(Err(error));
                }

                Err(panic) => {
                    eprintln!(
                        "[spawn_full_gif_loading] \
                         PANIC in full GIF loader: {:?}",
                        panic
                    );

                    let _ = tx.send(Err("Panic in full GIF loader".to_string()));
                }
            }

            println!("[spawn_full_gif_loading] task finished");
        });
    }
}
