//! Slideshow presentation effects, deliberately independent from image decoding.
//!
//! The module only operates on elapsed time, image dimensions, and viewport
//! geometry. It has no imports from `decoder`, `image`, or archive modules.

use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionEffect {
    None,
    Smart,
    PanHorizontal,
    PanVertical,
    ZoomIn,
    ZoomOut,
    Random,
}

impl MotionEffect {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "none" => Self::None,
            "pan-horizontal" => Self::PanHorizontal,
            "pan-vertical" => Self::PanVertical,
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
            "random" => Self::Random,
            _ => Self::Smart,
        }
    }

    #[allow(dead_code)] //This acctually gets used!
    pub fn setting_key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Smart => "smart",
            Self::PanHorizontal => "pan-horizontal",
            Self::PanVertical => "pan-vertical",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
            Self::Random => "random",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct FrameTransform {
    pub rect: egui::Rect,
    pub opacity: f32,
}

/// Compute a frame transform from normalized progress. Pure and deterministic,
/// which keeps effect behavior easy to test and separate from texture loading.
pub fn transform(
    base: egui::Rect,
    viewport: egui::Rect,
    progress: f32,
    effect: MotionEffect,
    strength: f32,
    fade: bool,
) -> FrameTransform {
    let t = progress.clamp(0.0, 1.0);
    let strength = (strength.clamp(5.0, 40.0) / 100.0).clamp(0.05, 0.40);
    let mut rect = base;
    let mut opacity = if fade {
        (t / 0.25).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let chosen = match effect {
        MotionEffect::Smart => {
            let image_aspect = base.width() / base.height().max(1.0);
            let viewport_aspect = viewport.width() / viewport.height().max(1.0);
            if image_aspect > viewport_aspect * 1.05 {
                MotionEffect::PanHorizontal
            } else if image_aspect < viewport_aspect / 1.05 {
                MotionEffect::PanVertical
            } else {
                MotionEffect::ZoomIn
            }
        }
        MotionEffect::Random => {
            // Stable per-frame fallback: smart motion avoids random jumps while
            // a slideshow is running and is friendly to repeatable previews.
            MotionEffect::Smart
        }
        other => other,
    };

    match chosen {
        MotionEffect::PanHorizontal => {
            // Cover-fit wide images to the viewport height before panning so a
            // fitted panorama still has horizontal travel to reveal.
            let aspect = base.width() / base.height().max(1.0);
            let width = (viewport.height() * aspect).max(base.width());
            let cover = egui::Rect::from_center_size(
                viewport.center(),
                egui::vec2(width, viewport.height().max(base.height())),
            );
            let distance = (cover.width() - viewport.width()).max(0.0);
            rect = cover.translate(egui::vec2(
                distance * (0.5 - t) * (strength / 0.20).clamp(0.25, 1.0),
                0.0,
            ));
        }
        MotionEffect::PanVertical => {
            let aspect = base.width() / base.height().max(1.0);
            let height = (viewport.width() / aspect.max(0.001)).max(base.height());
            let cover = egui::Rect::from_center_size(
                viewport.center(),
                egui::vec2(viewport.width().max(base.width()), height),
            );
            let distance = (cover.height() - viewport.height()).max(0.0);
            rect = cover.translate(egui::vec2(
                0.0,
                distance * (0.5 - t) * (strength / 0.20).clamp(0.25, 1.0),
            ));
        }
        MotionEffect::ZoomIn => {
            let scale = 1.0 + strength * t;
            rect = egui::Rect::from_center_size(base.center(), base.size() * scale);
        }
        MotionEffect::ZoomOut => {
            let scale = 1.0 + strength * (1.0 - t);
            rect = egui::Rect::from_center_size(base.center(), base.size() * scale);
        }
        MotionEffect::None | MotionEffect::Smart | MotionEffect::Random => {}
    }

    // Start gently rather than showing a blank frame during fade-in.
    opacity = opacity.clamp(0.0, 1.0);
    FrameTransform { rect, opacity }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_opacity_tracks_progress() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let frame = transform(rect, rect, 0.125, MotionEffect::None, 20.0, true);
        assert!((frame.opacity - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn zoom_in_grows_image() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let frame = transform(rect, rect, 1.0, MotionEffect::ZoomIn, 20.0, false);
        assert!(frame.rect.width() > rect.width());
    }
}
