use super::virtual_texture::PreparationProgress;
use crate::app::types::LoadingState::Idle;
use crate::app::virtual_texture::VirtualTexture;
use crate::gif::animation::GifAnimation;
use crate::image_entry::ImageEntry;
use crate::settings::SettingsManager;
use crate::shortcuts::InputBindings;
use eframe::egui;
use image::DynamicImage;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

// Define LoadedImage enum with Clone derive
#[derive(Clone)]
pub enum LoadedImage {
    Static(DynamicImage),
    Animated(GifAnimation, bool),
    VirtualPending(Vec<u8>, u32, u32), // raw bytes, width, height
}

// Cached image data
#[derive(Clone)]
pub struct CachedImage {
    pub texture: egui::TextureHandle,
    pub is_gif: bool,
    pub is_preview: bool,
    pub index: usize,
    pub file_type_detection: Option<FileTypeDetection>, // Store detection with cached image
}

#[derive(Clone, Debug)]
pub struct FileTypeDetection {
    pub detected_extension: String,
    pub current_extension: Option<String>,
    pub mismatch: bool,
    pub index: usize,    // which image this belongs to
    pub generation: u64, // which navigation round
}

// Preload task - receiver returns (index, result, generation)
pub struct PreloadTask {
    pub receiver: Receiver<(usize, Result<LoadedImage, String>, u64)>,
    pub index: usize,
    pub start_time: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Loading,
    LoadingFullGif,
    VirtualTextureLoading,
}

pub struct ViewerApp {
    pub texture: Option<egui::TextureHandle>,
    pub gif_animation: Option<GifAnimation>,
    pub receiver: Option<Receiver<Result<LoadedImage, String>>>,
    pub full_image_receiver: Option<Receiver<DynamicImage>>,
    pub full_gif_receiver: Option<Receiver<Result<LoadedImage, String>>>,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub current_directory: Option<PathBuf>,
    pub image_entries: Vec<ImageEntry>,
    pub current_index: usize,
    pub b_fit_to_window: bool,
    pub image_rect: Option<egui::Rect>,
    pub last_window_size: Option<egui::Vec2>,
    pub b_zoom_used: bool,
    pub b_ctrl_invert: bool,
    pub settings_manager: SettingsManager,
    pub input_bindings: InputBindings,
    pub logo_texture: Option<egui::TextureHandle>,
    pub show_delete_confirmation: bool,
    pub delete_key_was_pressed: bool,
    pub is_gif: bool,
    pub is_preview: bool,
    pub current_image_path: Option<PathBuf>,
    pub image_cache: LruCache<String, CachedImage>,
    pub max_cache_size: usize,
    pub preload_tasks: Vec<PreloadTask>,
    pub preloading_indices: std::collections::HashSet<usize>,
    pub preload_generation: u64,
    pub preload_workers: usize,
    pub cache_radius: usize,
    pub preload_origin: usize,
    pub delta_threshold: usize,
    pub should_stop_caching: bool,
    pub navigation_timer: Option<Instant>,
    pub navigation_pause_duration: Duration,
    pub cache_delta_factor: f32,
    pub max_cache_task: u8,
    pub last_preload_start: Option<Instant>,
    pub image_error: Option<String>,
    // Frame limiter fields
    pub last_frame_request_time: Instant,
    pub last_interaction_time: Instant,
    pub is_idle: bool,
    pub max_fps: f32,
    pub idle_fps_limit: f32,
    pub idle_timeout_ms: u64,
    pub unfocused_idle_timeout_ms: u64,
    pub unfocused_idle_fps_limit: f32,
    pub is_animating: bool,
    // Settings window
    pub show_settings_menu: bool,
    // Slideshow fields
    pub slideshow_enabled: bool,
    pub slideshow_interval: Duration,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    pub slideshow_last_advance: Instant,
    pub slideshow_has_advanced: bool,
    pub show_help_menu: bool,
    pub virtual_texture: Option<VirtualTexture>,
    pub virtual_texture_thread: Option<std::thread::JoinHandle<VirtualTexture>>,
    // Store progress separately so we can show it even when vt is moved
    pub vt_progress: Option<PreparationProgress>,
    pub vt_total_tiles: usize,
    pub file_type_detection: Option<FileTypeDetection>,
    pub startup_fullscreen_handled: bool,
    pub loading_state: LoadingState,
    pub dialog_open: bool,
    pub hamburger_menu_open: bool,
    pub overlay_visible: bool,
    pub current_fps: f32,  // Smoothed current FPS
    pub last_fps_update: std::time::Instant,
}

impl Default for ViewerApp {
    fn default() -> Self {
        let settings_manager = SettingsManager::new();
        let settings = settings_manager.get().clone();
        let radius = settings.cache_radius.max(1).min(100);
        let desired_capacity = radius * 2 + 2;
        let cache_capacity = desired_capacity.max(3);

        Self {
            texture: None,
            gif_animation: None,
            receiver: None,
            full_image_receiver: None,
            full_gif_receiver: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            current_directory: None,
            current_index: 0,
            b_fit_to_window: false,
            image_rect: None,
            last_window_size: None,
            b_zoom_used: false,
            b_ctrl_invert: settings.b_ctrl_invert,
            settings_manager: SettingsManager::new(),
            image_entries: Vec::new(),
            logo_texture: None,
            input_bindings: InputBindings::default(),
            show_delete_confirmation: false,
            delete_key_was_pressed: false,
            is_gif: false,
            is_preview: false,
            current_image_path: None,
            image_cache: LruCache::new(NonZeroUsize::new(cache_capacity).unwrap()),
            max_cache_size: cache_capacity,
            preload_tasks: Vec::new(),
            preloading_indices: std::collections::HashSet::new(),
            preload_generation: 0,
            preload_workers: 0,
            cache_radius: radius,
            preload_origin: 0,
            delta_threshold: ((radius as f32 * settings.cache_delta_factor).round() as usize)
                .max(1),
            should_stop_caching: false,
            navigation_timer: None,
            navigation_pause_duration: std::time::Duration::from_millis(
                settings_manager.get().navigation_pause_ms,
            ),
            cache_delta_factor: settings_manager.get().cache_delta_factor,
            max_cache_task: settings_manager.get().max_cache_task,
            last_preload_start: None,
            image_error: None,
            // Frame limiter defaults - will be overridden by load_frame_limiter_settings()
            last_frame_request_time: Instant::now(),
            last_interaction_time: Instant::now(),
            is_idle: false,
            max_fps: settings.max_fps,
            idle_fps_limit: settings.idle_fps_limit,
            idle_timeout_ms: settings.idle_timeout_ms,
            unfocused_idle_timeout_ms: settings.unfocused_idle_timeout_ms,
            unfocused_idle_fps_limit: settings.unfocused_idle_fps_limit,
            is_animating: false,
            // Settings window
            show_settings_menu: false,
            // Slideshow defaults
            slideshow_enabled: settings.slideshow_enabled,
            slideshow_interval: Duration::from_millis(settings.slideshow_interval_ms),
            slideshow_loop: settings.slideshow_loop,
            slideshow_random: settings.slideshow_random,
            slideshow_last_advance: Instant::now(),
            slideshow_has_advanced: false,
            show_help_menu: false,
            virtual_texture: None,
            virtual_texture_thread: None,
            vt_progress: None,
            vt_total_tiles: 0,
            file_type_detection: None,
            startup_fullscreen_handled: false,
            loading_state: Idle,
            dialog_open: false,
            hamburger_menu_open: false,
            overlay_visible: true,
            current_fps: 0.0,
            last_fps_update: std::time::Instant::now(),
        }
    }
}

impl ViewerApp {
    pub fn set_loading_state(&mut self, state: LoadingState) {
        self.loading_state = state;
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self.loading_state,
            LoadingState::Loading
                | LoadingState::LoadingFullGif
                | LoadingState::VirtualTextureLoading
        )
    }

    pub fn is_loading_virtual(&self) -> bool {
        matches!(self.loading_state, LoadingState::VirtualTextureLoading)
    }

    pub fn get_current_filename(&self) -> String {
        if let Some(entry) = self.image_entries.get(self.current_index) {
            match entry {
                ImageEntry::File(path) => path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                ImageEntry::Zip(zip) => zip.name.clone(),
                ImageEntry::S7z(s7z) => s7z.name.clone(),
                ImageEntry::Rar(rar) => rar.name.clone(),
            }
        } else {
            "JPEG Viewer".to_string()
        }
    }

    pub fn window_title(&self) -> String {
        let filename = self.get_current_filename();
        let total = self.image_entries.len();
        let slideshow_indicator = if self.slideshow_enabled { " ▶" } else { "" };

        if total > 0 {
            format!(
                "{}{} ({}/{}) - JPEG Viewer",
                filename,
                slideshow_indicator,
                self.current_index + 1,
                total
            )
        } else {
            "JPEG Viewer".to_string()
        }
    }
    pub fn update_window_title(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    pub fn get_texture_options(&self) -> egui::TextureOptions {
        let settings = self.settings_manager.get();
        match settings.texture_filter.as_str() {
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

    // Slideshow methods
    pub fn toggle_slideshow(&mut self) {
        self.slideshow_enabled = !self.slideshow_enabled;
        if self.slideshow_enabled {
            self.slideshow_last_advance = Instant::now();
            self.slideshow_has_advanced = false;
        }
        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_enabled = self.slideshow_enabled;
        });
        self.update_window_title(&eframe::egui::Context::default());
    }

    pub fn slideshow_speed_up(&mut self) {
        let new_interval = self.slideshow_interval.as_millis().max(500) as u64 / 2;
        self.slideshow_interval = Duration::from_millis(new_interval.max(500));
        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval.max(500);
        });
    }

    pub fn slideshow_speed_down(&mut self) {
        let new_interval = self.slideshow_interval.as_millis() as u64 * 2;
        self.slideshow_interval = Duration::from_millis(new_interval.min(60000));
        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval.min(60000);
        });
    }

    pub fn load_slideshow_settings(&mut self) {
        let settings = self.settings_manager.get();
        self.slideshow_enabled = settings.slideshow_enabled;
        self.slideshow_interval = Duration::from_millis(settings.slideshow_interval_ms);
        self.slideshow_loop = settings.slideshow_loop;
        self.slideshow_random = settings.slideshow_random;
    }

    // Extreme aspect ratio detection (too thin or thick)
    // === Ease of reading the Bee Movie Script with Jpeg_viewer! ===
    pub fn _has_extreme_aspect_ratio(&self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }

        // Calculate ratio (smaller / larger)
        let ratio = if width > height {
            height as f32 / width as f32
        } else {
            width as f32 / height as f32
        };

        // If ratio is less than 0.1 (very tall or very wide)
        ratio < 0.1
    }

    /// Spawn a background task to load the full GIF (all frames)
    pub fn spawn_full_gif_loading(&mut self) {
        println!(
            "[spawn_full_gif_loading] called: is_gif={}, is_preview={}, has_receiver={}, is_loading={}",
            self.is_gif,
            self.is_preview,
            self.full_gif_receiver.is_some(),
            self.is_loading(),
        );

        if !self.is_gif || !self.is_preview {
            println!("[spawn_full_gif_loading] Not a GIF preview, skipping");
            return;
        }

        if self.full_gif_receiver.is_some() || self.is_loading() {
            println!("[spawn_full_gif_loading] skipping - conditions not met");
            return;
        }

        if let Some(entry) = self.image_entries.get(self.current_index).cloned() {
            use rayon::spawn;
            use std::sync::mpsc::channel;

            let (tx, rx) = channel();
            self.full_gif_receiver = Some(rx);
            self.set_loading_state(LoadingState::LoadingFullGif);

            println!(
                "[spawn_full_gif_loading] spawning task for index {}",
                self.current_index
            );

            spawn(move || {
                println!(
                    "[spawn_full_gif_loading] task started, about to call load_full_gif_from_entry"
                );
                let result = std::panic::catch_unwind(|| {
                    crate::gif::loader::load_full_gif_from_entry(entry)
                        .map(|gif| LoadedImage::Animated(gif, false))
                });
                match result {
                    Ok(Ok(loaded_image)) => {
                        println!("[spawn_full_gif_loading] load_full_gif_from_entry succeeded");
                        let _ = tx.send(Ok(loaded_image));
                    }
                    Ok(Err(e)) => {
                        eprintln!(
                            "[spawn_full_gif_loading] load_full_gif_from_entry error: {}",
                            e
                        );
                        let _ = tx.send(Err(e));
                    }
                    Err(panic) => {
                        eprintln!(
                            "[spawn_full_gif_loading] PANIC in full GIF loader: {:?}",
                            panic
                        );
                        let _ = tx.send(Err("Panic in full GIF loader".to_string()));
                    }
                }
                println!("[spawn_full_gif_loading] task finished");
            });
        } else {
            println!(
                "[spawn_full_gif_loading] no entry found for index {}",
                self.current_index
            );
        }
    }
}
