//! Theme-tinted SVG assets for the application's custom UI icons.
//!
//! eframe/egui does not install an SVG loader by default. This small renderer
//! uses the `resvg` dependency already used by the image decoder, so the icon
//! assets stay as editable SVG files without adding another runtime dependency.

use eframe::egui;
use resvg::{tiny_skia, usvg};

const ICON_RASTER_SIZE: u32 = 96;

#[derive(Clone, Copy)]
pub(crate) enum SvgIcon {
    AppLogo,
    Hamburger,
    NavigationPrevious,
    NavigationNext,
    ZoomIn,
    ZoomOut,
    SidebarShow,
    SidebarHide,
    Masonry,
    Home,
    ContextShare,
    ContextLocate,
    ContextQueue,
    ContextFavorite,
    ContextReorder,
    ContextArchive,
    ContextCopyName,
}

impl SvgIcon {
    fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::AppLogo => (
                "app-logo",
                include_bytes!("../../../assets/icons/ui/app-logo.svg"),
            ),
            Self::Hamburger => (
                "hamburger",
                include_bytes!("../../../assets/icons/ui/hamburger.svg"),
            ),
            Self::NavigationPrevious => (
                "navigation-previous",
                include_bytes!("../../../assets/icons/ui/navigation-previous.svg"),
            ),
            Self::NavigationNext => (
                "navigation-next",
                include_bytes!("../../../assets/icons/ui/navigation-next.svg"),
            ),
            Self::ZoomIn => ("zoom-in", include_bytes!("../../../assets/icons/ui/zoom-in.svg")),
            Self::ZoomOut => ("zoom-out", include_bytes!("../../../assets/icons/ui/zoom-out.svg")),
            Self::SidebarShow => (
                "sidebar-show",
                include_bytes!("../../../assets/icons/ui/sidebar-show.svg"),
            ),
            Self::SidebarHide => (
                "sidebar-hide",
                include_bytes!("../../../assets/icons/ui/sidebar-hide.svg"),
            ),
            Self::Masonry => ("masonry", include_bytes!("../../../assets/icons/ui/masonry.svg")),
            Self::Home => ("home", include_bytes!("../../../assets/icons/ui/home.svg")),
            Self::ContextShare => (
                "context-share",
                include_bytes!("../../../assets/icons/ui/context-share.svg"),
            ),
            Self::ContextLocate => (
                "context-locate",
                include_bytes!("../../../assets/icons/ui/context-locate.svg"),
            ),
            Self::ContextQueue => (
                "context-queue",
                include_bytes!("../../../assets/icons/ui/context-queue.svg"),
            ),
            Self::ContextFavorite => (
                "context-favorite",
                include_bytes!("../../../assets/icons/ui/context-favorite.svg"),
            ),
            Self::ContextReorder => (
                "context-reorder",
                include_bytes!("../../../assets/icons/ui/context-reorder.svg"),
            ),
            Self::ContextArchive => (
                "context-archive",
                include_bytes!("../../../assets/icons/ui/context-archive.svg"),
            ),
            Self::ContextCopyName => (
                "context-copy-name",
                include_bytes!("../../../assets/icons/ui/context-copy-name.svg"),
            ),
        }
    }
}

/// Paint an SVG icon into an existing rectangle without affecting layout.
/// The SVG is rasterized only once per egui context and the texture is reused.
pub(crate) fn paint_svg_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: SvgIcon,
    tint: egui::Color32,
) {
    let side = rect.width().min(rect.height());
    if !side.is_finite() || side <= 0.0 {
        return;
    }

    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
    let Some(texture) = icon_texture(ui.ctx(), icon) else {
        return;
    };

    egui::Image::from_texture((texture.id(), icon_rect.size()))
        .fit_to_exact_size(icon_rect.size())
        .tint(tint)
        .paint_at(ui, icon_rect);
}

fn icon_texture(ctx: &egui::Context, icon: SvgIcon) -> Option<egui::TextureHandle> {
    let (name, svg_bytes) = icon.source();
    let cache_id = egui::Id::unique(("jpeg_viewer_svg_icon", name));

    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(cache_id)) {
        return Some(texture);
    }

    let image = rasterize_svg(svg_bytes)?;
    let texture = ctx.load_texture(
        format!("jpeg_viewer_svg_icon_{name}"),
        image,
        egui::TextureOptions::LINEAR,
    );

    ctx.data_mut(|data| data.insert_temp(cache_id, texture.clone()));

    Some(texture)
}

fn rasterize_svg(bytes: &[u8]) -> Option<egui::ColorImage> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let intrinsic_size = tree.size();
    if intrinsic_size.width() <= 0.0 || intrinsic_size.height() <= 0.0 {
        return None;
    }

    let mut pixmap = tiny_skia::Pixmap::new(ICON_RASTER_SIZE, ICON_RASTER_SIZE)?;
    let scale_x = ICON_RASTER_SIZE as f32 / intrinsic_size.width();
    let scale_y = ICON_RASTER_SIZE as f32 / intrinsic_size.height();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied RGBA. egui's unmultiplied constructor
    // expects straight-alpha bytes, so restore the color channels before upload.
    let mut rgba = pixmap.data().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        if alpha == 0 {
            pixel[..3].fill(0);
            continue;
        }

        for channel in &mut pixel[..3] {
            *channel = (((*channel as u16 * 255) + alpha / 2) / alpha).min(255) as u8;
        }
    }

    Some(egui::ColorImage::from_rgba_unmultiplied(
        [ICON_RASTER_SIZE as usize, ICON_RASTER_SIZE as usize],
        &rgba,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_ui_icon_parses_and_rasterizes() {
        let icons = [
            SvgIcon::AppLogo,
            SvgIcon::Hamburger,
            SvgIcon::NavigationPrevious,
            SvgIcon::NavigationNext,
            SvgIcon::ZoomIn,
            SvgIcon::ZoomOut,
            SvgIcon::SidebarShow,
            SvgIcon::SidebarHide,
            SvgIcon::Masonry,
            SvgIcon::Home,
            SvgIcon::ContextShare,
            SvgIcon::ContextLocate,
            SvgIcon::ContextQueue,
            SvgIcon::ContextFavorite,
            SvgIcon::ContextReorder,
            SvgIcon::ContextArchive,
            SvgIcon::ContextCopyName,
        ];

        for icon in icons {
            let (name, bytes) = icon.source();
            assert!(rasterize_svg(bytes).is_some(), "SVG icon {name} did not rasterize");
        }
    }
}
