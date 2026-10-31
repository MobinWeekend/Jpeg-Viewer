//! Decorative "Tricks" effects rendered strictly inside the main image viewport.
//!
//! The central panel is painted before foreground navigation, menus, and sidebars,
//! so these effects can decorate the image without ever covering application UI.
use crate::app::types::ViewerApp;
use crate::settings::AppSettings;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

impl ViewerApp {
    /// Derive a tiny backdrop source from the already-decoded image buffer. This avoids
    /// opening and decoding the same file a second time just for the decorative blur.
    pub(crate) fn cache_tricks_backdrop_from_decoded(
        &mut self,
        path: Option<PathBuf>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) {
        if !self.settings_manager.get().tricks_backdrop_enabled {
            return;
        }
        let Some(path) = path else { return };
        let expected = width as u64 * height as u64 * 4;
        if width == 0 || height == 0 || expected != rgba.len() as u64 {
            return;
        }

        self.tricks_backdrop_source_requested_path = Some(path.clone());
        self.tricks_backdrop_source_preview = None;
        self.tricks_backdrop_key = None;
        // Keep the old backdrop visible while the new decoded image is reduced,
        // blurred on a worker, uploaded, and cross-faded into view.
        self.tricks_backdrop_next_texture = None;
        self.tricks_backdrop_transition_started = None;
        self.tricks_backdrop_receiver = None;
        let (tx, rx) = mpsc::channel();
        self.tricks_backdrop_source_receiver = Some(rx);
        // Move the decoded buffer into the worker instead of cloning a potentially
        // hundreds-of-megabytes RGBA image on the UI thread.
        std::thread::spawn(move || {
            // Read only the pixels needed for the 256px decorative preview. The
            // previous imageops::thumbnail path scanned/resampled the entire source
            // image, which could saturate a CPU core for very large photographs.
            // This nearest-neighbour reduction has a fixed upper bound of 65,536
            // destination pixels and runs entirely on this worker thread.
            let result = downsample_rgba_nearest(width, height, rgba, 256);
            let _ = tx.send((path, result));
        });
    }

    /// Load/cache optional assets asynchronously. The backdrop always works from a
    /// 256x256-or-smaller source; its blur is performed on that small buffer only.
    pub(super) fn update_tricks_assets(&mut self, ctx: &egui::Context, settings: &AppSettings) {
        let current_path = self.current_image_path.clone();
        let current_ready = self.texture.is_some() || self.gif_animation.is_some();

        // A decoded source belongs to one image only. Discard it as soon as navigation
        // changes, but retain it while the backdrop toggle is off so re-enabling is fast.
        if self
            .tricks_backdrop_source_preview
            .as_ref()
            .is_some_and(|(path, ..)| Some(path) != current_path.as_ref())
            || self
                .tricks_backdrop_source_requested_path
                .as_ref()
                .is_some_and(|path| Some(path) != current_path.as_ref())
        {
            self.tricks_backdrop_source_preview = None;
            self.tricks_backdrop_source_requested_path = None;
            self.tricks_backdrop_source_receiver = None;
            self.tricks_backdrop_key = None;
            // Keep the currently visible texture until a preview for the new image
            // has been blurred and uploaded. Discard only an unfinished transition.
            self.tricks_backdrop_next_texture = None;
            self.tricks_backdrop_transition_started = None;
            self.tricks_backdrop_receiver = None;
        }

        // Promote the incoming texture after the cross-fade. Until this point the
        // previous texture remains alive and visible underneath it.
        if self
            .tricks_backdrop_transition_started
            .is_some_and(|started| started.elapsed() >= Duration::from_millis(320))
        {
            if let Some(next) = self.tricks_backdrop_next_texture.take() {
                self.tricks_backdrop_texture = Some(next);
            }
            self.tricks_backdrop_transition_started = None;
        }

        if settings.tricks_backdrop_enabled && self.virtual_texture.is_none() {
            if let Some(path) = current_path.clone() {
                // Cache hits do not carry decoded pixels, so use a bounded background
                // decode only in that case. Normal image loads populate the preview
                // directly from LoadedImage::Static above.
                if current_ready
                    && self.tricks_backdrop_source_preview.is_none()
                    && self.tricks_backdrop_source_receiver.is_none()
                    && self.tricks_backdrop_source_requested_path.as_ref() != Some(&path)
                {
                    self.tricks_backdrop_source_requested_path = Some(path.clone());
                    let (tx, rx) = mpsc::channel();
                    self.tricks_backdrop_source_receiver = Some(rx);
                    std::thread::spawn(move || {
                        // This fallback is only used for a texture-cache hit where the
                        // decoded pixels are unavailable. Keep its decode ceiling much
                        // lower than the main viewer's image limit; the decorative
                        // backdrop does not justify decoding enormous sources again.
                        let result = open_bounded_image(&path, 12_000_000).and_then(|image| {
                            let width = image.width();
                            let height = image.height();
                            downsample_rgba_nearest(width, height, image.to_rgba8().into_raw(), 256)
                        });
                        let _ = tx.send((path, result));
                    });
                }
            } else {
                self.tricks_backdrop_key = None;
                self.tricks_backdrop_texture = None;
                self.tricks_backdrop_next_texture = None;
                self.tricks_backdrop_transition_started = None;
                self.tricks_backdrop_receiver = None;
            }
        } else if !settings.tricks_backdrop_enabled {
            // Disabling the effect explicitly removes both buffers immediately.
            self.tricks_backdrop_key = None;
            self.tricks_backdrop_texture = None;
            self.tricks_backdrop_next_texture = None;
            self.tricks_backdrop_transition_started = None;
            self.tricks_backdrop_receiver = None;
        } else {
            // Virtual-texture images are not prepared for this effect. Keep the last
            // successfully uploaded backdrop rather than leaving the viewport blank.
            self.tricks_backdrop_key = None;
            self.tricks_backdrop_next_texture = None;
            self.tricks_backdrop_transition_started = None;
            self.tricks_backdrop_receiver = None;
        }

        let source_result = self
            .tricks_backdrop_source_receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match source_result {
            Some(Ok((path, Ok((width, height, pixels))))) => {
                self.tricks_backdrop_source_receiver = None;
                if self.tricks_backdrop_source_requested_path.as_ref() == Some(&path) {
                    self.tricks_backdrop_source_preview = Some((path, width, height, pixels));
                }
            }
            Some(Ok((_path, Err(error)))) => {
                self.tricks_backdrop_source_receiver = None;
                // Keep the requested path as a failed marker. Retrying the same
                // decode on every frame would create an unbounded thread loop.
                eprintln!("Tricks: could not create backdrop preview: {error}");
            }
            Some(Err(TryRecvError::Disconnected)) => self.tricks_backdrop_source_receiver = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        if settings.tricks_backdrop_enabled && self.virtual_texture.is_none() {
            if let Some((path, width, height, pixels)) =
                self.tricks_backdrop_source_preview.as_ref()
            {
                let blur = settings.tricks_backdrop_blur.round().clamp(8.0, 64.0) as u8;
                let key = (path.clone(), blur);
                if self.tricks_backdrop_key.as_ref() != Some(&key) {
                    self.tricks_backdrop_key = Some(key.clone());
                    // A newer setting supersedes an in-progress fade, but never clears
                    // the visible texture while the replacement is being prepared.
                    self.tricks_backdrop_next_texture = None;
                    self.tricks_backdrop_transition_started = None;
                    let result_path = path.clone();
                    let source = pixels.clone();
                    let source_width = *width;
                    let source_height = *height;
                    let (tx, rx) = mpsc::channel();
                    self.tricks_backdrop_receiver = Some(rx);
                    std::thread::spawn(move || {
                        let result = image::RgbaImage::from_raw(
                            source_width as u32,
                            source_height as u32,
                            source,
                        )
                        .map(|preview| {
                            // The source is capped at 256px, so scale the blur radius too.
                            image::imageops::blur(&preview, (blur as f32 / 4.0).clamp(1.0, 16.0))
                        })
                        .map(|blurred| {
                            let size = (blurred.width() as usize, blurred.height() as usize);
                            (size.0, size.1, blurred.into_raw())
                        })
                        .ok_or_else(|| "Backdrop preview buffer has an invalid size".to_owned());
                        let _ = tx.send((result_path, blur, result));
                    });
                }
            }
        }

        let backdrop_result = self
            .tricks_backdrop_receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match backdrop_result {
            Some(Ok((path, blur, Ok((width, height, pixels))))) => {
                self.tricks_backdrop_receiver = None;
                if self.tricks_backdrop_key.as_ref() == Some(&(path, blur)) {
                    let color = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
                    let next = ctx.load_texture(
                        "tricks_blurred_backdrop",
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    if self.tricks_backdrop_texture.is_some() {
                        self.tricks_backdrop_next_texture = Some(next);
                        self.tricks_backdrop_transition_started = Some(std::time::Instant::now());
                    } else {
                        self.tricks_backdrop_texture = Some(next);
                        self.tricks_backdrop_next_texture = None;
                        self.tricks_backdrop_transition_started = None;
                    }
                }
            }
            Some(Ok((_path, _blur, Err(error)))) => {
                self.tricks_backdrop_receiver = None;
                eprintln!("Tricks: could not blur current image preview: {error}");
            }
            Some(Err(TryRecvError::Disconnected)) => self.tricks_backdrop_receiver = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        // Watermark images: static formats use a bounded thumbnail; GIFs use a
        // bounded animation decoder and advance independently of the main image GIF.
        if settings.tricks_watermark_enabled
            && !settings.tricks_watermark_image_path.trim().is_empty()
        {
            let path = PathBuf::from(settings.tricks_watermark_image_path.trim());
            if self.tricks_watermark_requested_path.as_ref() != Some(&path) {
                self.tricks_watermark_requested_path = Some(path.clone());
                self.tricks_watermark_texture = None;
                self.tricks_watermark_receiver = None;
                self.tricks_watermark_gif = None;
                self.tricks_watermark_gif_frame = None;
                self.tricks_watermark_gif_receiver = None;

                let is_gif = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
                if is_gif {
                    let (tx, rx) = mpsc::channel();
                    self.tricks_watermark_gif_receiver = Some(rx);
                    std::thread::spawn(move || {
                        let result = (|| {
                            let metadata =
                                std::fs::metadata(&path).map_err(|error| error.to_string())?;
                            if metadata.len() > 64 * 1024 * 1024 {
                                return Err(
                                    "GIF watermark exceeds the 64 MB safety limit".to_owned()
                                );
                            }
                            let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
                            crate::gif::animation::GifAnimation::from_bytes_bounded(
                                &bytes, 180, 12_000_000, 256,
                            )
                        })();
                        let _ = tx.send((path, result));
                    });
                } else {
                    let (tx, rx) = mpsc::channel();
                    self.tricks_watermark_receiver = Some(rx);
                    std::thread::spawn(move || {
                        let result = open_bounded_image(&path, 30_000_000)
                            .map(|image| image.thumbnail(1024, 1024).to_rgba8())
                            .map(|preview| {
                                let size = (preview.width() as usize, preview.height() as usize);
                                (size.0, size.1, preview.into_raw())
                            });
                        let _ = tx.send((path, result));
                    });
                }
            }
        } else if settings.tricks_watermark_image_path.trim().is_empty() {
            self.tricks_watermark_requested_path = None;
            self.tricks_watermark_texture = None;
            self.tricks_watermark_receiver = None;
            self.tricks_watermark_gif = None;
            self.tricks_watermark_gif_frame = None;
            self.tricks_watermark_gif_receiver = None;
        }

        let watermark_result = self
            .tricks_watermark_receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match watermark_result {
            Some(Ok((path, Ok((width, height, pixels))))) => {
                self.tricks_watermark_receiver = None;
                if self.tricks_watermark_requested_path.as_ref() == Some(&path) {
                    let color = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
                    self.tricks_watermark_texture = Some(ctx.load_texture(
                        "tricks_watermark_logo",
                        color,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            Some(Ok((_path, Err(error)))) => {
                self.tricks_watermark_receiver = None;
                eprintln!("Tricks: could not load watermark image: {error}");
            }
            Some(Err(TryRecvError::Disconnected)) => self.tricks_watermark_receiver = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        let watermark_gif_result = self
            .tricks_watermark_gif_receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match watermark_gif_result {
            Some(Ok((path, Ok(mut gif)))) => {
                self.tricks_watermark_gif_receiver = None;
                if self.tricks_watermark_requested_path.as_ref() == Some(&path) {
                    gif.is_playing = true;
                    gif.last_update = std::time::Instant::now();
                    if let Some(frame) = gif.get_current_frame_ref() {
                        let size = [frame.width() as usize, frame.height() as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());
                        if let Some(texture) = self.tricks_watermark_texture.as_mut() {
                            texture.set(color, egui::TextureOptions::LINEAR);
                        } else {
                            self.tricks_watermark_texture = Some(ctx.load_texture(
                                "tricks_watermark_gif",
                                color,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                        self.tricks_watermark_gif_frame = Some(0);
                    }
                    self.tricks_watermark_gif = Some(gif);
                }
            }
            Some(Ok((_path, Err(error)))) => {
                self.tricks_watermark_gif_receiver = None;
                eprintln!("Tricks: could not load animated watermark: {error}");
            }
            Some(Err(TryRecvError::Disconnected)) => self.tricks_watermark_gif_receiver = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        // Upload only when the GIF frame changes; request repaint at the next frame's
        // actual delay so a static logo does not drive continuous repainting.
        let mut next_gif_frame = None;
        let mut next_gif_repaint = None;
        if let Some(gif) = self.tricks_watermark_gif.as_mut() {
            // Read all decoder state before borrowing the current frame mutably.
            // `get_current_frame()` keeps a mutable borrow alive while the frame
            // buffer is read, so querying the decoder again inside that borrow
            // would violate Rust's aliasing rules.
            let index = gif.get_current_frame_index();
            let repaint_delay =
                if settings.tricks_watermark_enabled && gif.is_animated() && gif.is_playing {
                    Some(
                        gif.delays
                            .get(index)
                            .copied()
                            .unwrap_or(Duration::from_millis(100)),
                    )
                } else {
                    None
                };
            let elapsed = gif.last_update.elapsed();

            if let Some(frame) = gif.get_current_frame() {
                if self.tricks_watermark_gif_frame != Some(index) {
                    next_gif_frame = Some((
                        index,
                        [frame.width() as usize, frame.height() as usize],
                        frame.as_raw().clone(),
                    ));
                }
            }

            if let Some(delay) = repaint_delay {
                let remaining = delay.saturating_sub(elapsed);
                next_gif_repaint = Some(remaining.max(Duration::from_millis(16)));
            }
        }
        if let Some((index, size, pixels)) = next_gif_frame {
            let color = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            if let Some(texture) = self.tricks_watermark_texture.as_mut() {
                texture.set(color, egui::TextureOptions::LINEAR);
            } else {
                self.tricks_watermark_texture = Some(ctx.load_texture(
                    "tricks_watermark_gif",
                    color,
                    egui::TextureOptions::LINEAR,
                ));
            }
            self.tricks_watermark_gif_frame = Some(index);
        }
        if let Some(delay) = next_gif_repaint {
            ctx.request_repaint_after(delay);
        }

        if self.tricks_backdrop_transition_started.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.tricks_backdrop_source_receiver.is_some()
            || self.tricks_backdrop_receiver.is_some()
            || self.tricks_watermark_receiver.is_some()
            || self.tricks_watermark_gif_receiver.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }

    pub(super) fn paint_tricks_backdrop(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        main_color: egui::Color32,
        tint_strength: f32,
        opacity: f32,
    ) {
        let clip = rect.intersect(painter.clip_rect());
        if !clip.is_positive() {
            return;
        }

        let clip_painter = painter.with_clip_rect(clip);
        let progress = self
            .tricks_backdrop_transition_started
            .map(|started| (started.elapsed().as_secs_f32() / 0.320).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let opacity = opacity.clamp(0.0, 1.0);

        if let Some(texture) = &self.tricks_backdrop_texture {
            paint_backdrop_texture(&clip_painter, texture, rect, opacity * (1.0 - progress));
        }
        if let Some(texture) = &self.tricks_backdrop_next_texture {
            paint_backdrop_texture(&clip_painter, texture, rect, opacity * progress);
        }

        // The main-color tint is painted once over the blended textures, so its
        // strength remains stable throughout the transition.
        let strength = tint_strength.clamp(0.0, 0.85);
        if strength > 0.0
            && (self.tricks_backdrop_texture.is_some()
                || self.tricks_backdrop_next_texture.is_some())
        {
            let [r, g, b, _] = main_color.to_srgba_unmultiplied();
            let tint = egui::Color32::from_rgba_unmultiplied(
                r,
                g,
                b,
                (strength * opacity * 255.0).round() as u8,
            );
            clip_painter.rect_filled(rect, 0.0, tint);
        }
    }

    pub(super) fn paint_tricks_snow(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        settings: &AppSettings,
        time: f64,
    ) {
        if !settings.tricks_snow_enabled || settings.tricks_snow_particle.trim().is_empty() {
            return;
        }
        let clip = rect.intersect(painter.clip_rect());
        if !clip.is_positive() {
            return;
        }
        let area = clip.width() * clip.height();
        let count = ((area / 13_500.0) * settings.tricks_snow_density.clamp(0.25, 2.5))
            .round()
            .clamp(14.0, 150.0) as usize;
        let speed = settings.tricks_snow_speed.clamp(0.1, 5.0);
        let particle = settings.tricks_snow_particle.trim();
        let particle_color =
            parse_hex_color(&settings.tricks_snow_color).unwrap_or(egui::Color32::WHITE);
        let [red, green, blue, _] = particle_color.to_srgba_unmultiplied();
        let painter = painter.with_clip_rect(clip);
        for index in 0..count {
            let seed = pseudo_random(index as u32 * 3 + 11);
            let seed_y = pseudo_random(index as u32 * 3 + 29);
            let seed_size = pseudo_random(index as u32 * 3 + 47);
            let x = clip.left() + seed * clip.width();
            let phase =
                (seed_y + (time as f32 * speed * (0.10 + seed_size * 0.06))).rem_euclid(1.0);
            let y = clip.top() + phase * clip.height();
            let drift = ((time as f32 * (0.22 + seed * 0.25) + seed_y * 9.0).sin())
                * (4.0 + seed_size * 10.0);
            let size = 10.0 + seed_size * 14.0;
            let alpha = (90.0 + seed * 150.0).round() as u8;
            painter.text(
                egui::pos2(x + drift, y),
                egui::Align2::CENTER_CENTER,
                particle,
                egui::FontId::proportional(size),
                egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha),
            );
        }
    }

    pub(super) fn paint_tricks_watermark(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        settings: &AppSettings,
        time: f64,
        text_color: egui::Color32,
    ) {
        if !settings.tricks_watermark_enabled {
            return;
        }
        let text = settings.tricks_watermark_text.trim();
        let text_color =
            parse_hex_color(&settings.tricks_watermark_text_color).unwrap_or(text_color);
        let logo = self.tricks_watermark_texture.as_ref();
        if text.is_empty() && logo.is_none() {
            return;
        }
        let clip = rect.intersect(painter.clip_rect());
        if !clip.is_positive() {
            return;
        }
        let opacity = settings.tricks_watermark_opacity.clamp(0.05, 1.0);
        let max_size = settings.tricks_watermark_size.clamp(12.0, 160.0);
        let logo_size = logo.map(|texture| {
            let source = texture.size_vec2();
            let scale = max_size / source.x.max(source.y).max(1.0);
            source * scale
        });
        let text_galley = if text.is_empty() {
            None
        } else {
            Some(painter.layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(max_size),
                text_color,
            ))
        };
        let logo_gap = if logo_size.is_some() && text_galley.is_some() {
            8.0
        } else {
            0.0
        };
        let text_size = text_galley
            .as_ref()
            .map(|galley| galley.size())
            .unwrap_or(egui::Vec2::ZERO);
        let content_width = logo_size.map(|size| size.x).unwrap_or(0.0).max(text_size.x);
        let content_height = logo_size.map(|size| size.y).unwrap_or(0.0) + logo_gap + text_size.y;
        let max_x = (clip.width() - content_width).max(0.0);
        let max_y = (clip.height() - content_height).max(0.0);
        let (x, y) = if settings.tricks_watermark_mode == "bounce" {
            let speed = settings.tricks_watermark_speed.clamp(0.1, 5.0);
            let x_phase = reflected_phase(time as f32 * speed * 0.17);
            let y_phase = reflected_phase(time as f32 * speed * 0.13 + 0.61);
            (clip.left() + x_phase * max_x, clip.top() + y_phase * max_y)
        } else {
            (
                clip.left() + max_x * (settings.tricks_watermark_x.clamp(0.0, 100.0) / 100.0),
                clip.top() + max_y * (settings.tricks_watermark_y.clamp(0.0, 100.0) / 100.0),
            )
        };
        let painter = painter.with_clip_rect(clip);
        let mut cursor_y = y;
        if let (Some(texture), Some(size)) = (logo, logo_size) {
            let logo_rect = egui::Rect::from_min_size(
                egui::pos2(x + (content_width - size.x) * 0.5, cursor_y),
                size,
            );
            painter.image(
                texture.id(),
                logo_rect,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                egui::Color32::from_white_alpha((opacity * 255.0).round() as u8),
            );
            cursor_y += size.y + logo_gap;
        }
        if let Some(galley) = text_galley {
            let color = egui::Color32::from_rgba_unmultiplied(
                text_color.r(),
                text_color.g(),
                text_color.b(),
                (text_color.a() as f32 * opacity).round() as u8,
            );
            painter.galley(
                egui::pos2(x + (content_width - galley.size().x) * 0.5, cursor_y),
                galley,
                color,
            );
        }
    }
}

fn paint_backdrop_texture(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    rect: egui::Rect,
    alpha: f32,
) {
    if alpha <= 0.001 {
        return;
    }
    let texture_size = texture.size_vec2();
    let source_aspect = texture_size.x / texture_size.y.max(1.0);
    let target_aspect = rect.width() / rect.height().max(1.0);
    // Crop the wider dimension so the backdrop covers the viewport without stretching.
    let uv = if source_aspect > target_aspect {
        let visible_width = target_aspect / source_aspect;
        egui::Rect::from_min_max(
            egui::pos2((1.0 - visible_width) * 0.5, 0.0),
            egui::pos2(1.0 - (1.0 - visible_width) * 0.5, 1.0),
        )
    } else {
        let visible_height = source_aspect / target_aspect.max(0.001);
        egui::Rect::from_min_max(
            egui::pos2(0.0, (1.0 - visible_height) * 0.5),
            egui::pos2(1.0, 1.0 - (1.0 - visible_height) * 0.5),
        )
    };
    painter.image(
        texture.id(),
        rect,
        uv,
        egui::Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0).round() as u8),
    );
}

fn parse_hex_color(value: &str) -> Option<egui::Color32> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(egui::Color32::from_rgb(
        ((rgb >> 16) & 0xFF) as u8,
        ((rgb >> 8) & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    ))
}

/// Reduce an RGBA image to a small preview by sampling only the destination
/// pixels. Unlike imageops::thumbnail this does not iterate over every source
/// pixel, which keeps decorative-backdrop preparation bounded for huge images.
fn downsample_rgba_nearest(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    max_dimension: u32,
) -> Result<(usize, usize, Vec<u8>), String> {
    if width == 0 || height == 0 || max_dimension == 0 {
        return Err("Backdrop source has invalid dimensions".to_owned());
    }

    let expected = (width as u64)
        .checked_mul(height as u64)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Backdrop source dimensions overflow".to_owned())?;
    if expected != rgba.len() as u64 {
        return Err("Backdrop source buffer has an invalid size".to_owned());
    }

    let scale = (max_dimension as f64 / width.max(height) as f64).min(1.0);
    let output_width = ((width as f64 * scale).round() as u32).max(1);
    let output_height = ((height as f64 * scale).round() as u32).max(1);
    let output_len = output_width as usize * output_height as usize * 4;
    let mut output = vec![0_u8; output_len];

    for y in 0..output_height {
        let source_y = (((2 * y as u64 + 1) * height as u64) / (2 * output_height as u64))
            .min(height as u64 - 1) as usize;
        for x in 0..output_width {
            let source_x = (((2 * x as u64 + 1) * width as u64) / (2 * output_width as u64))
                .min(width as u64 - 1) as usize;
            let source_offset = (source_y * width as usize + source_x) * 4;
            let target_offset = (y as usize * output_width as usize + x as usize) * 4;
            output[target_offset..target_offset + 4]
                .copy_from_slice(&rgba[source_offset..source_offset + 4]);
        }
    }

    Ok((output_width as usize, output_height as usize, output))
}

fn open_bounded_image(path: &PathBuf, max_pixels: u64) -> Result<image::DynamicImage, String> {
    let (width, height) = image::image_dimensions(path).map_err(|error| error.to_string())?;
    let pixel_count = width as u64 * height as u64;
    if pixel_count > max_pixels {
        return Err(format!(
            "Image is too large for this decorative effect ({width}×{height})"
        ));
    }
    image::open(path).map_err(|error| error.to_string())
}

fn pseudo_random(value: u32) -> f32 {
    let mut x = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    x ^= x >> 16;
    (x & 0x00FF_FFFF) as f32 / 16_777_215.0
}

fn reflected_phase(value: f32) -> f32 {
    let phase = value.rem_euclid(2.0);
    if phase <= 1.0 { phase } else { 2.0 - phase }
}

/// Settings section for the three main-view decoration effects.
pub fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("🎉 Tricks").strong(), |ui| {
        ui.label(egui::RichText::new("Decorative effects inside the main image view. They are rendered below application controls and sidebars.").small().weak());

        ui.separator();
        ui.label(egui::RichText::new("❄ Falling particles").strong());
        let settings = app.settings_manager.get().clone();
        let mut enabled = settings.tricks_snow_enabled;
        if crate::ui_theme::switch(ui, &mut enabled, "Enable falling particles") {
            let _ = app.settings_manager.update(|s| s.tricks_snow_enabled = enabled);
            ctx.request_repaint();
        }
        if enabled {
            let mut particle = settings.tricks_snow_particle.clone();
            ui.horizontal(|ui| {
                ui.label("Particle");
                if ui.add(egui::TextEdit::singleline(&mut particle).desired_width(100.0)).changed() {
                    let value = particle.chars().take(8).collect::<String>();
                    let _ = app.settings_manager.update(|s| s.tricks_snow_particle = value);
                }
            });
            let mut particle_color = parse_hex_color(&settings.tricks_snow_color).unwrap_or(egui::Color32::WHITE);
            ui.horizontal(|ui| {
                ui.label("Particle color");
                if ui.color_edit_button_srgba(&mut particle_color).changed() {
                    let value = format!("#{:02X}{:02X}{:02X}", particle_color.r(), particle_color.g(), particle_color.b());
                    let _ = app.settings_manager.update(|s| s.tricks_snow_color = value);
                }
            });
            let mut speed = settings.tricks_snow_speed;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut speed, 0.1..=5.0).text("Speed"))).changed() {
                let _ = app.settings_manager.update(|s| s.tricks_snow_speed = speed.clamp(0.1, 5.0));
            }
            let mut density = settings.tricks_snow_density;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut density, 0.25..=2.5).text("Density"))).changed() {
                let _ = app.settings_manager.update(|s| s.tricks_snow_density = density.clamp(0.25, 2.5));
            }
            let mut above = settings.tricks_snow_above_image;
            if ui.checkbox(&mut above, "Draw particles over the photo (otherwise behind it)").changed() {
                let _ = app.settings_manager.update(|s| s.tricks_snow_above_image = above);
            }
        }

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("◇ Watermark").strong());
        let settings = app.settings_manager.get().clone();
        let mut enabled = settings.tricks_watermark_enabled;
        if crate::ui_theme::switch(ui, &mut enabled, "Enable watermark") {
            let _ = app.settings_manager.update(|s| s.tricks_watermark_enabled = enabled);
            ctx.request_repaint();
        }
        if enabled {
            let mut text = settings.tricks_watermark_text.clone();
            ui.horizontal(|ui| {
                ui.label("Text");
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width() - 44.0)).changed() {
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_text = text.clone());
                }
            });
            let mut text_color = parse_hex_color(&settings.tricks_watermark_text_color)
                .unwrap_or(egui::Color32::WHITE);
            ui.horizontal(|ui| {
                ui.label("Text color");
                if ui.color_edit_button_srgba(&mut text_color).changed() {
                    let value = format!("#{:02X}{:02X}{:02X}", text_color.r(), text_color.g(), text_color.b());
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_text_color = value);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Choose logo…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Watermark image", &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"])
                        .pick_file()
                    {
                        let value = path.to_string_lossy().into_owned();
                        let _ = app.settings_manager.update(|s| s.tricks_watermark_image_path = value);
                    }
                }
                if !settings.tricks_watermark_image_path.is_empty() && ui.small_button("Clear").clicked() {
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_image_path.clear());
                }
            });
            if !settings.tricks_watermark_image_path.is_empty() {
                let name = PathBuf::from(&settings.tricks_watermark_image_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| settings.tricks_watermark_image_path.clone());
                ui.label(egui::RichText::new(name).small().weak());
            }
            let mut mode = settings.tricks_watermark_mode.clone();
            egui::ComboBox::from_id_salt("tricks_watermark_mode")
                .selected_text(if mode == "bounce" { "DVD bounce" } else { "Anchored position" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, "anchored".to_owned(), "Anchored position");
                    ui.selectable_value(&mut mode, "bounce".to_owned(), "DVD bounce");
                });
            if mode != settings.tricks_watermark_mode {
                let _ = app.settings_manager.update(|s| s.tricks_watermark_mode = mode);
            }
            if settings.tricks_watermark_mode == "bounce" {
                let mut speed = settings.tricks_watermark_speed;
                if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut speed, 0.1..=5.0).text("Bounce speed"))).changed() {
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_speed = speed.clamp(0.1, 5.0));
                }
            } else {
                let mut x = settings.tricks_watermark_x;
                if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut x, 0.0..=100.0).text("Horizontal position").suffix("%"))).changed() {
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_x = x.clamp(0.0, 100.0));
                }
                let mut y = settings.tricks_watermark_y;
                if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut y, 0.0..=100.0).text("Vertical position").suffix("%"))).changed() {
                    let _ = app.settings_manager.update(|s| s.tricks_watermark_y = y.clamp(0.0, 100.0));
                }
            }
            let mut opacity = (settings.tricks_watermark_opacity * 100.0).round() as i32;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut opacity, 5..=100).text("Opacity").suffix("%"))).changed() {
                let value = opacity as f32 / 100.0;
                let _ = app.settings_manager.update(|s| s.tricks_watermark_opacity = value.clamp(0.05, 1.0));
            }
            let mut size = settings.tricks_watermark_size;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut size, 12.0..=160.0).text("Size"))).changed() {
                let _ = app.settings_manager.update(|s| s.tricks_watermark_size = size.clamp(12.0, 160.0));
            }
        }

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("▧ Blurred photo backdrop").strong());
        let settings = app.settings_manager.get().clone();
        let mut enabled = settings.tricks_backdrop_enabled;
        if crate::ui_theme::switch(ui, &mut enabled, "Show blurred photo behind the image") {
            let _ = app.settings_manager.update(|s| s.tricks_backdrop_enabled = enabled);
            ctx.request_repaint();
        }
        if enabled {
            let mut blur = settings.tricks_backdrop_blur;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut blur, 8.0..=64.0).text("Blur radius"))).changed() {
                let _ = app.settings_manager.update(|s| s.tricks_backdrop_blur = blur.clamp(8.0, 64.0));
            }
            let mut tint = (settings.tricks_backdrop_tint * 100.0).round() as i32;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut tint, 0..=85).text("Main color tint").suffix("%"))).changed() {
                let value = tint as f32 / 100.0;
                let _ = app.settings_manager.update(|s| s.tricks_backdrop_tint = value.clamp(0.0, 0.85));
            }
            let mut transparency = ((1.0 - settings.tricks_backdrop_opacity) * 100.0)
                .round()
                .clamp(0.0, 100.0) as i32;
            if ui.add(crate::ui_theme::BorderedSlider::new(egui::Slider::new(&mut transparency, 0..=100).text("Transparency").suffix("%"))).changed() {
                let value = 1.0 - transparency as f32 / 100.0;
                let _ = app.settings_manager.update(|s| s.tricks_backdrop_opacity = value.clamp(0.0, 1.0));
            }
            ui.label(egui::RichText::new("The backdrop is built from a cached 256×256-or-smaller preview of the decoded image, then tinted with the program's Main color. Archive entries and virtual-texture images are skipped.").small().weak());
        }
    });
}

#[cfg(test)]
mod backdrop_preview_tests {
    use super::downsample_rgba_nearest;

    #[test]
    fn backdrop_preview_is_bounded_to_maximum_dimension() {
        let width = 1024;
        let height = 512;
        let pixels = vec![127_u8; width as usize * height as usize * 4];
        let (preview_width, preview_height, preview) =
            downsample_rgba_nearest(width, height, pixels, 256).unwrap();
        assert_eq!((preview_width, preview_height), (256, 128));
        assert_eq!(preview.len(), 256 * 128 * 4);
    }

    #[test]
    fn backdrop_preview_rejects_invalid_source_length() {
        assert!(downsample_rgba_nearest(2, 2, vec![0; 12], 256).is_err());
    }
}
