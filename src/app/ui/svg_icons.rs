//! Theme-tinted SVG assets for the application's custom UI icons.
//!
//! eframe/egui does not install an SVG loader by default. This small renderer
//! uses the `resvg` dependency already used by the image decoder, so the icon
//! assets stay as editable SVG files without adding another runtime dependency.

use eframe::egui;
use resvg::{tiny_skia, usvg};

// Rasterize well above normal UI icon size, then downsample through egui's
// linear texture sampler. This produces cleaner diagonals at fractional UI scales.
const ICON_RASTER_SIZE: u32 = 192;

#[derive(Clone, Copy)]
pub(crate) enum SvgIcon {
    AppLogo,
    Hamburger,
    NavigationPrevious,
    NavigationNext,
    ZoomIn,
    ZoomOut,
    OneToOne,
    Fit,
    CaptionMinimize,
    CaptionMaximize,
    CaptionRestore,
    CaptionClose,
    Eye,
    Undo,
    Redo,
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
    Crop,
    Resize,
    Brightness,
    Contrast,
    Saturation,
    Hue,
    Fullscreen,
    Pin,
    Unpin,
    SidebarShow,
    SidebarHide,
    Masonry,
    Home,
    ContextShare,
    ContextLocate,
    ContextQueue,
    ContextReorder,
    ContextArchive,
    ContextCopyName,
    ContextNewTab,
    ContextCompare,
    ContextCopy,
    ContextMove,
    ContextDelete,
    ContextEdit,
    ContextOpenWith,
    ContextRename,
    ContextCopyPath,
    FavoriteStarOutline,
    FavoriteStarFilled,
    FavoriteHeartOutline,
    FavoriteHeartFilled,
    OpenFile,
    OpenFolder,
    FolderTree,
    Wrap,
    Inspect,
    Raw,
    Settings,
    Help,
    Exit,
    Slideshow,
    Sleep,
    Interval,
    Loop,
    Transition,
    Motion,
    Strength,
    Palette,
    Layout,
    General,
    Viewing,
    WindowFrame,
    Cache,
    Performance,
    VirtualTexture,
    Reset,
    Updates,
    SettingsFile,
    Interface,
    Input,
    MasonryFolder,
    HomeStart,
    Image,
    Startup,
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
            Self::OneToOne => ("one-to-one", include_bytes!("../../../assets/icons/ui/one-to-one.svg")),
            Self::Fit => ("fit", include_bytes!("../../../assets/icons/ui/fit.svg")),
            Self::CaptionMinimize => ("caption-minimize", include_bytes!("../../../assets/icons/ui/caption-minimize.svg")),
            Self::CaptionMaximize => ("caption-maximize", include_bytes!("../../../assets/icons/ui/caption-maximize.svg")),
            Self::CaptionRestore => ("caption-restore", include_bytes!("../../../assets/icons/ui/caption-restore.svg")),
            Self::CaptionClose => ("caption-close", include_bytes!("../../../assets/icons/ui/caption-close.svg")),
            Self::Eye => ("eye", include_bytes!("../../../assets/icons/ui/eye.svg")),
            Self::Undo => ("undo", include_bytes!("../../../assets/icons/ui/undo.svg")),
            Self::Redo => ("redo", include_bytes!("../../../assets/icons/ui/redo.svg")),
            Self::RotateLeft => ("rotate-left", include_bytes!("../../../assets/icons/ui/rotate-left.svg")),
            Self::RotateRight => ("rotate-right", include_bytes!("../../../assets/icons/ui/rotate-right.svg")),
            Self::FlipHorizontal => ("flip-horizontal", include_bytes!("../../../assets/icons/ui/flip-horizontal.svg")),
            Self::FlipVertical => ("flip-vertical", include_bytes!("../../../assets/icons/ui/flip-vertical.svg")),
            Self::Crop => ("crop", include_bytes!("../../../assets/icons/ui/crop.svg")),
            Self::Resize => ("resize", include_bytes!("../../../assets/icons/ui/resize.svg")),
            Self::Brightness => ("brightness", include_bytes!("../../../assets/icons/ui/brightness.svg")),
            Self::Contrast => ("contrast", include_bytes!("../../../assets/icons/ui/contrast.svg")),
            Self::Saturation => ("saturation", include_bytes!("../../../assets/icons/ui/saturation.svg")),
            Self::Hue => ("hue", include_bytes!("../../../assets/icons/ui/hue.svg")),
            Self::Fullscreen => ("fullscreen", include_bytes!("../../../assets/icons/ui/fullscreen.svg")),
            Self::Pin => ("pin", include_bytes!("../../../assets/icons/ui/pin.svg")),
            Self::Unpin => ("unpin", include_bytes!("../../../assets/icons/ui/unpin.svg")),
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
            Self::ContextNewTab => ("context-new-tab", include_bytes!("../../../assets/icons/ui/context-new-tab.svg")),
            Self::ContextCompare => ("context-compare", include_bytes!("../../../assets/icons/ui/context-compare.svg")),
            Self::ContextCopy => ("context-copy", include_bytes!("../../../assets/icons/ui/context-copy.svg")),
            Self::ContextMove => ("context-move", include_bytes!("../../../assets/icons/ui/context-move.svg")),
            Self::ContextDelete => ("context-delete", include_bytes!("../../../assets/icons/ui/context-delete.svg")),
            Self::ContextEdit => ("context-edit", include_bytes!("../../../assets/icons/ui/context-edit.svg")),
            Self::ContextOpenWith => ("context-open-with", include_bytes!("../../../assets/icons/ui/context-open-with.svg")),
            Self::ContextRename => ("context-rename", include_bytes!("../../../assets/icons/ui/context-rename.svg")),
            Self::ContextCopyPath => ("context-copy-path", include_bytes!("../../../assets/icons/ui/context-copy-path.svg")),
            Self::FavoriteStarOutline => ("favorite-star-outline", include_bytes!("../../../assets/icons/ui/favorite-star-outline.svg")),
            Self::FavoriteStarFilled => ("favorite-star-filled", include_bytes!("../../../assets/icons/ui/favorite-star-filled.svg")),
            Self::FavoriteHeartOutline => ("favorite-heart-outline", include_bytes!("../../../assets/icons/ui/favorite-heart-outline.svg")),
            Self::FavoriteHeartFilled => ("favorite-heart-filled", include_bytes!("../../../assets/icons/ui/favorite-heart-filled.svg")),
            Self::OpenFile => ("open-file", include_bytes!("../../../assets/icons/ui/open-file.svg")),
            Self::OpenFolder => ("open-folder", include_bytes!("../../../assets/icons/ui/open-folder.svg")),
            Self::FolderTree => ("folder-tree", include_bytes!("../../../assets/icons/ui/folder-tree.svg")),
            Self::Wrap => ("wrap", include_bytes!("../../../assets/icons/ui/wrap.svg")),
            Self::Inspect => ("inspect", include_bytes!("../../../assets/icons/ui/inspect.svg")),
            Self::Raw => ("raw", include_bytes!("../../../assets/icons/ui/raw.svg")),
            Self::Settings => ("settings", include_bytes!("../../../assets/icons/ui/settings.svg")),
            Self::Help => ("help", include_bytes!("../../../assets/icons/ui/help.svg")),
            Self::Exit => ("exit", include_bytes!("../../../assets/icons/ui/exit.svg")),
            Self::Slideshow => ("slideshow", include_bytes!("../../../assets/icons/ui/slideshow.svg")),
            Self::Sleep => ("sleep", include_bytes!("../../../assets/icons/ui/sleep.svg")),
            Self::Interval => ("interval", include_bytes!("../../../assets/icons/ui/interval.svg")),
            Self::Loop => ("loop", include_bytes!("../../../assets/icons/ui/loop.svg")),
            Self::Transition => ("transition", include_bytes!("../../../assets/icons/ui/transition.svg")),
            Self::Motion => ("motion", include_bytes!("../../../assets/icons/ui/motion.svg")),
            Self::Strength => ("strength", include_bytes!("../../../assets/icons/ui/strength.svg")),
            Self::Palette => ("palette", include_bytes!("../../../assets/icons/ui/palette.svg")),
            Self::Layout => ("layout", include_bytes!("../../../assets/icons/ui/layout.svg")),
            Self::General => ("general", include_bytes!("../../../assets/icons/ui/general.svg")),
            Self::Viewing => ("viewing", include_bytes!("../../../assets/icons/ui/viewing.svg")),
            Self::WindowFrame => ("window-frame", include_bytes!("../../../assets/icons/ui/window-frame.svg")),
            Self::Cache => ("cache", include_bytes!("../../../assets/icons/ui/cache.svg")),
            Self::Performance => ("performance", include_bytes!("../../../assets/icons/ui/performance.svg")),
            Self::VirtualTexture => ("virtual-texture", include_bytes!("../../../assets/icons/ui/virtual-texture.svg")),
            Self::Reset => ("reset", include_bytes!("../../../assets/icons/ui/reset.svg")),
            Self::Updates => ("updates", include_bytes!("../../../assets/icons/ui/updates.svg")),
            Self::SettingsFile => ("settings-file", include_bytes!("../../../assets/icons/ui/settings-file.svg")),
            Self::Interface => ("interface", include_bytes!("../../../assets/icons/ui/interface.svg")),
            Self::Input => ("input", include_bytes!("../../../assets/icons/ui/input.svg")),
            Self::MasonryFolder => ("masonry-folder", include_bytes!("../../../assets/icons/ui/masonry-folder.svg")),
            Self::HomeStart => ("home-start", include_bytes!("../../../assets/icons/ui/home-start.svg")),
            Self::Image => ("image", include_bytes!("../../../assets/icons/ui/image.svg")),
            Self::Startup => ("startup", include_bytes!("../../../assets/icons/ui/startup.svg")),
        }
    }
}


/// Select the configured favorite mark while keeping added/removed states distinct.
pub(crate) fn favorite_svg_icon(style: &str, filled: bool) -> SvgIcon {
    match (style, filled) {
        ("heart", false) => SvgIcon::FavoriteHeartOutline,
        ("heart", true) => SvgIcon::FavoriteHeartFilled,
        (_, false) => SvgIcon::FavoriteStarOutline,
        (_, true) => SvgIcon::FavoriteStarFilled,
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
            SvgIcon::OneToOne,
            SvgIcon::Fit,
            SvgIcon::CaptionMinimize,
            SvgIcon::CaptionMaximize,
            SvgIcon::CaptionRestore,
            SvgIcon::CaptionClose,
            SvgIcon::Eye,
            SvgIcon::Undo,
            SvgIcon::Redo,
            SvgIcon::RotateLeft,
            SvgIcon::RotateRight,
            SvgIcon::FlipHorizontal,
            SvgIcon::FlipVertical,
            SvgIcon::Crop,
            SvgIcon::Resize,
            SvgIcon::Brightness,
            SvgIcon::Contrast,
            SvgIcon::Saturation,
            SvgIcon::Hue,
            SvgIcon::Fullscreen,
            SvgIcon::Pin,
            SvgIcon::Unpin,
            SvgIcon::SidebarShow,
            SvgIcon::SidebarHide,
            SvgIcon::Masonry,
            SvgIcon::Home,
            SvgIcon::ContextShare,
            SvgIcon::ContextLocate,
            SvgIcon::ContextQueue,
            SvgIcon::ContextReorder,
            SvgIcon::ContextArchive,
            SvgIcon::ContextCopyName,
            SvgIcon::ContextNewTab, SvgIcon::ContextCompare, SvgIcon::ContextCopy,
            SvgIcon::ContextMove, SvgIcon::ContextDelete, SvgIcon::ContextEdit,
            SvgIcon::ContextOpenWith, SvgIcon::ContextRename, SvgIcon::ContextCopyPath,
            SvgIcon::FavoriteStarOutline, SvgIcon::FavoriteStarFilled,
            SvgIcon::FavoriteHeartOutline, SvgIcon::FavoriteHeartFilled,
            SvgIcon::OpenFile, SvgIcon::OpenFolder, SvgIcon::FolderTree, SvgIcon::Wrap,
            SvgIcon::Inspect, SvgIcon::Raw, SvgIcon::Settings, SvgIcon::Help, SvgIcon::Exit,
            SvgIcon::Slideshow, SvgIcon::Sleep, SvgIcon::Interval, SvgIcon::Loop,
            SvgIcon::Transition, SvgIcon::Motion, SvgIcon::Strength, SvgIcon::Palette,
            SvgIcon::Layout, SvgIcon::General, SvgIcon::Viewing, SvgIcon::WindowFrame,
            SvgIcon::Cache, SvgIcon::Performance, SvgIcon::VirtualTexture, SvgIcon::Reset,
            SvgIcon::Updates, SvgIcon::SettingsFile, SvgIcon::Interface, SvgIcon::Input,
            SvgIcon::MasonryFolder, SvgIcon::HomeStart, SvgIcon::Image, SvgIcon::Startup,
        ];

        for icon in icons {
            let (name, bytes) = icon.source();
            assert!(rasterize_svg(bytes).is_some(), "SVG icon {name} did not rasterize");
        }
    }
}
