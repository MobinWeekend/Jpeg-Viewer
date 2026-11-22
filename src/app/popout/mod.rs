//! Floating image windows associated with Image workspace tabs.
//!
//! Pop-outs are independent native egui viewports. Their state and rendering
//! are kept here instead of mixing auxiliary-window behavior into workspace tabs.

mod dialogs;
mod helpers;
mod hover_controls;
mod image_render;
mod jvp;
mod manager;
mod platform;
mod sidebar;
#[cfg(test)]
mod tests;
mod viewports;
mod window_animation;
mod window_geometry;
mod window_render;

use helpers::*;
use platform::*;

use crate::decoder::gif::animation::GifAnimation;
use crate::imaging::entry::ImageEntry;
use jvp::{LoadedJvp, PopoutPreset, SaveImageChoice};
use eframe::egui;
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_SIZE: egui::Vec2 = egui::Vec2::new(560.0, 420.0);
const MIN_SIZE: egui::Vec2 = egui::Vec2::new(120.0, 90.0);
const MAX_SIZE: egui::Vec2 = egui::Vec2::new(2400.0, 1800.0);
const DVD_SPEED: egui::Vec2 = egui::Vec2::new(190.0, 145.0);
const RESIZE_EDGE: f32 = 9.0;
const RESIZE_CORNER: f32 = 34.0;

#[derive(Default)]
pub(crate) struct PopoutManager {
    // Deferred viewports render independently of the root window. This is
    // essential on egui 0.33: immediate child viewports can be discarded when
    // the parent becomes hidden/minimized because the parent stops submitting
    // them. Each child owns a synchronized state object for its independent UI.
    windows: HashMap<u64, Arc<Mutex<PopoutWindow>>>,
    pending_close: Vec<egui::ViewportId>,
    /// A deferred viewport ID is single-use after its native window is closed.
    /// Each reopening gets a fresh generation so egui/winit can create a new
    /// native window for the same Image tab.
    next_viewport_generation: u64,
    /// Per-Image-tab options survive closing the native pop-out.
    presets: HashMap<u64, PopoutPreset>,
    save_dialog: Option<SaveDialogState>,
    load_dialog: Option<LoadDialogState>,
    pending_image_open: Option<(ImageEntry, PopoutPreset)>,
    /// Image tab whose right-hand Pop-out settings sidebar is currently open.
    settings_sidebar_tab: Option<u64>,
    status_message: Option<String>,
}

struct SaveDialogState {
    tab_id: u64,
    preset: PopoutPreset,
    image_entry: Option<ImageEntry>,
    choice: SaveImageChoice,
}

struct LoadDialogState {
    tab_id: u64,
    document: LoadedJvp,
}

struct PopoutWindow {
    viewport_id: egui::ViewportId,
    viewport_generation: u64,
    texture: egui::TextureHandle,
    animation: Option<GifAnimation>,
    last_frame_index: Option<usize>,
    size: egui::Vec2,
    base_size: egui::Vec2,
    aspect_ratio: f32,
    size_scale: f32,
    /// Mirrored live size slider value.
    pending_size_scale: f32,
    /// Last requested native size. Viewport size notifications can lag the
    /// command by a few frames; do not let an old notification undo a resize.
    expected_native_size: Option<(egui::Vec2, Instant, u8)>,
    /// Last requested native position. Native position notifications can lag
    /// while a resize keeps the opposite edge anchored; do not overwrite the
    /// in-progress geometry with stale viewport data.
    expected_native_position: Option<(egui::Pos2, Instant)>,
    /// Initial size/position are supplied to ViewportBuilder exactly once.
    /// Later geometry updates travel through the single ViewportCommand path.
    viewport_initialized: bool,
    /// Original decoded RGBA pixels for static images, allowing alpha to be
    /// adjusted without multiplying RGB and accidentally darkening the image.
    source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
    display_texture: egui::TextureHandle,
    rendered_opacity: u8,
    opacity_percent: f32,
    /// Eased opacity actually used by the image texture and painter.
    displayed_opacity_percent: f32,
    click_through: bool,
    pass_near_enabled: bool,
    pass_near_distance: f32,
    pass_near_opacity: f32,
    pass_hover_enabled: bool,
    pass_hover_opacity: f32,
    /// Screen-space pointer position forwarded by the main viewport while
    /// click-through is enabled (the pop-out itself intentionally receives no input).
    pointer_screen_pos: Option<egui::Pos2>,
    show_bounds: bool,
    dvd_mode: bool,
    /// True while the user temporarily moves a DVD-mode pop-out with native
    /// window dragging. Physics pauses, but the mode resumes on release.
    dvd_dragging: bool,
    dvd_circle_collision: bool,
    random_reflect_on_bounce: bool,
    random_rotate_on_bounce: bool,
    rotation_radians: f32,
    dvd_rotation_target: f32,
    dvd_rotation_ease: f32,
    dvd_speed: f32,
    ball_mode: bool,
    ball_gravity: f32,
    ball_bounciness: f32,
    /// Contact friction coefficient. It is applied only when the ball hits a
    /// boundary, coupling tangential linear velocity with angular velocity.
    ball_friction: f32,
    /// Fixed-step simulation remainder (seconds) for stable Ball mode physics.
    ball_time_accumulator: f32,
    ball_velocity: egui::Vec2,
    ball_rotate: bool,
    /// Live angular velocity in radians per second.
    ball_angular_velocity: f32,
    ball_dragging: bool,
    /// Whether a secondary-button throw gesture is currently sampling pointer
    /// motion. Unlike native left-drag, this keeps the viewport stationary.
    ball_throw_dragging: bool,
    /// Smoothed pointer velocity captured during a right-button throw gesture.
    ball_throw_velocity: egui::Vec2,
    random_bounce_enabled: bool,
    random_bounce_interval: f32,
    next_random_bounce: Instant,
    /// Inward offsets from the monitor edges in left, top, right, bottom order.
    dvd_edge_offsets: [f32; 4],
    monitor_id: Option<String>,
    stay_on_top: bool,
    stay_on_bottom: bool,
    /// Window level is sent only when it changes. Reapplying it every frame can
    /// continuously raise the child above the main application on some WMs.
    applied_window_level: Option<egui::WindowLevel>,
    position: egui::Pos2,
    dvd_velocity: egui::Vec2,
    last_dvd_tick: Instant,
    last_bottom_enforcement: Instant,
    resize_gesture: Option<ResizeGesture>,
    /// Set by the deferred child callback when the OS close button is used.
    /// The manager observes it on the next root pass and removes the tab link.
    close_requested: bool,
    /// On Windows, detach the native top-level HWND from the main window's
    /// owner relationship. Owned windows are automatically hidden when their
    /// owner is minimized, which contradicts the pop-out lifecycle contract.
    native_owner_detached: bool,
    /// DWM per-pixel alpha compositing has been enabled for this native window.
    #[cfg(target_os = "windows")]
    dwm_transparency_applied: bool,
}

#[derive(Clone, Copy)]
struct ResizeGesture {
    start_size: egui::Vec2,
    start_position: egui::Pos2,
    horizontal: f32,
    vertical: f32,
    /// Last cumulative pointer delta. Usually supplied by egui directly; the
    /// accumulated fallback keeps resizing stable if the pointer leaves a
    /// viewport and the integration briefly cannot provide a total delta.
    accumulated_delta: egui::Vec2,
}

