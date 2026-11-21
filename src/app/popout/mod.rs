//! Floating image windows associated with Image workspace tabs.
//!
//! Pop-outs are independent native egui viewports. Their state and rendering
//! are kept here instead of mixing auxiliary-window behavior into workspace tabs.

use crate::decoder::gif::animation::GifAnimation;
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_SIZE: egui::Vec2 = egui::Vec2::new(560.0, 420.0);
const MIN_SIZE: egui::Vec2 = egui::Vec2::new(120.0, 90.0);
const MAX_SIZE: egui::Vec2 = egui::Vec2::new(2400.0, 1800.0);
const DVD_SPEED: egui::Vec2 = egui::Vec2::new(190.0, 145.0);
const RESIZE_EDGE: f32 = 9.0;
const RESIZE_CORNER: f32 = 34.0;
pub(crate) const IMAGE_TAB_PANEL_HEIGHT: f32 = 64.0;

#[derive(Default)]
pub(crate) struct PopoutManager {
    // Deferred viewports render independently of the root window. This is
    // essential on egui 0.33: immediate child viewports can be discarded when
    // the parent becomes hidden/minimized because the parent stops submitting
    // them. Each child owns a synchronized state object for its independent UI.
    windows: HashMap<u64, Arc<Mutex<PopoutWindow>>>,
    pending_close: Vec<u64>,
}

struct PopoutWindow {
    texture: egui::TextureHandle,
    animation: Option<GifAnimation>,
    last_frame_index: Option<usize>,
    size: egui::Vec2,
    base_size: egui::Vec2,
    aspect_ratio: f32,
    size_scale: f32,
    /// Slider edits are staged here and applied only when the user releases it.
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
    click_through: bool,
    dvd_mode: bool,
    dvd_speed: f32,
    /// Inward offsets from the monitor edges in left, top, right, bottom order.
    dvd_edge_offsets: [f32; 4],
    monitor_id: Option<String>,
    stay_on_top: bool,
    stay_on_bottom: bool,
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

impl PopoutWindow {
    fn viewport_id(tab_id: u64) -> egui::ViewportId {
        egui::ViewportId::from_hash_of(("jpeg_viewer_popout", tab_id))
    }

    fn minimum_size(&self) -> egui::Vec2 {
        // Keep a feasible minimum even for extremely panoramic/portrait sources:
        // aspect ratio is more important than imposing independent min width/height.
        let max_width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        let width = MIN_SIZE.x.min(max_width);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn maximum_size(&self) -> egui::Vec2 {
        let width = MAX_SIZE.x.min(MAX_SIZE.y * self.aspect_ratio);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn size_for_width(&self, width: f32) -> egui::Vec2 {
        let min = self.minimum_size();
        let max = self.maximum_size();
        let width = width.clamp(min.x, max.x);
        egui::vec2(width, width / self.aspect_ratio)
    }

    fn size_for_scale(&self, scale: f32) -> egui::Vec2 {
        self.size_for_width(self.base_size.x * scale)
    }

    fn viewport_builder(&mut self, tab_id: u64) -> egui::ViewportBuilder {
        let level = if self.stay_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else if self.stay_on_bottom {
            egui::WindowLevel::AlwaysOnBottom
        } else {
            egui::WindowLevel::Normal
        };
        let mut builder = egui::ViewportBuilder::default()
            .with_title(format!("Pop-out · JPEG Viewer · Tab {tab_id}"))
            .with_min_inner_size(self.minimum_size())
            .with_max_inner_size(self.maximum_size())
            .with_clamp_size_to_monitor_size(true)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_decorations(false)
            // Resizing is deliberately owned by the eight egui edge/corner
            // handles below. Enabling native resizing as well lets the OS take
            // pointer capture at the same edges, competing with aspect-ratio
            // geometry commands and causing flicker or missed drags.
            .with_resizable(false)
            .with_window_level(level)
            .with_mouse_passthrough(self.click_through);
        if !self.viewport_initialized {
            // ViewportBuilder is accumulated every frame. Re-supplying inner
            // size and position while gestures also issue native geometry
            // commands makes the backend repeatedly reconcile two owners of
            // the same geometry, producing resize flicker and stale-size resets.
            builder = builder
                .with_inner_size(self.size)
                .with_position(self.position)
                .with_active(true);
            self.viewport_initialized = true;
        }
        builder
    }

    fn update_animation_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
        let Some(animation) = self.animation.as_mut() else {
            self.update_static_opacity_texture(ctx, tab_id);
            return;
        };
        let _ = animation.get_current_frame();
        let frame_index = animation.get_current_frame_index();
        let opacity = opacity_to_alpha(self.opacity_percent);
        if self.last_frame_index != Some(frame_index) || self.rendered_opacity != opacity {
            let Some(frame) = animation.get_current_frame_ref() else {
                return;
            };
            let size = [frame.width() as usize, frame.height() as usize];
            let rgba = frame.as_raw();
            let source = egui::ColorImage::from_rgba_unmultiplied(size, rgba);
            self.texture = ctx.load_texture(
                format!("jpeg_viewer_popout_source_{tab_id}_{frame_index}"),
                source,
                Default::default(),
            );
            if opacity == u8::MAX {
                self.display_texture = self.texture.clone();
            } else {
                let adjusted = apply_opacity_to_rgba(rgba, opacity);
                self.display_texture = ctx.load_texture(
                    format!("jpeg_viewer_popout_display_{tab_id}_{frame_index}_{opacity}"),
                    egui::ColorImage::from_rgba_unmultiplied(size, &adjusted),
                    Default::default(),
                );
            }
            self.last_frame_index = Some(frame_index);
            self.rendered_opacity = opacity;
        }
        if animation.is_animated() {
            let delay = animation
                .delays
                .get(animation.current_frame)
                .copied()
                .unwrap_or(Duration::from_millis(100));
            let adjusted = if animation.speed_multiplier > 0.0 {
                Duration::from_secs_f64(
                    delay.as_secs_f64() / animation.speed_multiplier.clamp(0.1, 10.0) as f64,
                )
            } else {
                delay
            };
            ctx.request_repaint_after(adjusted.max(Duration::from_millis(8)));
        }
    }

    fn update_static_opacity_texture(&mut self, ctx: &egui::Context, tab_id: u64) {
        let opacity = opacity_to_alpha(self.opacity_percent);
        if self.rendered_opacity == opacity {
            return;
        }
        if opacity == u8::MAX {
            self.display_texture = self.texture.clone();
            self.rendered_opacity = opacity;
            return;
        }
        if let Some((width, height, pixels)) = &self.source_pixels {
            let adjusted = apply_opacity_to_rgba(pixels, opacity);
            self.display_texture = ctx.load_texture(
                format!("jpeg_viewer_popout_display_{tab_id}_{opacity}"),
                egui::ColorImage::from_rgba_unmultiplied([*width, *height], &adjusted),
                Default::default(),
            );
            self.rendered_opacity = opacity;
        }
    }

    fn update_dvd_position(&mut self, ctx: &egui::Context) {
        if !self.dvd_mode {
            self.last_dvd_tick = Instant::now();
            return;
        }
        let monitors = crate::platform::monitors::available_monitors();
        let monitor = self
            .monitor_id
            .as_deref()
            .and_then(|id| monitors.iter().find(|monitor| monitor.id == id))
            .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
            .or_else(|| monitors.first());
        let Some(monitor) = monitor else {
            return;
        };

        let now = Instant::now();
        let delta = now
            .saturating_duration_since(self.last_dvd_tick)
            .as_secs_f32()
            .clamp(0.0, 0.05);
        self.last_dvd_tick = now;
        let monitor_bounds = monitor.bounds();
        // Offsets are applied to the four monitor edges, not to the window's
        // corners. Keep a small feasible rectangle even if the user enters
        // offsets larger than the selected monitor can accommodate.
        let max_horizontal_inset = (monitor_bounds.width() - 1.0).max(0.0) * 0.5;
        let max_vertical_inset = (monitor_bounds.height() - 1.0).max(0.0) * 0.5;
        let left =
            monitor_bounds.left() + self.dvd_edge_offsets[0].clamp(0.0, max_horizontal_inset);
        let top = monitor_bounds.top() + self.dvd_edge_offsets[1].clamp(0.0, max_vertical_inset);
        let right =
            monitor_bounds.right() - self.dvd_edge_offsets[2].clamp(0.0, max_horizontal_inset);
        let bottom =
            monitor_bounds.bottom() - self.dvd_edge_offsets[3].clamp(0.0, max_vertical_inset);
        let bounds = egui::Rect::from_min_max(
            egui::pos2(left, top),
            egui::pos2(right.max(left), bottom.max(top)),
        );
        let monitor_size = bounds.size();
        let max_width = monitor_size.x.min(monitor_size.y * self.aspect_ratio);
        let fitted_size = self.size_for_width(self.size.x.min(max_width));
        if (fitted_size - self.size).length_sq() > 0.1 {
            self.request_inner_size(ctx, fitted_size);
        }
        let max_x = (bounds.right() - self.size.x).max(bounds.left());
        let max_y = (bounds.bottom() - self.size.y).max(bounds.top());
        // `self.position` is synchronized from the native viewport while DVD
        // mode is off, and is also where the settings panel places the window
        // when DVD mode or a different monitor is selected.
        let mut position = self.position;
        position.x += DVD_SPEED.x * self.dvd_speed * delta * self.dvd_velocity.x.signum();
        position.y += DVD_SPEED.y * self.dvd_speed * delta * self.dvd_velocity.y.signum();

        if position.x < bounds.left() {
            position.x = bounds.left();
            self.dvd_velocity.x = self.dvd_velocity.x.abs();
        } else if position.x > max_x {
            position.x = max_x;
            self.dvd_velocity.x = -self.dvd_velocity.x.abs();
        }
        if position.y < bounds.top() {
            position.y = bounds.top();
            self.dvd_velocity.y = self.dvd_velocity.y.abs();
        } else if position.y > max_y {
            position.y = max_y;
            self.dvd_velocity.y = -self.dvd_velocity.y.abs();
        }
        self.request_outer_position(ctx, position);
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn request_inner_size(&mut self, ctx: &egui::Context, size: egui::Vec2) {
        let size = self.size_for_width(size.x);
        let size_changed = (self.size - size).length_sq() > 0.25;
        self.size = size;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        if size_changed {
            self.expected_native_size = Some((size, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.request_repaint();
        }
    }

    fn request_geometry(&mut self, ctx: &egui::Context, size: egui::Vec2, position: egui::Pos2) {
        let size = self.size_for_width(size.x);
        let size_changed = (self.size - size).length_sq() > 0.25;
        let position_changed = (self.position - position).length_sq() > 0.25;
        self.size = size;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        self.position = position;

        if size_changed {
            self.expected_native_size = Some((size, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.request_repaint();
        }
        if position_changed {
            self.expected_native_position = Some((position, Instant::now()));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
            ctx.request_repaint();
        }
    }

    fn request_inner_size_for_viewport(
        &mut self,
        ctx: &egui::Context,
        tab_id: u64,
        size: egui::Vec2,
    ) {
        let size = self.size_for_width(size.x);
        let changed = (self.size - size).length_sq() > 0.25;
        self.size = size;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        if changed {
            self.expected_native_size = Some((size, Instant::now(), 0));
            ctx.send_viewport_cmd_to(
                Self::viewport_id(tab_id),
                egui::ViewportCommand::InnerSize(size),
            );
            ctx.request_repaint();
        }
    }

    fn request_outer_position(&mut self, ctx: &egui::Context, position: egui::Pos2) {
        if (self.position - position).length_sq() <= 0.25 {
            return;
        }
        self.position = position;
        self.expected_native_position = Some((position, Instant::now()));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
        ctx.request_repaint();
    }

    fn synchronize_native_position(&mut self, ctx: &egui::Context) {
        if self.resize_gesture.is_some() || self.dvd_mode {
            return;
        }
        let Some(actual_position) =
            ctx.input(|input| input.viewport().outer_rect.map(|rect| rect.min))
        else {
            return;
        };
        if !actual_position.x.is_finite() || !actual_position.y.is_finite() {
            return;
        }
        if let Some((expected, requested_at)) = self.expected_native_position {
            let acknowledged = (actual_position - expected).length_sq() <= 4.0;
            if acknowledged {
                self.expected_native_position = None;
            } else if requested_at.elapsed() < Duration::from_millis(1200) {
                return;
            } else {
                self.expected_native_position = None;
            }
        }
        self.position = actual_position;
    }

    fn synchronize_native_size(&mut self, ctx: &egui::Context) {
        if self.resize_gesture.is_some() {
            return;
        }
        let Some(actual_size) =
            ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()))
        else {
            return;
        };
        if !actual_size.x.is_finite()
            || !actual_size.y.is_finite()
            || actual_size.x <= 0.0
            || actual_size.y <= 0.0
        {
            return;
        }
        if let Some((expected, requested_at, retry_count)) = self.expected_native_size {
            let acknowledged = (actual_size - expected).length_sq() <= 4.0;
            if acknowledged {
                self.expected_native_size = None;
            } else if requested_at.elapsed() < Duration::from_millis(650) {
                // Native viewport information may still describe the previous
                // frame. Keep our requested size until winit reports the result.
                return;
            } else if retry_count < 2 {
                // A child viewport may miss a geometry command while it is being
                // recreated or while a window manager is processing a move/resize.
                // Retry the last requested size instead of reverting to stale data.
                self.expected_native_size = Some((expected, Instant::now(), retry_count + 1));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(expected));
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            } else {
                // After bounded retries, accept the native size. This also lets
                // the OS enforce a monitor/work-area constraint without an
                // endless resize loop.
                self.expected_native_size = None;
            }
        }
        let width_delta = ((actual_size.x - self.size.x) / self.size.x.max(1.0)).abs();
        let height_delta = ((actual_size.y - self.size.y) / self.size.y.max(1.0)).abs();
        if width_delta < 0.01 && height_delta < 0.01 {
            return;
        }

        // Native resize notifications may change only one dimension. Pick the
        // axis that moved most, then derive the other dimension from the source
        // aspect ratio. This also keeps the slider in sync with edge/corner drags.
        let target_width = if width_delta >= height_delta {
            actual_size.x
        } else {
            actual_size.y * self.aspect_ratio
        };
        let constrained = self.size_for_width(target_width);
        self.size = constrained;
        self.size_scale = self.size.x / self.base_size.x.max(1.0);
        self.pending_size_scale = self.size_scale;
        if (constrained - actual_size).length_sq() > 1.0 {
            self.expected_native_size = Some((constrained, Instant::now(), 0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(constrained));
            ctx.request_repaint();
        }
    }

    fn render(&mut self, ui: &mut egui::Ui, tab_id: u64) -> bool {
        let ctx = ui.ctx().clone();
        self.synchronize_native_size(&ctx);
        self.synchronize_native_position(&ctx);
        self.update_dvd_position(&ctx);
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        let viewport_rect = ui.max_rect();
        self.update_animation_texture(&ctx, tab_id);
        let texture = self.display_texture.clone();

        ui.painter().rect_filled(
            viewport_rect,
            egui::CornerRadius::ZERO,
            egui::Color32::TRANSPARENT,
        );
        // The native window is sized to the source aspect ratio. Painting into
        // the full client rect therefore wraps the image without letterboxing.
        // When decoded pixels are available, opacity has already been applied
        // to alpha alone. Keep the tint white so RGB channels are never darkened.
        // Fallback texture paths still use egui's premultiplied tint handling.
        let fallback_tint = if self.source_pixels.is_none() && self.animation.is_none() {
            egui::Color32::from_white_alpha(opacity_to_alpha(self.opacity_percent))
        } else {
            egui::Color32::WHITE
        };
        ui.painter().image(
            texture.id(),
            viewport_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            fallback_tint,
        );

        if !self.click_through {
            // Partition the client area into explicit move regions. The previous
            // full-window drag widget overlapped all resize handles; although the
            // code tried to suppress StartDrag by checking the pointer afterward,
            // egui could still capture the pointer for that full-surface widget.
            // Non-overlapping regions make move, resize, and toolbar gestures
            // mutually exclusive before input ownership is assigned.
            let corner = RESIZE_CORNER
                .min(viewport_rect.width() * 0.25)
                .min(viewport_rect.height() * 0.25);
            // On extremely panoramic/portrait images the aspect-preserving
            // window can be shorter than the normal 9-point edge target. Scale
            // edge depth down to the corner size so horizontal/vertical handles
            // still meet cleanly instead of overlapping one another.
            let edge = RESIZE_EDGE.min(corner);
            let left = viewport_rect.left();
            let right = viewport_rect.right();
            let top = viewport_rect.top();
            let bottom = viewport_rect.bottom();
            let center_left = left + corner;
            let center_right = right - corner;
            let center_top = top + corner;
            let center_bottom = bottom - corner;
            let controls_fit = viewport_rect.height() >= 48.0;
            let move_regions = if controls_fit {
                let bar = Self::control_bar_rect(viewport_rect);
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center_above_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, bar.top()),
                        ),
                    ),
                    (
                        "center_left_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.top()),
                            egui::pos2(bar.left(), bar.bottom()),
                        ),
                    ),
                    (
                        "center_right_of_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(bar.right(), bar.top()),
                            egui::pos2(center_right, bar.bottom()),
                        ),
                    ),
                    (
                        "center_below_controls",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, bar.bottom()),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            } else {
                // Very short aspect-ratio windows cannot physically fit the
                // 30-point hover toolbar. Keep the whole non-resize interior
                // draggable and leave window actions available in the tab panel.
                vec![
                    (
                        "top_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, top + edge),
                            egui::pos2(center_right, center_top),
                        ),
                    ),
                    (
                        "bottom_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_bottom),
                            egui::pos2(center_right, bottom - edge),
                        ),
                    ),
                    (
                        "left_band",
                        egui::Rect::from_min_max(
                            egui::pos2(left + edge, center_top),
                            egui::pos2(center_left, center_bottom),
                        ),
                    ),
                    (
                        "right_band",
                        egui::Rect::from_min_max(
                            egui::pos2(center_right, center_top),
                            egui::pos2(right - edge, center_bottom),
                        ),
                    ),
                    (
                        "center",
                        egui::Rect::from_min_max(
                            egui::pos2(center_left, center_top),
                            egui::pos2(center_right, center_bottom),
                        ),
                    ),
                ]
            };
            for (name, rect) in move_regions {
                if !rect.is_positive() {
                    continue;
                }
                let drag = ui.interact(
                    rect,
                    egui::Id::new(("jpeg_viewer_popout_move", tab_id, name)),
                    egui::Sense::drag(),
                );
                if drag.drag_started() {
                    self.dvd_mode = false;
                    // A native move supersedes any queued position request from a
                    // prior resize/monitor change. Let the window manager own it.
                    self.expected_native_position = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            }

            let pointer_hovering_window = ctx
                .pointer_hover_pos()
                .is_some_and(|position| viewport_rect.contains(position));
            // Keep resize hit zones registered while pointer is inside or a drag
            // is active. The hover toolbar appears only while pointer is inside.
            self.render_hover_controls(ui, viewport_rect, tab_id, pointer_hovering_window);
        }
        if close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        close_requested
    }

    fn control_bar_rect(viewport_rect: egui::Rect) -> egui::Rect {
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        // Keep the controls entirely inside the central area between the
        // enlarged corner-resize hotspots, so toolbar clicks never overlap a
        // move region on compact pop-out windows.
        let width = (viewport_rect.width() - corner * 2.0 - 8.0).clamp(54.0, 119.0);
        egui::Rect::from_center_size(
            egui::pos2(
                viewport_rect.center().x,
                viewport_rect.top() + corner + 16.0,
            ),
            egui::vec2(width, 30.0),
        )
    }

    fn render_hover_controls(
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        tab_id: u64,
        show_buttons: bool,
    ) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter().clone();
        if show_buttons && viewport_rect.height() >= 48.0 {
            let gap = 4.0;
            let bar_rect = Self::control_bar_rect(viewport_rect);
            let bar_width = bar_rect.width();
            let button_width = ((bar_width - 16.0 - gap * 2.0) / 3.0).max(10.0);
            let button_size = egui::vec2(button_width, 24.0);
            painter.rect_filled(
                bar_rect,
                egui::CornerRadius::same(15),
                egui::Color32::from_black_alpha(165),
            );
            let first = bar_rect.left() + (bar_width - (button_size.x * 3.0 + gap * 2.0)) * 0.5;
            let top_rect =
                egui::Rect::from_min_size(egui::pos2(first, bar_rect.top() + 3.0), button_size);
            let bottom_rect = egui::Rect::from_min_size(
                egui::pos2(first + button_size.x + gap, bar_rect.top() + 3.0),
                button_size,
            );
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(first + (button_size.x + gap) * 2.0, bar_rect.top() + 3.0),
                button_size,
            );
            if Self::hover_button(
                ui,
                tab_id,
                top_rect,
                if self.stay_on_top { "●" } else { "↑" },
                "Stay on top",
            ) {
                self.stay_on_top = !self.stay_on_top;
                if self.stay_on_top {
                    self.stay_on_bottom = false;
                }
            }
            if Self::hover_button(
                ui,
                tab_id,
                bottom_rect,
                if self.stay_on_bottom { "●" } else { "↓" },
                "Stay on bottom",
            ) {
                self.stay_on_bottom = !self.stay_on_bottom;
                if self.stay_on_bottom {
                    self.stay_on_top = false;
                }
            }
            if Self::hover_button(ui, tab_id, close_rect, "×", "Close pop-out") {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Full four-edge resize strips and larger diagonal corner hit zones.
        // The top strip remains usable because the floating toolbar is inset
        // well below it; corner handles win over the generic move surface.
        let corner = RESIZE_CORNER
            .min(viewport_rect.width() * 0.25)
            .min(viewport_rect.height() * 0.25);
        let edge = RESIZE_EDGE.min(corner);
        let left = viewport_rect.left();
        let right = viewport_rect.right();
        let top = viewport_rect.top();
        let bottom = viewport_rect.bottom();
        let zones = [
            (
                "north",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, top),
                    egui::pos2(right - corner, top + edge),
                ),
                0.0,
                -1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "south",
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, bottom - edge),
                    egui::pos2(right - corner, bottom),
                ),
                0.0,
                1.0,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                "west",
                egui::Rect::from_min_max(
                    egui::pos2(left, top + corner),
                    egui::pos2(left + edge, bottom - corner),
                ),
                -1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "east",
                egui::Rect::from_min_max(
                    egui::pos2(right - edge, top + corner),
                    egui::pos2(right, bottom - corner),
                ),
                1.0,
                0.0,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                "north_west",
                egui::Rect::from_min_size(viewport_rect.min, egui::vec2(corner, corner)),
                -1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthWest,
            ),
            (
                "north_east",
                egui::Rect::from_min_size(
                    egui::pos2(right - corner, top),
                    egui::vec2(corner, corner),
                ),
                1.0,
                -1.0,
                egui::CursorIcon::ResizeNorthEast,
            ),
            (
                "south_west",
                egui::Rect::from_min_size(
                    egui::pos2(left, bottom - corner),
                    egui::vec2(corner, corner),
                ),
                -1.0,
                1.0,
                egui::CursorIcon::ResizeSouthWest,
            ),
            (
                "south_east",
                egui::Rect::from_min_size(
                    viewport_rect.right_bottom() - egui::vec2(corner, corner),
                    egui::vec2(corner, corner),
                ),
                1.0,
                1.0,
                egui::CursorIcon::ResizeSouthEast,
            ),
        ];
        for (name, rect, horizontal, vertical, cursor) in zones {
            let response = ui.interact(
                rect,
                egui::Id::new(("jpeg_viewer_popout_resize", tab_id, name)),
                egui::Sense::drag(),
            );
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started() {
                self.resize_gesture = Some(ResizeGesture {
                    start_size: self.size,
                    start_position: self.position,
                    horizontal,
                    vertical,
                    accumulated_delta: egui::Vec2::ZERO,
                });
                self.dvd_mode = false;
            }
            if response.dragged() {
                if let Some(gesture) = self.resize_gesture {
                    // Resize is based on total movement from the original press.
                    // drag_delta() is only the movement since the previous frame;
                    // adding it to start_size causes a reset/flicker every frame.
                    let mut gesture = gesture;
                    if let Some(total_delta) = response.total_drag_delta() {
                        gesture.accumulated_delta = total_delta;
                    } else {
                        gesture.accumulated_delta += response.drag_delta();
                    }
                    self.resize_gesture = Some(gesture);
                    let delta = gesture.accumulated_delta;
                    let horizontal_scale = if gesture.horizontal != 0.0 {
                        (gesture.start_size.x + delta.x * gesture.horizontal)
                            / gesture.start_size.x.max(1.0)
                    } else {
                        1.0
                    };
                    let vertical_scale = if gesture.vertical != 0.0 {
                        (gesture.start_size.y + delta.y * gesture.vertical)
                            / gesture.start_size.y.max(1.0)
                    } else {
                        1.0
                    };
                    let scale = match (gesture.horizontal != 0.0, gesture.vertical != 0.0) {
                        (true, true) => {
                            if (horizontal_scale - 1.0).abs() >= (vertical_scale - 1.0).abs() {
                                horizontal_scale
                            } else {
                                vertical_scale
                            }
                        }
                        (true, false) => horizontal_scale,
                        (false, true) => vertical_scale,
                        (false, false) => 1.0,
                    };
                    let new_size = self.size_for_width(gesture.start_size.x * scale);
                    let mut new_position = gesture.start_position;
                    if gesture.horizontal < 0.0 {
                        new_position.x += gesture.start_size.x - new_size.x;
                    } else if gesture.horizontal == 0.0 {
                        new_position.x += (gesture.start_size.x - new_size.x) * 0.5;
                    }
                    if gesture.vertical < 0.0 {
                        new_position.y += gesture.start_size.y - new_size.y;
                    } else if gesture.vertical == 0.0 {
                        new_position.y += (gesture.start_size.y - new_size.y) * 0.5;
                    }
                    self.request_geometry(&ctx, new_size, new_position);
                }
            }
            if response.drag_stopped() {
                self.resize_gesture = None;
            }
        }
    }

    fn hover_button(
        ui: &mut egui::Ui,
        tab_id: u64,
        rect: egui::Rect,
        label: &str,
        tooltip: &str,
    ) -> bool {
        let response = ui.interact(
            rect,
            egui::Id::new(("jpeg_viewer_popout_button", tab_id, label)),
            egui::Sense::click(),
        );
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(6),
            if response.hovered() && label == "×" {
                egui::Color32::from_rgb(175, 55, 60)
            } else if response.hovered() {
                egui::Color32::from_black_alpha(220)
            } else {
                egui::Color32::from_black_alpha(155)
            },
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        response.on_hover_text(tooltip).clicked()
    }
}

fn opacity_to_alpha(opacity_percent: f32) -> u8 {
    (opacity_percent.clamp(0.0, 100.0) * 2.55).round() as u8
}

fn apply_opacity_to_rgba(rgba: &[u8], opacity: u8) -> Vec<u8> {
    let mut adjusted = rgba.to_vec();
    for pixel in adjusted.chunks_exact_mut(4) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity) + 127) / 255) as u8;
    }
    adjusted
}

fn fit_aspect_size(aspect_ratio: f32, bounds: egui::Vec2) -> egui::Vec2 {
    let aspect_ratio = aspect_ratio.clamp(0.03, 32.0);
    let bounds = bounds.max(egui::vec2(1.0, 1.0));
    let fitted_width = if bounds.x / bounds.y > aspect_ratio {
        bounds.y * aspect_ratio
    } else {
        bounds.x
    };
    let maximum_width = MAX_SIZE.x.min(MAX_SIZE.y * aspect_ratio);
    let minimum_width = MIN_SIZE.x.min(maximum_width);
    let width = fitted_width.clamp(minimum_width, maximum_width);
    egui::vec2(width, width / aspect_ratio)
}

impl PopoutManager {
    pub(crate) fn has_active_popouts(&self) -> bool {
        !self.windows.is_empty()
    }

    pub(crate) fn has_tab_popout(&self, tab_id: u64) -> bool {
        self.windows.contains_key(&tab_id)
    }

    pub(crate) fn close_tab(&mut self, tab_id: u64) {
        if self.windows.remove(&tab_id).is_some() {
            self.pending_close.push(tab_id);
        }
    }

    fn open_tab(
        &mut self,
        tab_id: u64,
        texture: egui::TextureHandle,
        animation: Option<GifAnimation>,
        source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
        ctx: &egui::Context,
    ) {
        let image_size = texture.size_vec2().max(egui::vec2(1.0, 1.0));
        let aspect_ratio = (image_size.x / image_size.y).clamp(0.03, 32.0);
        let base_size = fit_aspect_size(aspect_ratio, DEFAULT_SIZE);
        let main_rect = ctx.input(|input| input.viewport().outer_rect);
        let monitors = crate::platform::monitors::available_monitors();
        let monitor = crate::platform::monitors::resolve_default_monitor_at(
            &monitors,
            crate::platform::monitors::CURRENT_WINDOW_MONITOR,
            main_rect,
        )
        .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
        .or_else(|| monitors.first());
        let position = monitor
            .map(|monitor| monitor.bounds().center() - base_size * 0.5)
            .unwrap_or(egui::pos2(120.0, 120.0));
        let monitor_id = monitor.map(|monitor| monitor.id.clone());
        let mut animation = animation;
        if let Some(animation) = animation.as_mut() {
            animation.is_playing = true;
            animation.last_update = Instant::now();
        }
        let display_texture = texture.clone();
        self.windows.insert(
            tab_id,
            Arc::new(Mutex::new(PopoutWindow {
                texture,
                animation,
                last_frame_index: None,
                size: base_size,
                base_size,
                aspect_ratio,
                size_scale: 1.0,
                pending_size_scale: 1.0,
                expected_native_size: None,
                expected_native_position: None,
                viewport_initialized: false,
                source_pixels,
                display_texture,
                rendered_opacity: u8::MAX,
                opacity_percent: 100.0,
                click_through: false,
                dvd_mode: false,
                dvd_speed: 1.0,
                dvd_edge_offsets: [0.0; 4],
                monitor_id,
                stay_on_top: false,
                stay_on_bottom: false,
                position,
                dvd_velocity: egui::vec2(1.0, 1.0),
                last_dvd_tick: Instant::now(),
                last_bottom_enforcement: Instant::now(),
                resize_gesture: None,
                close_requested: false,
                native_owner_detached: false,
            })),
        );
    }

    /// Compact, persistent settings panel rendered below the image in an Image tab.
    /// Most-used controls are arranged in two short rows. Less-frequent DVD monitor
    /// and edge-boundary settings live in a pop-up menu so they do not permanently
    /// consume image viewing space.
    pub(crate) fn render_navigation_panel(
        &mut self,
        ctx: &egui::Context,
        panel_rect: egui::Rect,
        tab_id: u64,
        texture: Option<egui::TextureHandle>,
        animation: Option<GifAnimation>,
        source_pixels: Option<(usize, usize, Arc<Vec<u8>>)>,
    ) {
        let mut open_requested = false;
        let mut close_requested = false;
        let monitors = crate::platform::monitors::available_monitors();
        let window_state = self.windows.get(&tab_id).cloned();
        let is_open = window_state.is_some();
        let mut window_guard = window_state.as_ref().and_then(|state| state.lock().ok());

        egui::Area::new(egui::Id::new(("jpeg_viewer_popout_settings", tab_id)))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_rect.min)
            .movable(false)
            .interactable(true)
            .show(ctx, |area_ui| {
                area_ui.set_min_size(panel_rect.size());
                area_ui.set_max_size(panel_rect.size());
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill.gamma_multiply(0.96))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ctx.style().visuals.widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(area_ui, |ui| {
                        ui.set_min_width((panel_rect.width() - 16.0).max(1.0));
                        ui.set_max_width((panel_rect.width() - 16.0).max(1.0));
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                        ui.spacing_mut().interact_size.y = 17.0;

                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Pop-out").strong().size(10.5));
                            if is_open {
                                ui.label(
                                    egui::RichText::new("● On")
                                        .color(egui::Color32::from_rgb(80, 190, 130))
                                        .size(10.0),
                                );
                                if ui.small_button("Close").clicked() {
                                    close_requested = true;
                                }
                            } else {
                                ui.label(egui::RichText::new("Off").weak().size(10.0));
                                if ui
                                    .add_enabled(texture.is_some(), egui::Button::new("Open"))
                                    .clicked()
                                {
                                    open_requested = true;
                                }
                            }

                            if let Some(window) = window_guard.as_deref_mut() {
                                ui.separator();
                                if ui
                                    .selectable_label(window.click_through, "⇢ Pass")
                                    .on_hover_text("Pass pointer clicks through the pop-out")
                                    .clicked()
                                {
                                    window.click_through = !window.click_through;
                                }
                                if ui
                                    .selectable_label(window.stay_on_top, "↑ Top")
                                    .on_hover_text("Keep the pop-out above other windows")
                                    .clicked()
                                {
                                    window.stay_on_top = !window.stay_on_top;
                                    if window.stay_on_top {
                                        window.stay_on_bottom = false;
                                    }
                                }
                                if ui
                                    .selectable_label(window.stay_on_bottom, "↓ Bottom")
                                    .on_hover_text("Keep the pop-out below other windows")
                                    .clicked()
                                {
                                    window.stay_on_bottom = !window.stay_on_bottom;
                                    if window.stay_on_bottom {
                                        window.stay_on_top = false;
                                    }
                                }

                                ui.menu_button("DVD ▾", |menu| {
                                    menu.checkbox(&mut window.dvd_mode, "DVD bounce");
                                    if window.dvd_mode {
                                        menu.separator();
                                        menu.horizontal(|ui| {
                                            ui.label("Speed");
                                            ui.add_sized(
                                                [112.0, 18.0],
                                                egui::Slider::new(&mut window.dvd_speed, 0.1..=4.0)
                                                    .suffix("×")
                                                    .show_value(false),
                                            );
                                        });

                                        menu.label(
                                            egui::RichText::new("Monitor").weak().size(10.0),
                                        );
                                        let monitor_name = window
                                            .monitor_id
                                            .as_deref()
                                            .and_then(|id| monitors.iter().find(|m| m.id == id))
                                            .map(|monitor| monitor.display_label())
                                            .unwrap_or_else(|| "Primary".to_owned());
                                        let mut selected_monitor = window.monitor_id.clone();
                                        egui::ComboBox::from_id_salt(("popout_monitor", tab_id))
                                            .selected_text(monitor_name)
                                            .width(190.0)
                                            .show_ui(menu, |combo| {
                                                for monitor in &monitors {
                                                    combo.selectable_value(
                                                        &mut selected_monitor,
                                                        Some(monitor.id.clone()),
                                                        monitor.display_label(),
                                                    );
                                                }
                                            });
                                        if selected_monitor != window.monitor_id {
                                            window.monitor_id = selected_monitor;
                                            if let Some(monitor) = window
                                                .monitor_id
                                                .as_deref()
                                                .and_then(|id| monitors.iter().find(|m| m.id == id))
                                            {
                                                window.request_outer_position(
                                                    ctx,
                                                    monitor.bounds().center() - window.size * 0.5,
                                                );
                                                window.last_dvd_tick = Instant::now();
                                            }
                                        }

                                        menu.separator();
                                        menu.label(
                                            egui::RichText::new("Monitor edge offsets").weak(),
                                        );
                                        for (index, label) in ["Left", "Top", "Right", "Bottom"]
                                            .into_iter()
                                            .enumerate()
                                        {
                                            menu.horizontal(|ui| {
                                                ui.label(label);
                                                ui.add_sized(
                                                    [150.0, 18.0],
                                                    egui::Slider::new(
                                                        &mut window.dvd_edge_offsets[index],
                                                        0.0..=1000.0,
                                                    )
                                                    .suffix("pt")
                                                    .show_value(false),
                                                );
                                            });
                                        }
                                    }
                                });
                            }
                        });

                        if let Some(window) = window_guard.as_deref_mut() {
                            ui.horizontal_wrapped(|ui| {
                                let min_scale =
                                    window.minimum_size().x / window.base_size.x.max(1.0);
                                let max_scale = (window.maximum_size().x
                                    / window.base_size.x.max(1.0))
                                .max(min_scale + 0.01);
                                let slider_width = (ui.available_width() * 0.25).clamp(72.0, 124.0);
                                ui.label(egui::RichText::new("Size").size(10.0));
                                let size_response = ui
                                    .add_sized(
                                        [slider_width, 15.0],
                                        egui::Slider::new(
                                            &mut window.pending_size_scale,
                                            min_scale..=max_scale,
                                        )
                                        .suffix("×")
                                        .show_value(true)
                                        .logarithmic(true),
                                    )
                                    .on_hover_text(
                                        "Apply the new size when you release the slider.",
                                    );
                                let commit_size = size_response.drag_stopped()
                                    || (size_response.changed() && !size_response.dragged());
                                if commit_size
                                    && (window.pending_size_scale - window.size_scale).abs() > 0.001
                                {
                                    window.size_scale = window.pending_size_scale;
                                    let requested_size = window.size_for_scale(window.size_scale);
                                    ctx.send_viewport_cmd_to(
                                        PopoutWindow::viewport_id(tab_id),
                                        egui::ViewportCommand::InnerSize(requested_size),
                                    );
                                    window.size = requested_size;
                                    window.expected_native_size =
                                        Some((requested_size, Instant::now(), 0));
                                }

                                ui.label(egui::RichText::new("Opacity").size(10.0));
                                ui.add_sized(
                                    [slider_width, 15.0],
                                    egui::Slider::new(&mut window.opacity_percent, 0.0..=100.0)
                                        .suffix("%")
                                        .show_value(false),
                                );
                            });
                        }
                    });
            });

        drop(window_guard);
        if close_requested {
            self.close_tab(tab_id);
        } else if open_requested && let Some(texture) = texture {
            self.open_tab(tab_id, texture, animation, source_pixels, ctx);
        }
    }

    pub(crate) fn sync_active_animation(&mut self, tab_id: u64, source: Option<&GifAnimation>) {
        let Some(window_state) = self.windows.get(&tab_id) else {
            return;
        };
        let Ok(mut window) = window_state.lock() else {
            return;
        };
        let Some(source) = source else {
            window.animation = None;
            window.last_frame_index = None;
            return;
        };

        let replace = window
            .animation
            .as_ref()
            .is_none_or(|current| current.frame_count() != source.frame_count());
        if replace {
            let mut animation = source.clone();
            animation.is_playing = true;
            animation.last_update = Instant::now();
            window.animation = Some(animation);
            window.last_frame_index = None;
        } else if let Some(animation) = window.animation.as_mut()
            && (animation.speed_multiplier - source.speed_multiplier).abs() > f32::EPSILON
        {
            animation.set_speed(source.speed_multiplier);
        }
    }

    /// Register deferred pop-out viewports. Their callbacks continue rendering
    /// and animating while the root application is minimized/occluded, unlike
    /// immediate viewports whose lifetime depended on the root update pass.
    pub(crate) fn render_viewports(&mut self, ctx: &egui::Context) {
        for tab_id in self.pending_close.drain(..) {
            ctx.send_viewport_cmd_to(
                PopoutWindow::viewport_id(tab_id),
                egui::ViewportCommand::Close,
            );
        }

        // A close button in a deferred child cannot directly borrow this manager.
        // It sets a flag in the shared window state; harvest those flags next root pass.
        let closed = self
            .windows
            .iter()
            .filter_map(|(tab_id, state)| {
                state
                    .lock()
                    .ok()
                    .filter(|window| window.close_requested)
                    .map(|_| *tab_id)
            })
            .collect::<Vec<_>>();
        for tab_id in closed {
            self.windows.remove(&tab_id);
        }

        let states = self
            .windows
            .iter()
            .map(|(tab_id, state)| (*tab_id, Arc::clone(state)))
            .collect::<Vec<_>>();
        for (tab_id, window_state) in states {
            let viewport_id = PopoutWindow::viewport_id(tab_id);
            let builder = match window_state.lock() {
                Ok(mut window) => window.viewport_builder(tab_id),
                Err(_) => continue,
            };
            let callback_state = Arc::clone(&window_state);
            ctx.show_viewport_deferred(viewport_id, builder, move |viewport_ctx, _class| {
                let Ok(mut window) = callback_state.lock() else {
                    return;
                };
                if window.close_requested {
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                if !window.native_owner_detached {
                    window.native_owner_detached = detach_popout_owner(tab_id);
                }
                let level = if window.stay_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else if window.stay_on_bottom {
                    egui::WindowLevel::AlwaysOnBottom
                } else {
                    egui::WindowLevel::Normal
                };
                viewport_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
                if window.stay_on_bottom {
                    viewport_ctx.request_repaint_after(Duration::from_millis(350));
                    if window.last_bottom_enforcement.elapsed() >= Duration::from_millis(350) {
                        enforce_bottom_z_order(tab_id);
                        window.last_bottom_enforcement = Instant::now();
                    }
                }
                let should_close = egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(viewport_ctx, |ui| window.render(ui, tab_id))
                    .inner;
                if should_close {
                    window.close_requested = true;
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn detach_popout_owner(tab_id: u64) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GW_OWNER, GWLP_HWNDPARENT, GetWindow, SetWindowLongPtrW,
    };
    use windows::core::PCWSTR;

    // Find by our unique tab-scoped title. Deferred viewports are created as
    // top-level owned windows; Win32 hides owned windows when their owner is
    // minimized. Clearing GWLP_HWNDPARENT makes the pop-out an independent
    // top-level window while keeping it in the same process and UI lifecycle.
    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) else {
            return false;
        };
        let owner_is_detached = || GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| owner.0.is_null());
        if owner_is_detached() {
            return true;
        }
        let _previous_owner = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
        owner_is_detached()
    }
}

#[cfg(not(target_os = "windows"))]
fn detach_popout_owner(_tab_id: u64) -> bool {
    // The native parent-owner minimize behavior addressed above is Windows
    // specific. Deferred viewports already repaint independently elsewhere.
    true
}

#[cfg(target_os = "windows")]
fn enforce_bottom_z_order(tab_id: u64) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };
    use windows::core::PCWSTR;

    let title = format!("Pop-out · JPEG Viewer · Tab {tab_id}");
    let title_wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title_wide.as_ptr())) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn enforce_bottom_z_order(_tab_id: u64) {
    // Other window managers receive egui's WindowLevel::AlwaysOnBottom hint.
}

#[cfg(test)]
mod tests {
    use super::{apply_opacity_to_rgba, fit_aspect_size, opacity_to_alpha};

    #[test]
    fn opacity_scales_only_alpha_and_preserves_rgb() {
        let source = [120, 80, 40, 255, 9, 19, 29, 64];
        let half = apply_opacity_to_rgba(&source, 128);
        assert_eq!(&half[0..3], &[120, 80, 40]);
        assert_eq!(half[3], 128);
        assert_eq!(&half[4..7], &[9, 19, 29]);
        assert_eq!(half[7], 32);
    }

    #[test]
    fn zero_opacity_makes_pixels_transparent_without_changing_rgb() {
        let source = [200, 100, 50, 255];
        assert_eq!(apply_opacity_to_rgba(&source, 0), [200, 100, 50, 0]);
        assert_eq!(opacity_to_alpha(100.0), u8::MAX);
        assert_eq!(opacity_to_alpha(0.0), 0);
    }

    #[test]
    fn window_fit_keeps_the_source_aspect_ratio() {
        let size = fit_aspect_size(16.0 / 9.0, egui::vec2(560.0, 420.0));
        assert!((size.x / size.y - 16.0 / 9.0).abs() < 0.001);
        assert!(size.x <= 560.0 && size.y <= 420.0);
    }
}
