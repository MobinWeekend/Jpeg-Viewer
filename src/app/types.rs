use crate::gif_animation::GifAnimation;
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
}

// Cached image data - add Clone derive
#[derive(Clone)]
pub struct CachedImage {
    pub texture: egui::TextureHandle,
    pub is_gif: bool,
    pub is_preview: bool,
}

// Preload task
pub struct PreloadTask {
    pub index: usize,
    pub receiver: Receiver<Result<LoadedImage, String>>,
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
    pub b_is_loading: bool,
    pub b_is_loading_full: bool,
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
    pub cache_radius: usize,
    pub preload_origin: usize,
    pub delta_threshold: usize,
    pub should_stop_caching: bool,
    pub navigation_timer: Option<Instant>,
    pub navigation_pause_duration: Duration,
    pub cache_delta_factor: f32,
    pub max_cache_task: u8,
    pub last_preload_start: Option<Instant>,
    pub processed_this_frame: usize,
    pub image_error: Option<String>,
    // Frame limiter fields
    pub last_repaint_time: Instant,
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
}

impl Default for ViewerApp {
    fn default() -> Self {
        let settings_manager = SettingsManager::new();
        let settings = settings_manager.get().clone();
        let radius = settings.cache_radius.max(1).min(100);

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
            b_is_loading: false,
            b_is_loading_full: false,
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
            image_cache: LruCache::new(NonZeroUsize::new((radius * 2 + 1).max(3)).unwrap()),
            max_cache_size: radius * 2 + 1,
            preload_tasks: Vec::new(),
            preloading_indices: std::collections::HashSet::new(),
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
            processed_this_frame: 0,
            image_error: None,
            // Frame limiter defaults - will be overridden by load_frame_limiter_settings()
            last_repaint_time: Instant::now(),
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
        }
    }
}

impl ViewerApp {
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

    pub fn update_window_title(&self, ctx: &egui::Context) {
        let filename = self.get_current_filename();
        let total = self.image_entries.len();
        let slideshow_indicator = if self.slideshow_enabled { " ▶" } else { "" };
        let title = if total > 0 {
            format!(
                "{}{} ({}/{}) - JPEG Viewer",
                filename,
                slideshow_indicator,
                self.current_index + 1,
                total
            )
        } else {
            "JPEG Viewer".to_string()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    // Because JV crashed on Bee Movie Script Jpeg!
    pub fn has_extreme_aspect_ratio(&self, width: u32, height: u32) -> bool {
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

    pub fn advance_slideshow(&mut self) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        use rand::Rng;
        let new_index = if self.slideshow_random {
            let mut rng = rand::thread_rng();

            let mut idx;
            loop {
                idx = rng.gen_range(0..len);

                if idx != self.current_index || len <= 1 {
                    break;
                }
            }

            idx
        } else {
            (self.current_index + 1) % len
        };

        self.current_index = new_index;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.b_is_loading_full = false;
        self.image_error = None;
        self.receiver = None;

        self.load_current_image_with_cache();
        self.update_window_title(&eframe::egui::Context::default());
    }

    pub fn load_slideshow_settings(&mut self) {
        let settings = self.settings_manager.get();
        self.slideshow_enabled = settings.slideshow_enabled;
        self.slideshow_interval = Duration::from_millis(settings.slideshow_interval_ms);
        self.slideshow_loop = settings.slideshow_loop;
        self.slideshow_random = settings.slideshow_random;
    }
}
