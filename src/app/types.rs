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
    pub receiver: Receiver<LoadedImage>,
}

pub struct ViewerApp {
    pub texture: Option<egui::TextureHandle>,
    pub gif_animation: Option<GifAnimation>,
    pub receiver: Option<Receiver<LoadedImage>>,
    pub full_image_receiver: Option<Receiver<DynamicImage>>,
    pub full_gif_receiver: Option<Receiver<LoadedImage>>,
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
    pub navigation_timer: Option<std::time::Instant>,
    pub navigation_pause_duration: std::time::Duration,
    pub cache_delta_factor: f32,
    pub max_cache_task: u8,
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
        let title = if total > 0 {
            format!(
                "{} ({}/{}) - JPEG Viewer",
                filename,
                self.current_index + 1,
                total
            )
        } else {
            "JPEG Viewer".to_string()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    //Because JV crashed on Bee Movie Script Jpeg!
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
                mipmap_mode: None, // Mipmaps not needed for nearest
                ..Default::default()
            },
            "mipmap" => egui::TextureOptions {
                magnification: egui::TextureFilter::Linear,
                minification: egui::TextureFilter::Linear,
                // Enable mipmaps with linear filtering between levels
                mipmap_mode: Some(egui::TextureFilter::Linear),
                ..Default::default()
            },
            _ => egui::TextureOptions::LINEAR, // Default fallback
        }
    }
}
