use super::editing::EditOutputFormat;
use super::image_metadata::FileMetadata;
use super::texture_mipmap::SharedMipmapState;
use super::virtual_texture::{PreparationProgress, VirtualTexture};
use crate::decoder::gif::animation::GifAnimation;
use crate::imaging::core::DecodedImage;
use crate::imaging::core::ImageFormat;
use crate::imaging::entry::ImageEntry;
use crate::imaging::sort::SortMethod;
use crate::input::shortcuts::InputBindings;
use crate::settings::SettingsManager;
use eframe::egui;
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ====== LOADED IMAGE ======

#[derive(Clone)]
pub enum LoadedImage {
    Static(DecodedImage),
    Animated(GifAnimation, bool),
    VirtualPending(Vec<u8>, u32, u32),
}

/// Image result produced by the current-image/preload worker. Metadata and
/// format detection are extracted from the same source bytes as decoding.
pub(crate) struct LoadedEntryData {
    pub image: LoadedImage,
    pub metadata: FileMetadata,
    pub file_type_detection: Option<FileTypeDetection>,
    /// Compressed source retained only for a bounded GIF preview-to-full upgrade.
    pub full_gif_source: Option<Arc<Vec<u8>>>,
}

/// Result of a background thumbnail job. When original dimensions are known,
/// preview pixels are normalized to the source image's aspect ratio before they
/// enter the shared texture cache, so every consumer (Masonry, Browse, Queue,
/// Favorites, and New Tab) can use the texture dimensions for contain rendering.
/// `source_aspect_ratio` remains separate because Masonry's tile layout must
/// stay stable even when a texture is evicted or replaced.
#[derive(Clone)]
pub(crate) struct GeneratedThumbnail {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    /// Effective fitted resolution before source-ratio correction expands the
    /// long side. Used for cache selection so synthetic pixels do not count as
    /// a higher-resolution source preview.
    pub resolution: u32,
    pub source_aspect_ratio: Option<f32>,
    /// Original EXIF-oriented dimensions discovered by the metadata pool.
    /// Shared with Masonry and later sidebar requests to avoid repeat probes.
    pub source_dimensions: Option<(u32, u32)>,
}

// ====== CACHE ======

#[derive(Clone)]
pub struct CachedImage {
    pub texture: egui::TextureHandle,
    /// Texture filtering is part of the GPU resource configuration. A cached
    /// texture created with another filter must be rebuilt before it is shown.
    pub texture_filter: String,
    pub(crate) mipmap_state: Option<SharedMipmapState>,
    pub is_gif: bool,
    pub is_preview: bool,
    pub index: usize,
    pub file_type_detection: Option<FileTypeDetection>,
    /// Detailed metadata obtained from the same bytes used to decode this entry.
    pub file_metadata: Option<FileMetadata>,
}

// ====== FILE TYPE DETECTION ======

#[derive(Clone, Debug)]
pub struct FileTypeDetection {
    pub detected_format: ImageFormat,
    pub current_extension: Option<String>,
    pub mismatch: bool,
    pub index: usize,
    pub generation: u64,
}

// ====== PRELOAD ======

pub struct PreloadTask {
    pub receiver: Receiver<(usize, Result<LoadedEntryData, String>, u64)>,
    pub index: usize,
    pub generation: u64,
    pub start_time: Instant,
    /// Cooperative cancellation for obsolete speculative work.
    pub cancellation: Arc<AtomicBool>,
}

impl Drop for PreloadTask {
    fn drop(&mut self) {
        // Dropping/clearing a preload task is itself a cancellation request.
        // Workers check this flag between bounded reads and before publishing.
        self.cancellation
            .store(true, std::sync::atomic::Ordering::Release);
    }
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
    /// Stable entry identity for `texture`. During navigation the previous
    /// texture may remain visible while a new entry loads, so `current_index`
    /// alone cannot safely identify the texture being displayed or cached.
    pub(crate) texture_image_id: Option<String>,
    pub gif_animation: Option<GifAnimation>,
    pub receiver: Option<Receiver<Result<LoadedEntryData, String>>>,
    /// One retained CPU-side copy for the currently displayed image. It lets
    /// Inspect reuse decoded pixels instead of reopening the source file.
    pub(crate) current_decoded_pixels: Option<(String, u32, u32, Arc<Vec<u8>>)>,
    pub full_image_receiver: Option<Receiver<DecodedImage>>,
    pub full_gif_receiver: Option<Receiver<Result<LoadedImage, String>>>,
    /// Reuses the original compressed GIF bytes while upgrading a preview to
    /// the full animation. Bounded by the loader to avoid pinning huge sources.
    pub(crate) full_gif_source: Option<Arc<Vec<u8>>>,

    pub current_directory: Option<PathBuf>,
    pub image_entries: Vec<ImageEntry>,
    pub current_index: usize,
    pub current_image_path: Option<PathBuf>,

    pub is_gif: bool,
    pub is_preview: bool,
    /// True when the user explicitly requested full RAW sensor development.
    pub showing_raw_developed: bool,
    pub image_error: Option<String>,

    // ====== IMAGE VIEW ======
    pub zoom: f32,
    /// (start zoom, target zoom, animation start, zoom-in direction).
    pub zoom_animation: Option<(f32, f32, Instant, bool)>,
    pub pan: egui::Vec2,
    pub b_fit_to_window: bool,
    pub b_zoom_used: bool,
    pub image_rect: Option<egui::Rect>,
    pub last_window_size: Option<egui::Vec2>,
    /// Pending automatic or user-requested window fit, performed once image dimensions are ready.
    pub window_fit_pending: bool,
    pub zoom_drag_start_pos: Option<egui::Pos2>,
    pub zoom_drag_start_zoom: f32,
    pub zoom_drag_start_pan: egui::Vec2,
    pub zoom_drag_start_center: egui::Pos2,
    /// True while a primary-button drag that began on the image is panning it.
    pub main_view_pan_drag_active: bool,
    /// True while a primary-button drag that began outside the image moves the window.
    pub main_view_window_drag_active: bool,
    /// True while a global Alt/Shift mouse shortcut has handed control to the OS.
    pub(crate) window_mouse_operation_active: bool,

    // ====== CACHE ======
    pub image_cache: LruCache<String, CachedImage>,
    /// At most one preloaded neighbour retains decoded CPU pixels for mipmap
    /// generation. The buffer is consumed when that neighbour's mip job starts.
    pub(crate) preloaded_mipmap_pixels: Option<(String, u32, u32, Arc<Vec<u8>>)>,
    /// Progressive mip uploads share a single GPU upload allowance per frame.
    pub(crate) mipmap_upload_budget: Arc<AtomicUsize>,
    /// Mipmap preparation associated with the currently displayed texture.
    pub(crate) current_mipmap_state: Option<SharedMipmapState>,
    pub max_cache_size: usize,

    // ====== PRELOAD ======
    pub preload_tasks: Vec<PreloadTask>,
    pub preloading_indices: HashSet<usize>,
    /// GIF detection is performed once per stable entry identity and shared by
    /// preload scheduling/progress checks rather than reopening entries per frame.
    pub(crate) gif_detection_cache: Arc<Mutex<HashMap<String, bool>>>,
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
    pub power_inhibitor: crate::platform::power::PowerInhibitor,
    pub slideshow_interval: Duration,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    pub slideshow_transition: String,
    pub slideshow_motion: String,
    pub slideshow_motion_strength: f32,
    pub slideshow_last_advance: Instant,
    pub slideshow_has_advanced: bool,
    pub slideshow_settings_loaded: bool,
    /// Outgoing image retained while the next slideshow image transitions in.
    pub slideshow_previous_texture: Option<egui::TextureHandle>,
    /// Last rendered destination rectangle for the outgoing image, preserving its zoom and pan.
    pub slideshow_previous_rect: Option<egui::Rect>,
    /// Starts when the incoming image is ready, not when navigation begins.
    pub slideshow_transition_started: Option<Instant>,
    /// Pending Dial-up reveal after a navigation event; waits until the new texture is ready.
    pub win98_dialup_reveal_pending: bool,
    /// Previous texture ID lets the async loader distinguish the incoming image from the retained image.
    pub win98_dialup_previous_texture_id: Option<egui::TextureId>,
    /// Time when the incoming image started its simulated row-by-row reveal.
    pub win98_dialup_reveal_started: Option<Instant>,

    // ====== VIRTUAL TEXTURE ======
    pub virtual_texture: Option<VirtualTexture>,
    pub virtual_texture_thread: Option<std::thread::JoinHandle<VirtualTexture>>,
    pub vt_progress: Option<PreparationProgress>,
    pub vt_total_tiles: usize,

    // ====== FILE DETECTION ======
    pub file_type_detection: Option<FileTypeDetection>,

    // ====== SETTINGS ======
    pub settings_manager: SettingsManager,
    /// Independent Viewer, Image, Masonry, and Compare workspace tabs.
    pub(crate) workspace: super::workspace::WorkspaceManager,
    pub input_bindings: InputBindings,

    // ====== UI ======
    pub logo_texture: Option<egui::TextureHandle>,

    pub show_delete_confirmation: bool,
    pub delete_key_was_pressed: bool,

    pub show_settings_menu: bool,
    pub show_help_menu: bool,

    pub dialog_open: bool,
    pub hamburger_menu_open: bool,
    /// Screen-space rectangle of the hamburger button from the most recent UI frame.
    pub hamburger_button_rect: Option<egui::Rect>,
    pub overlay_visible: bool,

    pub compactui: bool,

    // ====== WINDOW ======
    pub startup_fullscreen_handled: bool,
    pub startup_window_placement_handled: bool,
    /// Keep the saved maximized state until the native window manager confirms restoration.
    pub startup_restore_maximize_pending: bool,
    /// Keep reset window-geometry defaults from being overwritten by the
    /// current session's native viewport when it closes. Cleared on next launch.
    pub preserve_reset_window_geometry: bool,
    pub sidebar_width_needs_save: bool,
    /// Actual width allocated to the left sidebar this frame, used to position overlays.
    pub sidebar_actual_width: f32,
    /// Actual width allocated to the active right-side panel (Settings or Edit) this frame.
    pub right_sidebar_actual_width: f32,
    /// Screen-space rectangle allocated to egui's CentralPanel after docked sidebars.
    /// Overlay toolbars and fullscreen workspace tabs use this instead of estimating
    /// width by subtracting preferred sidebar sizes from the outer window.
    pub central_panel_rect: egui::Rect,
    /// True only when the user began dragging the sidebar resize handle.
    pub sidebar_resize_dragging: bool,
    pub window_always_on_top: bool,

    // ====== FPS ======
    pub current_fps: f32,
    pub last_fps_update: Instant,

    // ====== INPUT ======
    pub b_ctrl_invert: bool,

    // ====== LOADING ======
    pub loading_state: LoadingState,
    pub indexing_receiver: Option<Receiver<(Vec<PathBuf>, Option<PathBuf>)>>,
    /// Optional IPC server used to receive file-open requests from later launches.
    pub instance_server: Option<crate::platform::instance::InstanceServer>,
    /// File paths collected from startup/IPC launches before opening as one selection.
    pub pending_open_paths: Vec<PathBuf>,
    /// Quiet-period deadline used to group Explorer multi-select launches.
    pub pending_open_deadline: Option<Instant>,
    /// Whether the first image in the pending external-open batch was already opened eagerly.
    pub pending_open_first_opened: bool,
    /// Recently opened external selection, allowing slightly staggered Explorer
    /// per-file activations to be merged even if one request arrives after the
    /// initial quiet period has elapsed.
    pub recent_external_open_batch: Option<(Vec<PathBuf>, Instant)>,

    // ====== Sort method ======
    pub sort_method: SortMethod,

    //meta data
    pub file_metadata_open: bool,
    pub file_metadata: Option<FileMetadata>,
    // ====== TRICKS ASSETS ======
    /// Currently displayed blurred backdrop; retained while the next one is prepared.
    pub tricks_backdrop_texture: Option<egui::TextureHandle>,
    /// Newly prepared backdrop, cross-faded over the currently displayed texture.
    pub tricks_backdrop_next_texture: Option<egui::TextureHandle>,
    pub tricks_backdrop_transition_started: Option<Instant>,
    pub tricks_backdrop_key: Option<(PathBuf, u8)>,
    pub tricks_backdrop_receiver:
        Option<Receiver<(PathBuf, u8, Result<(usize, usize, Vec<u8>), String>)>>,
    /// Small (at most 256x256) preview derived from the already-decoded current image.
    pub tricks_backdrop_source_preview: Option<(PathBuf, usize, usize, Vec<u8>)>,
    pub tricks_backdrop_source_requested_path: Option<PathBuf>,
    pub tricks_backdrop_source_receiver:
        Option<Receiver<(PathBuf, Result<(usize, usize, Vec<u8>), String>)>>,
    pub tricks_watermark_texture: Option<egui::TextureHandle>,
    pub tricks_watermark_requested_path: Option<PathBuf>,
    pub tricks_watermark_receiver:
        Option<Receiver<(PathBuf, Result<(usize, usize, Vec<u8>), String>)>>,
    pub tricks_watermark_gif: Option<GifAnimation>,
    pub tricks_watermark_gif_frame: Option<usize>,
    pub tricks_watermark_gif_receiver: Option<Receiver<(PathBuf, Result<GifAnimation, String>)>>,

    // ====== WORKING-COPY EDITING ======
    pub(crate) edit_session: Option<super::editing::EditSession>,
    /// `show_edit_menu` is the toggleable editing mode/panel visibility.
    /// The session survives when the user returns to normal viewing mode.
    pub show_edit_menu: bool,
    pub edit_status: Option<String>,
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
    pub resize_width: u32,
    pub resize_height: u32,
    pub resize_linked: bool,
    pub crop_tool_active: bool,
    pub crop_drag_start_image: Option<egui::Pos2>,
    pub crop_selection: Option<[u32; 4]>,
    pub adjustment_gesture_active: bool,
    pub edit_output_format: EditOutputFormat,
    pub last_export_format: EditOutputFormat,
    pub save_quality: u8,
    // Inspect-channel view is a non-destructive display-only overlay.
    pub inspect_mode: bool,
    pub inspect_channels: [bool; 4],
    pub inspect_colorized: bool,
    pub inspect_texture: Option<egui::TextureHandle>,
    pub(crate) inspect_mipmap_state: Option<SharedMipmapState>,
    pub inspect_source: Option<image::RgbaImage>,
    pub inspect_source_path: Option<PathBuf>,
    pub confirm_overwrite_path: Option<PathBuf>,
    pub overwrite_is_save_as: bool,
    pub confirm_discard_edits: bool,

    // ====== QUEUE / SELECTION WORKFLOWS ======
    pub queue_paths: Vec<PathBuf>,
    /// Persisted favorites, stored beside settings.ini in favorites.jvq.
    pub favorite_paths: Vec<PathBuf>,
    pub favorites_loaded: bool,
    pub sidebar_favorites_tab: bool,
    pub sidebar_info_tab: bool,
    /// Active quick-position editor: (collection: 0=browse, 1=queue, 2=favorites, index, text).
    pub sidebar_number_edit: Option<(u8, usize, String)>,
    /// Rename dialog for a filesystem image; the extension is preserved.
    pub rename_dialog: Option<(PathBuf, String)>,
    pub queue_mode: bool,
    pub queue_repeat: bool,
    pub queue_root: Option<PathBuf>,
    pub queue_status: Option<String>,
    pub queue_previous_entries: Option<Vec<ImageEntry>>,
    pub queue_previous_index: usize,
    /// Which content tab is active in the left sidebar (false = browse, true = queue).
    pub sidebar_queue_tab: bool,
    /// Last current image index handled by sidebar auto-scroll.
    pub sidebar_last_current_index: Option<usize>,
    /// Last rendered queue drop target; external drops here are queued rather than opened.
    pub queue_drop_rect: Option<egui::Rect>,
    /// Imported queue waiting for the user to choose Add or Replace.
    pub queue_import_prompt: Option<(Vec<PathBuf>, Option<PathBuf>)>,
    /// Browse-sidebar drag source index, used to reorder image entries.
    pub sidebar_drag_index: Option<usize>,
    pub sidebar_drag_collection: u8,
    /// LRU cache shared by sidebar and Masonry thumbnails. Entries use their
    /// returned physical resolution; collection enforces both a 256-entry cap
    /// and a 96 MiB approximate texture budget for high-DPI galleries.
    pub sidebar_thumbnails: LruCache<String, egui::TextureHandle>,
    pub sidebar_thumbnail_attempted: HashSet<String>,
    pub sidebar_thumbnail_failed: HashSet<String>,
    pub sidebar_thumbnail_pending: HashSet<String>,
    /// Disk-cache writes associated only with thumbnails requested by Home.
    pub(crate) home_thumbnail_cache_writes: HashMap<String, PathBuf>,
    /// Persistent cache file paths resolved for Home cards in this session.
    pub(crate) home_thumbnail_cache_paths: HashMap<String, PathBuf>,
    /// Home card/source-size pairs whose persistent cache was already checked.
    pub(crate) home_thumbnail_cache_checked: HashSet<String>,
    /// Cache reads and Home preview-source discovery currently in flight.
    pub(crate) home_thumbnail_cache_pending: HashSet<String>,
    pub(crate) home_preview_receivers: Vec<(
        Option<PathBuf>,
        Option<String>,
        Receiver<super::home_thumbnail_cache::HomePreviewTaskResult>,
    )>,
    pub sidebar_thumbnail_receivers: Vec<(
        String,
        Option<String>,
        bool,
        Arc<AtomicU8>,
        Instant,
        Arc<Mutex<Option<Instant>>>,
        Arc<AtomicBool>,
        Receiver<(String, Result<GeneratedThumbnail, String>)>,
    )>,
    /// Files currently visible in the active sidebar collection. Masonry has
    /// its own independent visibility set and cancellation policy.
    pub sidebar_visible_sources: HashSet<String>,
    /// One-row directional lookahead for sidebar thumbnails.
    pub sidebar_prefetch_sources: HashSet<String>,
    /// Previous Browse-sidebar scroll offset, used to prefetch in the actual
    /// scroll direction rather than reacting to wheel input elsewhere.
    pub sidebar_last_scroll_offset: Option<f32>,
    /// Source keys currently intersecting the active Masonry viewport. Completed
    /// Masonry-only results outside this set are discarded instead of cached.
    pub masonry_visible_sources: HashSet<String>,
    /// Shared source-dimension cache, populated by Masonry and completed
    /// sidebar thumbnail pipelines. The attempted set also remembers failures.
    pub masonry_dimension_attempted: HashSet<String>,
    pub masonry_dimension_pending: HashSet<String>,
    /// Shared, EXIF-oriented source dimensions keyed by stable source identity.
    pub masonry_dimension_results: HashMap<String, Option<(u32, u32)>>,
    /// Best-known fallback aspect ratios from thumbnail providers. These are
    /// applied lazily by visible Masonry cards instead of scanning every tab
    /// whenever one thumbnail finishes. Exact source dimensions take priority.
    pub masonry_aspect_ratio_results: HashMap<String, f32>,
    pub masonry_dimension_receivers: Vec<(
        String,
        bool,
        Instant,
        Arc<Mutex<Option<Instant>>>,
        Arc<AtomicBool>,
        Receiver<Option<(u32, u32)>>,
    )>,
}

impl Default for ViewerApp {
    fn default() -> Self {
        Self::new(SettingsManager::new())
    }
}

impl ViewerApp {
    /// Construct the application from the settings already loaded by the native
    /// bootstrap. This avoids reading/migrating settings.ini twice on startup.
    pub(crate) fn new(settings_manager: SettingsManager) -> Self {
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
            texture_image_id: None,
            gif_animation: None,
            receiver: None,
            current_decoded_pixels: None,
            full_image_receiver: None,
            full_gif_receiver: None,
            full_gif_source: None,

            current_directory: None,
            image_entries: Vec::new(),
            current_index: 0,
            current_image_path: None,

            is_gif: false,
            is_preview: false,
            showing_raw_developed: false,
            image_error: None,

            // ====== IMAGE VIEW ======
            zoom: 1.0,
            zoom_animation: None,
            pan: egui::Vec2::ZERO,
            b_fit_to_window: false,
            b_zoom_used: false,
            image_rect: None,
            last_window_size: None,
            window_fit_pending: false,
            zoom_drag_start_pos: None,
            zoom_drag_start_zoom: 1.0,
            zoom_drag_start_pan: egui::Vec2::ZERO,
            zoom_drag_start_center: egui::Pos2::ZERO,
            main_view_pan_drag_active: false,
            main_view_window_drag_active: false,
            window_mouse_operation_active: false,

            // ====== CACHE ======
            image_cache: LruCache::new(
                NonZeroUsize::new(cache_capacity)
                    .expect("cache capacity must be greater than zero"),
            ),
            preloaded_mipmap_pixels: None,
            mipmap_upload_budget: Arc::new(AtomicUsize::new(
                super::texture_mipmap::MAX_MIP_UPLOAD_BYTES_PER_FRAME,
            )),
            current_mipmap_state: None,
            max_cache_size: cache_capacity,

            // ====== PRELOAD ======
            preload_tasks: Vec::new(),
            preloading_indices: HashSet::new(),
            gif_detection_cache: Arc::new(Mutex::new(HashMap::new())),
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
            slideshow_enabled: false,
            power_inhibitor: crate::platform::power::PowerInhibitor::default(),
            slideshow_interval: Duration::from_millis(settings.slideshow_interval_ms),
            slideshow_loop: settings.slideshow_loop,
            slideshow_random: settings.slideshow_random,
            slideshow_transition: settings.slideshow_transition.clone(),
            slideshow_motion: settings.slideshow_motion.clone(),
            slideshow_motion_strength: settings.slideshow_motion_strength,

            slideshow_last_advance: now,
            slideshow_has_advanced: false,
            slideshow_settings_loaded: false,
            slideshow_previous_texture: None,
            slideshow_previous_rect: None,
            slideshow_transition_started: None,
            win98_dialup_reveal_pending: false,
            win98_dialup_previous_texture_id: None,
            win98_dialup_reveal_started: None,

            // ====== VIRTUAL TEXTURE ======
            virtual_texture: None,
            virtual_texture_thread: None,
            vt_progress: None,
            vt_total_tiles: 0,

            // ====== FILE DETECTION ======
            file_type_detection: None,

            // ====== SETTINGS ======
            settings_manager,
            workspace: super::workspace::WorkspaceManager::default(),
            input_bindings: InputBindings::default(),

            // ====== UI ======
            logo_texture: None,

            show_delete_confirmation: false,
            delete_key_was_pressed: false,

            show_settings_menu: false,
            show_help_menu: false,

            dialog_open: false,
            hamburger_menu_open: false,
            hamburger_button_rect: None,
            overlay_visible: true,

            compactui: settings.compact_mode,

            // ====== WINDOW ======
            startup_fullscreen_handled: false,
            startup_window_placement_handled: false,
            startup_restore_maximize_pending: settings.window_maximized
                && !settings.start_fullscreen,
            preserve_reset_window_geometry: false,
            sidebar_width_needs_save: false,
            sidebar_actual_width: 0.0,
            right_sidebar_actual_width: 0.0,
            central_panel_rect: egui::Rect::NOTHING,
            sidebar_resize_dragging: false,
            window_always_on_top: false,

            // ====== FPS ======
            current_fps: 0.0,
            last_fps_update: now,

            // ====== INPUT ======
            b_ctrl_invert: settings.b_ctrl_invert,

            // ====== LOADING ======
            loading_state: LoadingState::Idle,
            indexing_receiver: None,
            instance_server: None,
            pending_open_paths: Vec::new(),
            pending_open_deadline: None,
            pending_open_first_opened: false,
            recent_external_open_batch: None,

            // ====== Sort method ======
            sort_method: SortMethod::Natural,

            //metadata
            file_metadata_open: false,
            file_metadata: None,
            tricks_backdrop_texture: None,
            tricks_backdrop_next_texture: None,
            tricks_backdrop_transition_started: None,
            tricks_backdrop_key: None,
            tricks_backdrop_receiver: None,
            tricks_backdrop_source_preview: None,
            tricks_backdrop_source_requested_path: None,
            tricks_backdrop_source_receiver: None,
            tricks_watermark_texture: None,
            tricks_watermark_requested_path: None,
            tricks_watermark_receiver: None,
            tricks_watermark_gif: None,
            tricks_watermark_gif_frame: None,
            tricks_watermark_gif_receiver: None,

            edit_session: None,
            show_edit_menu: false,
            edit_status: None,
            crop_x: 0,
            crop_y: 0,
            crop_width: 0,
            crop_height: 0,
            resize_width: 0,
            resize_height: 0,
            resize_linked: true,
            crop_tool_active: false,
            crop_drag_start_image: None,
            crop_selection: None,
            adjustment_gesture_active: false,
            edit_output_format: EditOutputFormat::Png,
            last_export_format: EditOutputFormat::Png,
            save_quality: 90,
            inspect_mode: false,
            inspect_channels: [true, true, true, true],
            inspect_colorized: false,
            inspect_texture: None,
            inspect_mipmap_state: None,
            inspect_source: None,
            inspect_source_path: None,
            confirm_overwrite_path: None,
            overwrite_is_save_as: false,
            confirm_discard_edits: false,
            queue_paths: Vec::new(),
            favorite_paths: Vec::new(),
            favorites_loaded: false,
            sidebar_favorites_tab: false,
            sidebar_info_tab: false,
            sidebar_number_edit: None,
            rename_dialog: None,
            queue_mode: false,
            queue_repeat: true,
            queue_root: None,
            queue_status: None,
            queue_previous_entries: None,
            queue_previous_index: 0,
            sidebar_queue_tab: false,
            sidebar_last_current_index: None,
            queue_drop_rect: None,
            queue_import_prompt: None,
            sidebar_drag_index: None,
            sidebar_drag_collection: 0,
            sidebar_thumbnails: LruCache::new(
                NonZeroUsize::new(256).expect("non-zero thumbnail cache capacity"),
            ),
            sidebar_thumbnail_attempted: HashSet::new(),
            sidebar_thumbnail_failed: HashSet::new(),
            sidebar_thumbnail_pending: HashSet::new(),
            home_thumbnail_cache_writes: HashMap::new(),
            home_thumbnail_cache_paths: HashMap::new(),
            home_thumbnail_cache_checked: HashSet::new(),
            home_thumbnail_cache_pending: HashSet::new(),
            home_preview_receivers: Vec::new(),
            sidebar_thumbnail_receivers: Vec::new(),
            sidebar_visible_sources: HashSet::new(),
            sidebar_prefetch_sources: HashSet::new(),
            sidebar_last_scroll_offset: None,
            masonry_visible_sources: HashSet::new(),
            masonry_dimension_attempted: HashSet::new(),
            masonry_dimension_pending: HashSet::new(),
            masonry_dimension_results: HashMap::new(),
            masonry_aspect_ratio_results: HashMap::new(),
            masonry_dimension_receivers: Vec::new(),
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

    /// Start the Win98 Dial-up animation once a navigation's incoming texture is installed.
    /// `force` is used for cache hits, where the requested image is already known to be ready.
    pub fn mark_win98_dialup_texture_ready(&mut self, force: bool) {
        if !self.win98_dialup_reveal_pending {
            return;
        }
        let current_id = self.texture.as_ref().map(egui::TextureHandle::id);
        if let Some(current_id) = current_id
            && (force || Some(current_id) != self.win98_dialup_previous_texture_id)
        {
            self.win98_dialup_reveal_started = Some(Instant::now());
            self.win98_dialup_reveal_pending = false;
        }
    }

    pub(crate) fn is_home_surface_active(&self) -> bool {
        self.workspace.home_page_visible
            && self
                .workspace
                .tabs
                .get(self.workspace.active_index)
                .is_some_and(|tab| {
                    tab.id == 1 && tab.kind == super::workspace::WorkspaceTabKind::Viewer
                })
    }

    pub fn window_title(&self) -> String {
        if self.is_home_surface_active() {
            return "Home — JPEG Viewer".to_owned();
        }
        match self.workspace.active_kind() {
            super::workspace::WorkspaceTabKind::Masonry
            | super::workspace::WorkspaceTabKind::Compare
            | super::workspace::WorkspaceTabKind::Welcome => {
                let workspace_name = self.workspace.active_tab_title().unwrap_or("Workspace");
                return format!("{workspace_name} — JPEG Viewer");
            }
            super::workspace::WorkspaceTabKind::Viewer
            | super::workspace::WorkspaceTabKind::Image => {}
        }

        let filename = self.get_current_filename();
        let total = self.image_entries.len();
        let slideshow_indicator = if self.slideshow_enabled { " ▶" } else { "" };
        if total == 0 {
            return "JPEG Viewer".to_string();
        }
        format!(
            "({}/{}) {}{}",
            self.current_index + 1,
            total,
            slideshow_indicator,
            filename,
        )
    }

    pub fn update_window_title(&self, ctx: &egui::Context) {
        const LAST_TITLE_ID: &str = "jpeg_viewer_last_viewport_title";
        let title = self.window_title();
        let unchanged = ctx.data(|data| {
            data.get_temp::<String>(egui::Id::new(LAST_TITLE_ID))
                .is_some_and(|last_title| last_title == title)
        });
        if unchanged {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        ctx.data_mut(|data| data.insert_temp(egui::Id::new(LAST_TITLE_ID), title));
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

    /// Texture options for the base upload. Mipmap levels are added later by
    /// the progressive Glow callback, never by egui's synchronous generate_mipmap.
    pub fn get_texture_base_options(&self) -> egui::TextureOptions {
        let mut options = self.get_texture_options();
        options.mipmap_mode = None;
        options
    }

    // ====== SLIDESHOW ======

    pub fn toggle_slideshow(&mut self) {
        self.slideshow_enabled = !self.slideshow_enabled;

        if self.slideshow_enabled {
            self.slideshow_last_advance = Instant::now();
            self.slideshow_has_advanced = false;
        }
    }

    pub fn toggle_compactui(&mut self) {
        self.compactui = !self.compactui;

        let _ = self.settings_manager.update(|settings| {
            settings.compact_mode = self.compactui;
        });
    }

    pub fn slideshow_speed_up(&mut self) {
        let current = self.slideshow_interval.as_millis() as u64;

        let new_interval = (current.max(500) / 2).max(500);

        self.slideshow_interval = Duration::from_millis(new_interval);
        self.slideshow_last_advance = Instant::now();

        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval;
        });
    }

    pub fn slideshow_speed_down(&mut self) {
        let current = self.slideshow_interval.as_millis() as u64;

        let new_interval = current.saturating_mul(2).min(60_000);

        self.slideshow_interval = Duration::from_millis(new_interval);
        self.slideshow_last_advance = Instant::now();

        let _ = self.settings_manager.update(|settings| {
            settings.slideshow_interval_ms = new_interval;
        });
    }

    pub fn load_slideshow_settings(&mut self) {
        let settings = self.settings_manager.get();

        self.slideshow_enabled = false;

        self.slideshow_interval = Duration::from_millis(settings.slideshow_interval_ms);

        self.slideshow_loop = settings.slideshow_loop;
        self.slideshow_random = settings.slideshow_random;
        self.slideshow_transition = settings.slideshow_transition.clone();
        self.slideshow_motion = settings.slideshow_motion.clone();
        self.slideshow_motion_strength = settings.slideshow_motion_strength;
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

        // Keep the compressed source only while a bounded preview upgrade is
        // active. This avoids a second file read or archive extraction for GIFs
        // whose source was small enough to retain during the preview load.
        let retained_source = self.full_gif_source.clone();

        // ====== BACKGROUND TASK ======

        rayon::spawn(move || {
            println!(
                "[spawn_full_gif_loading] \
                 task started, about to call load_full_gif_from_entry"
            );

            let result = std::panic::catch_unwind(|| {
                if let Some(bytes) = retained_source {
                    crate::decoder::gif::animation::GifAnimation::from_bytes(bytes.as_slice())
                        .map(|gif| LoadedImage::Animated(gif, false))
                } else {
                    crate::decoder::gif::loader::load_full_gif_from_entry(entry)
                        .map(|gif| LoadedImage::Animated(gif, false))
                }
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
