//! Settings menu UI

mod advanced;
mod copy_buttons;
pub mod general;
mod look;
mod settings_file;
mod slideshow;
mod updates;
pub(crate) mod viewing;

use crate::app::types::ViewerApp;
use crate::app::ui::tricks;
use eframe::egui;


fn settings_search_pending_id() -> egui::Id {
    static ID: std::sync::OnceLock<egui::Id> = std::sync::OnceLock::new();
    *ID.get_or_init(|| egui::Id::unique("jpeg_viewer_settings_search_pending"))
}

fn settings_search_highlight_id() -> egui::Id {
    static ID: std::sync::OnceLock<egui::Id> = std::sync::OnceLock::new();
    *ID.get_or_init(|| egui::Id::unique("jpeg_viewer_settings_search_highlight"))
}

fn settings_search_highlight_until_id() -> egui::Id {
    static ID: std::sync::OnceLock<egui::Id> = std::sync::OnceLock::new();
    *ID.get_or_init(|| egui::Id::unique("jpeg_viewer_settings_search_highlight_until"))
}

fn settings_search_pending_target(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<String>(settings_search_pending_id()))
}

fn set_settings_search_target(ctx: &egui::Context, target: &'static str) {
    ctx.data_mut(|data| data.insert_temp(settings_search_pending_id(), target.to_owned()));
    ctx.request_repaint();
}

/// Draw a settings group with a temporary highlight and bring it into view
/// after a search result is selected. The pending target is consumed exactly
/// once, so subsequent user scrolling is never fought by repeated auto-scrolls.
pub(crate) fn settings_search_anchor(
    ui: &mut egui::Ui,
    target: &'static str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let ctx = ui.ctx().clone();
    let pending = settings_search_pending_target(&ctx);
    let active = ctx.data(|data| data.get_temp::<String>(settings_search_highlight_id()));
    let highlight_until = ctx
        .data(|data| data.get_temp::<std::time::Instant>(settings_search_highlight_until_id()));
    let now = std::time::Instant::now();
    let is_pending = pending.as_deref() == Some(target);
    let is_highlighted = is_pending
        || (active.as_deref() == Some(target) && highlight_until.is_some_and(|until| until > now));

    let response = if is_highlighted {
        egui::Frame::new()
            .fill(crate::ui_theme::with_alpha(ui.visuals().selection.bg_fill, 48))
            .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color))
            .corner_radius(egui::CornerRadius::same(7))
            .inner_margin(egui::Margin::same(0))
            .show(ui, contents)
            .response
    } else {
        egui::Frame::new()
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE)
            .inner_margin(egui::Margin::same(0))
            .show(ui, contents)
            .response
    };

    if is_pending {
        ui.scroll_to_rect(response.rect.expand(10.0), Some(egui::Align::Center));
        ctx.data_mut(|data| {
            data.remove::<String>(settings_search_pending_id());
            data.insert_temp(settings_search_highlight_id(), target.to_owned());
            data.insert_temp(
                settings_search_highlight_until_id(),
                now + std::time::Duration::from_secs(2),
            );
        });
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
    } else if active.as_deref() == Some(target)
        && highlight_until.is_some_and(|until| until <= now)
    {
        ctx.data_mut(|data| {
            data.remove::<String>(settings_search_highlight_id());
            data.remove::<std::time::Instant>(settings_search_highlight_until_id());
        });
    }
}

/// Collapsible settings section with a consistent SVG icon and hover state.
pub(crate) fn section(
    ui: &mut egui::Ui,
    id: &'static str,
    icon: crate::app::ui::SvgIcon,
    title: &str,
    default_open: bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let section_id = egui::Id::unique(("jpeg_viewer_settings_section", id));
    let search_targeted = settings_search_pending_target(ui.ctx()).as_deref() == Some(id);
    let mut open = ui.ctx().data(|data| data.get_temp::<bool>(section_id)).unwrap_or(default_open);
    if search_targeted {
        open = true;
        ui.ctx().data_mut(|data| data.insert_temp(section_id, true));
    }
    let width = ui.available_width().max(1.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    if response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else {
            &ui.visuals().widgets.hovered
        };
        ui.painter().rect_filled(rect, egui::CornerRadius::same(5), visuals.bg_fill);
    }
    let center_y = rect.center().y;
    let chevron = if open { "▾" } else { "▸" };
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, center_y),
        egui::Align2::CENTER_CENTER,
        chevron,
        egui::FontId::proportional(14.0),
        ui.visuals().weak_text_color(),
    );
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 31.0, center_y),
        egui::vec2(16.0, 16.0),
    );
    crate::app::ui::paint_svg_icon(ui, icon_rect, icon, ui.visuals().text_color());
    ui.painter().text(
        egui::pos2(rect.left() + 46.0, center_y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(14.0),
        ui.visuals().text_color(),
    );
    if response.clicked() {
        open = !open;
        ui.ctx().data_mut(|data| data.insert_temp(section_id, open));
    }
    if open {
        ui.add_space(2.0);
        ui.indent(section_id, |contents_ui| {
            settings_search_anchor(contents_ui, id, contents);
        });
        ui.add_space(3.0);
    }
}

/// Compact icon-and-label heading used for grouped options inside a section.
pub(crate) fn subsection_heading(
    ui: &mut egui::Ui,
    icon: crate::app::ui::SvgIcon,
    title: &str,
) {
    let draw_heading = |ui: &mut egui::Ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            crate::app::ui::paint_svg_icon(ui, rect, icon, ui.visuals().text_color());
            ui.label(egui::RichText::new(title).strong());
        });
    };
    draw_heading(ui);
}

/// Navigation destinations shown in the dedicated Settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum SettingsPage {
    Look = 0,
    General = 1,
    Viewing = 2,
    Slideshow = 3,
    Tricks = 4,
    Advanced = 5,
    Updates = 6,
    SettingsFile = 7,
}

impl SettingsPage {
    const ALL: [Self; 8] = [
        Self::Look,
        Self::General,
        Self::Viewing,
        Self::Slideshow,
        Self::Tricks,
        Self::Advanced,
        Self::Updates,
        Self::SettingsFile,
    ];

    fn from_index(index: u8) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::Look)
    }

    fn index(self) -> u8 {
        self as u8
    }

    fn label(self) -> &'static str {
        match self {
            Self::Look => "Look",
            Self::General => "General",
            Self::Viewing => "Viewing",
            Self::Slideshow => "Slideshow",
            Self::Tricks => "Tricks",
            Self::Advanced => "Advanced",
            Self::Updates => "Updates",
            Self::SettingsFile => "Settings file",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Look => "Appearance, layout, navigation, and window presentation.",
            Self::General => "Everyday behavior, input, browsing, and startup preferences.",
            Self::Viewing => "Image fit, zoom, padding, and window sizing.",
            Self::Slideshow => "Playback, timing, transitions, and system sleep behavior.",
            Self::Tricks => "Optional visual effects and decorative features.",
            Self::Advanced => "Performance, caching, frame limits, and low-level options.",
            Self::Updates => "Release checks and application updates.",
            Self::SettingsFile => "Configuration file location and diagnostics.",
        }
    }

    fn icon(self) -> crate::app::ui::SvgIcon {
        match self {
            Self::Look => crate::app::ui::SvgIcon::Palette,
            Self::General => crate::app::ui::SvgIcon::General,
            Self::Viewing => crate::app::ui::SvgIcon::Viewing,
            Self::Slideshow => crate::app::ui::SvgIcon::Slideshow,
            Self::Tricks => crate::app::ui::SvgIcon::Image,
            Self::Advanced => crate::app::ui::SvgIcon::Performance,
            Self::Updates => crate::app::ui::SvgIcon::Updates,
            Self::SettingsFile => crate::app::ui::SvgIcon::SettingsFile,
        }
    }
}

#[derive(Clone, Copy)]
struct SettingsSearchEntry {
    label: &'static str,
    page: SettingsPage,
    keywords: &'static str,
}

/// Human-readable search aliases for settings. Clicking a result opens the
/// relevant page, scrolls the matching settings group into view, and briefly
/// highlights it. This index stays separate from the INI storage schema.
const SETTINGS_SEARCH_ENTRIES: &[SettingsSearchEntry] = &[
    SettingsSearchEntry { label: "Look profile", page: SettingsPage::Look, keywords: "appearance theme desktop COSMIC GNOME Adwaita Windows 98 classic style" },
    SettingsSearchEntry { label: "Light or dark theme", page: SettingsPage::Look, keywords: "colors brightness dark mode light mode theme" },
    SettingsSearchEntry { label: "Accent color", page: SettingsPage::Look, keywords: "main color highlight selection tint custom color" },
    SettingsSearchEntry { label: "UI scale", page: SettingsPage::Look, keywords: "interface size scaling density controls" },
    SettingsSearchEntry { label: "Text scale", page: SettingsPage::Look, keywords: "font size typography readability" },
    SettingsSearchEntry { label: "Favorites icon", page: SettingsPage::Look, keywords: "star heart bookmark favorite symbol" },
    SettingsSearchEntry { label: "Toolbar and navigation placement", page: SettingsPage::Look, keywords: "layout top bottom center sidebar controls" },
    SettingsSearchEntry { label: "Desktop menu bar", page: SettingsPage::Look, keywords: "menu hamburger native titlebar COSMIC GNOME" },
    SettingsSearchEntry { label: "Navigation auto-hide", page: SettingsPage::Look, keywords: "toolbar hide show timeout fullscreen" },
    SettingsSearchEntry { label: "OS/DE blur or glass", page: SettingsPage::Look, keywords: "backdrop transparency blur background compositor" },
    SettingsSearchEntry { label: "Main view opacity", page: SettingsPage::Look, keywords: "transparency alpha glass background" },
    SettingsSearchEntry { label: "Native title bar and window frame", page: SettingsPage::Look, keywords: "decorations custom titlebar resize border thickness" },
    SettingsSearchEntry { label: "Workspace tabs in title bar", page: SettingsPage::Look, keywords: "tabs window title navigation" },
    SettingsSearchEntry { label: "Center window title", page: SettingsPage::Look, keywords: "caption title alignment" },
    SettingsSearchEntry { label: "Click title to rename", page: SettingsPage::Look, keywords: "rename file titlebar" },
    SettingsSearchEntry { label: "Compact interface", page: SettingsPage::General, keywords: "compact UI spacing controls density" },
    SettingsSearchEntry { label: "Invert scroll zoom", page: SettingsPage::General, keywords: "mouse wheel reverse direction zoom" },
    SettingsSearchEntry { label: "Window mouse shortcuts", page: SettingsPage::General, keywords: "mouse gestures drag middle right button Alt Shift move resize" },
    SettingsSearchEntry { label: "Open dropped folders recursively", page: SettingsPage::General, keywords: "drag drop subfolders recursive directory" },
    SettingsSearchEntry { label: "Show Masonry folder headers", page: SettingsPage::General, keywords: "folder groups gallery sections" },
    SettingsSearchEntry { label: "Recently opened rows", page: SettingsPage::General, keywords: "Home recent folders queues archives" },
    SettingsSearchEntry { label: "Image texture filtering", page: SettingsPage::General, keywords: "nearest linear filtering sharp smooth quality" },
    SettingsSearchEntry { label: "Startup fullscreen", page: SettingsPage::General, keywords: "launch start maximized full screen" },
    SettingsSearchEntry { label: "Start centered on screen", page: SettingsPage::General, keywords: "startup position monitor placement" },
    SettingsSearchEntry { label: "Restore previous session", page: SettingsPage::General, keywords: "startup reopen tabs folders state" },
    SettingsSearchEntry { label: "Single-instance mode", page: SettingsPage::General, keywords: "one process multiple opens file explorer" },
    SettingsSearchEntry { label: "Zoom sensitivity", page: SettingsPage::Viewing, keywords: "zoom step scroll mouse button speed" },
    SettingsSearchEntry { label: "Fit images to Window", page: SettingsPage::Viewing, keywords: "fit zoom enlarge upscale small images all images" },
    SettingsSearchEntry { label: "Fit only big images to Window", page: SettingsPage::Viewing, keywords: "fit shrink reduce large images small original size default" },
    SettingsSearchEntry { label: "Image padding", page: SettingsPage::Viewing, keywords: "space around image margin fit canvas" },
    SettingsSearchEntry { label: "Window sizing around image", page: SettingsPage::Viewing, keywords: "resize fit current photo automatic one-time" },
    SettingsSearchEntry { label: "Enable slideshow", page: SettingsPage::Slideshow, keywords: "play pause start stop presentation" },
    SettingsSearchEntry { label: "Prevent system sleep", page: SettingsPage::Slideshow, keywords: "keep awake screensaver idle suspend" },
    SettingsSearchEntry { label: "Slideshow interval", page: SettingsPage::Slideshow, keywords: "seconds timing delay duration" },
    SettingsSearchEntry { label: "Loop slideshow", page: SettingsPage::Slideshow, keywords: "repeat cycle end" },
    SettingsSearchEntry { label: "Slideshow transition", page: SettingsPage::Slideshow, keywords: "fade animation crossfade" },
    SettingsSearchEntry { label: "Slideshow motion", page: SettingsPage::Slideshow, keywords: "Ken Burns pan zoom smart random" },
    SettingsSearchEntry { label: "Motion strength", page: SettingsPage::Slideshow, keywords: "animation amount intensity" },
    SettingsSearchEntry { label: "Falling particles", page: SettingsPage::Tricks, keywords: "snow effect overlay speed density size color" },
    SettingsSearchEntry { label: "Watermark", page: SettingsPage::Tricks, keywords: "logo text image opacity size position bounce DVD" },
    SettingsSearchEntry { label: "Blurred photo backdrop", page: SettingsPage::Tricks, keywords: "background blur tint transparency radius" },
    SettingsSearchEntry { label: "Virtual texture", page: SettingsPage::Advanced, keywords: "large image tiled rendering texture size" },
    SettingsSearchEntry { label: "Image cache", page: SettingsPage::Advanced, keywords: "memory cache capacity cleanup performance" },
    SettingsSearchEntry { label: "Frame limiter", page: SettingsPage::Advanced, keywords: "FPS refresh rate repaint performance" },
    SettingsSearchEntry { label: "Thumbnail worker count", page: SettingsPage::Advanced, keywords: "threads workers CPU concurrency restart sidebar" },
    SettingsSearchEntry { label: "Reset all settings", page: SettingsPage::Advanced, keywords: "defaults restore factory reset preferences" },
    SettingsSearchEntry { label: "Check for updates", page: SettingsPage::Updates, keywords: "latest release version GitHub manual automatic" },
    SettingsSearchEntry { label: "Install application update", page: SettingsPage::Updates, keywords: "download upgrade release stable" },
    SettingsSearchEntry { label: "Automatic update checks", page: SettingsPage::Updates, keywords: "weekly background check new version" },
    SettingsSearchEntry { label: "Settings file location", page: SettingsPage::SettingsFile, keywords: "INI configuration path file location" },
    SettingsSearchEntry { label: "Open settings folder", page: SettingsPage::SettingsFile, keywords: "file manager config directory" },
    SettingsSearchEntry { label: "Settings file status", page: SettingsPage::SettingsFile, keywords: "loaded successfully error diagnostics" },
    SettingsSearchEntry { label: "Main color and custom accent", page: SettingsPage::Look, keywords: "accent color palette custom picker teal blue purple orange rose green yellow red" },
    SettingsSearchEntry { label: "Main view background", page: SettingsPage::Look, keywords: "light dark follow theme custom canvas behind image" },
    SettingsSearchEntry { label: "Transparency checkerboard", page: SettingsPage::Look, keywords: "alpha transparent PNG background squares pattern" },
    SettingsSearchEntry { label: "Checkerboard size and brightness", page: SettingsPage::Look, keywords: "transparency alpha pattern grid size shade" },
    SettingsSearchEntry { label: "Windows 98 dial-up image reveal", page: SettingsPage::Look, keywords: "slow download connection kbps strip reveal retro" },
    SettingsSearchEntry { label: "Custom title bar color", page: SettingsPage::Look, keywords: "caption system main color background" },
    SettingsSearchEntry { label: "Integrate navigation into title bar", page: SettingsPage::Look, keywords: "sidebar menu previous next zoom controls" },
    SettingsSearchEntry { label: "OS-managed rounded corners", page: SettingsPage::Look, keywords: "window shape border radius native corner rounding" },
    SettingsSearchEntry { label: "Padding mode", page: SettingsPage::Viewing, keywords: "look defaults fine tune custom image margins" },
    SettingsSearchEntry { label: "Top right bottom left padding", page: SettingsPage::Viewing, keywords: "individual side spacing image viewport margin" },
    SettingsSearchEntry { label: "Automatically fit window to each image", page: SettingsPage::Viewing, keywords: "resize on image change automatic dimensions" },
    SettingsSearchEntry { label: "Default monitor for window sizing", page: SettingsPage::Viewing, keywords: "display screen monitor selection" },
    SettingsSearchEntry { label: "Window minimum and maximum size", page: SettingsPage::Viewing, keywords: "width height constraints logical points" },
    SettingsSearchEntry { label: "Monitor edge padding", page: SettingsPage::Viewing, keywords: "screen bounds work area gap margin" },
    SettingsSearchEntry { label: "Keep window centered after resizing", page: SettingsPage::Viewing, keywords: "position center resize fit" },
    SettingsSearchEntry { label: "Mouse wheel zoom sensitivity", page: SettingsPage::Viewing, keywords: "wheel scroll zoom step" },
    SettingsSearchEntry { label: "Right mouse button zoom sensitivity", page: SettingsPage::Viewing, keywords: "right click drag zoom step" },
    SettingsSearchEntry { label: "Thumbnail worker multiplier", page: SettingsPage::Advanced, keywords: "thumbnail threads dimension workers CPU restart" },
    SettingsSearchEntry { label: "Virtual texture tile size", page: SettingsPage::Advanced, keywords: "large image texture tiles chunks memory" },
    SettingsSearchEntry { label: "Virtual texture threshold", page: SettingsPage::Advanced, keywords: "large image pixel limit activate tiled rendering" },
    SettingsSearchEntry { label: "Cache radius", page: SettingsPage::Advanced, keywords: "preload neighboring images memory cache" },
    SettingsSearchEntry { label: "Cache delta factor", page: SettingsPage::Advanced, keywords: "preload radius multiplier" },
    SettingsSearchEntry { label: "Maximum cache tasks", page: SettingsPage::Advanced, keywords: "concurrent preload jobs workers" },
    SettingsSearchEntry { label: "Preload throttle", page: SettingsPage::Advanced, keywords: "delay milliseconds cache decoding rate" },
    SettingsSearchEntry { label: "Publish generated thumbnails to desktop cache", page: SettingsPage::Advanced, keywords: "Linux XDG shared thumbnails OS file manager" },
    SettingsSearchEntry { label: "Maximum FPS", page: SettingsPage::Advanced, keywords: "frame rate limit rendering refresh" },
    SettingsSearchEntry { label: "Idle FPS", page: SettingsPage::Advanced, keywords: "frame rate inactive quiet" },
    SettingsSearchEntry { label: "Unfocused FPS and timeout", page: SettingsPage::Advanced, keywords: "background inactive window performance sleep" },
    SettingsSearchEntry { label: "Falling particle color, size, speed, and density", page: SettingsPage::Tricks, keywords: "snow particle overlay configure" },
    SettingsSearchEntry { label: "Watermark text, logo, and position", page: SettingsPage::Tricks, keywords: "image stamp text overlay custom logo placement" },
    SettingsSearchEntry { label: "Watermark opacity and size", page: SettingsPage::Tricks, keywords: "transparency scale image overlay" },
    SettingsSearchEntry { label: "DVD watermark bounce speed", page: SettingsPage::Tricks, keywords: "bouncing logo movement speed" },
    SettingsSearchEntry { label: "Blur radius and tint", page: SettingsPage::Tricks, keywords: "photo backdrop blur effect main color transparency" },
    SettingsSearchEntry { label: "Maximum frame rate and idle timeout", page: SettingsPage::Advanced, keywords: "FPS idle rendering delay unfocused" },
];

fn settings_search_target(entry: &SettingsSearchEntry) -> &'static str {
    match entry.label {
        // Look.
        "Toolbar and navigation placement" | "Desktop menu bar" | "Navigation auto-hide"
        | "Workspace tabs in title bar" | "Center window title" | "Click title to rename"
        | "Integrate navigation into title bar" => "look_layout",
        "OS/DE blur or glass" | "Main view opacity" | "Native title bar and window frame"
        | "Custom title bar color" | "OS-managed rounded corners" => "look_window",

        // General.
        "Compact interface" => "general_interface",
        "Invert scroll zoom" | "Window mouse shortcuts" | "Open dropped folders recursively" => "general_input",
        "Show Masonry folder headers" => "general_masonry",
        "Recently opened rows" => "general_home",
        "Image texture filtering" => "general_image_rendering",
        "Startup fullscreen" | "Start centered on screen" | "Restore previous session"
        | "Single-instance mode" => "general_startup",

        // Viewing.
        "Fit images to Window" | "Fit only big images to Window" => "viewing_fit_mode",
        "Zoom sensitivity" | "Mouse wheel zoom sensitivity"
        | "Right mouse button zoom sensitivity" => "viewing_zoom_sensitivity",
        "Image padding" | "Padding mode" | "Top right bottom left padding" => "viewing_padding",
        "Window sizing around image" | "Automatically fit window to each image"
        | "Default monitor for window sizing" | "Window minimum and maximum size"
        | "Monitor edge padding" | "Keep window centered after resizing" => "viewing_window_sizing",

        // Slideshow.
        "Prevent system sleep" => "slideshow_system_sleep",
        "Enable slideshow" | "Slideshow interval" | "Loop slideshow"
        | "Slideshow transition" | "Slideshow motion" | "Motion strength" => "slideshow_playback",

        // Tricks.
        "Falling particles" | "Falling particle color, size, speed, and density" => "tricks_snow",
        "Watermark" | "Watermark text, logo, and position" | "Watermark opacity and size"
        | "DVD watermark bounce speed" => "tricks_watermark",
        "Blurred photo backdrop" | "Blur radius and tint" => "tricks_backdrop",

        // Advanced.
        "Virtual texture" | "Virtual texture tile size" | "Virtual texture threshold" => "advanced_virtual_texture",
        "Image cache" | "Cache radius" | "Cache delta factor" | "Maximum cache tasks"
        | "Preload throttle" | "Publish generated thumbnails to desktop cache" => "advanced_cache",
        "Frame limiter" | "Maximum FPS" | "Idle FPS" | "Unfocused FPS and timeout"
        | "Maximum frame rate and idle timeout" => "advanced_frame_limiter",
        "Thumbnail worker count" | "Thumbnail worker multiplier" => "advanced_thumbnail_workers",
        "Reset all settings" => "advanced_reset_all",
        "Check for updates" | "Install application update" | "Automatic update checks" => "page_updates",
        "Settings file location" | "Open settings folder" | "Settings file status" => "page_settings_file",

        // All remaining Look entries are in Appearance.
        _ if entry.page == SettingsPage::Look => "look_appearance",
        _ => "page_top",
    }
}

fn settings_search_matches(entry: &SettingsSearchEntry, query: &str) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        entry.label,
        entry.page.label(),
        entry.page.description(),
        entry.keywords,
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

/// Start or focus the single Settings viewport. Existing settings controls still
/// mutate the shared in-memory settings so supported options preview immediately;
/// autosaving is suspended until the user explicitly presses Save.
pub fn toggle_settings_menu(app: &mut ViewerApp, ctx: &egui::Context) {
    if app.show_settings_menu {
        let id = settings_viewport_id(app.settings_window_generation);
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
        ctx.request_repaint();
        return;
    }

    app.settings_window_generation = app.settings_window_generation.wrapping_add(1).max(1);
    app.settings_window_initialized = false;
    app.settings_session_snapshot = Some(app.settings_manager.get().clone());
    app.settings_session_was_dirty = app.settings_manager.is_dirty();
    let (fullscreen, maximized) = ctx.input_for(egui::ViewportId::ROOT, |input| {
        (
            input.viewport().fullscreen.unwrap_or(false),
            input.viewport().maximized.unwrap_or(false),
        )
    });
    app.settings_runtime_snapshot = Some(super::types::SettingsRuntimeSnapshot {
        fullscreen,
        maximized,
        always_on_top: app.window_always_on_top,
        slideshow_enabled: app.slideshow_enabled,
        slideshow_settings_loaded: app.slideshow_settings_loaded,
        slideshow_has_advanced: app.slideshow_has_advanced,
        startup_restore_maximize_pending: app.startup_restore_maximize_pending,
        startup_fullscreen_handled: app.startup_fullscreen_handled,
        startup_window_placement_handled: app.startup_window_placement_handled,
        preserve_reset_window_geometry: app.preserve_reset_window_geometry,
    });
    app.settings_reset_all_previewed = false;
    app.settings_close_prompt = false;
    app.settings_save_error = None;
    app.show_settings_menu = true;
    app.show_edit_menu = false;
    app.crop_tool_active = false;
    app.crop_drag_start_image = None;
    ctx.request_repaint();
}

fn settings_viewport_id(generation: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("jpeg_viewer_settings_viewport", generation))
}

pub(crate) fn centered_on_main_monitor(
    ctx: &egui::Context,
    size: egui::Vec2,
) -> egui::Pos2 {
    let root_rect = ctx.input_for(egui::ViewportId::ROOT, |input| input.viewport().outer_rect);
    let monitors = crate::platform::monitors::available_monitors();
    let monitor = crate::platform::monitors::resolve_default_monitor_at(
        &monitors,
        crate::platform::monitors::CURRENT_WINDOW_MONITOR,
        root_rect,
    )
    .or_else(|| monitors.iter().find(|monitor| monitor.is_primary))
    .or_else(|| monitors.first());

    if let Some(monitor) = monitor {
        return monitor.bounds().center() - size * 0.5;
    }

    // Best-effort fallback when platform monitor enumeration is unavailable.
    root_rect
        .map(|rect| rect.center() - size * 0.5)
        .unwrap_or(egui::pos2(120.0, 120.0))
}

/// Register and render the independent native Settings viewport.
pub fn render_settings_viewport(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.show_settings_menu {
        return;
    }

    let viewport_id = settings_viewport_id(app.settings_window_generation);
    let size = egui::vec2(920.0, 720.0);
    let mut builder = egui::ViewportBuilder::default()
        .with_app_id("com.jpegviewer.JpegViewer")
        .with_icon(crate::application_window_icon_data())
        .with_title("Settings · JPEG Viewer")
        .with_min_inner_size([680.0, 500.0])
        .with_resizable(true)
        .with_decorations(true)
        // Utility windows must remain opaque even when the main viewer uses
        // the OS backdrop / transparent native surface.
        .with_transparent(false)
        .with_clamp_size_to_monitor_size(true);
    if !app.settings_window_initialized {
        // Geometry is initial-only. Reapplying a centered position every frame
        // would fight native dragging and snap the window back to the center.
        builder = builder
            .with_inner_size(size)
            .with_position(centered_on_main_monitor(ctx, size))
            .with_active(true);
        app.settings_window_initialized = true;
    }

    let mut native_close_without_changes = false;
    ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
        let viewport_ctx = ui.ctx().clone();
        let close_requested = viewport_ctx.input(|input| input.viewport().close_requested());
        if close_requested {
            if app.settings_have_unsaved_changes() {
                viewport_ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                app.settings_close_prompt = true;
            } else {
                native_close_without_changes = true;
                viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }

        // The root application window may use an alpha clear color for glass
        // and Pop-out support. Paint an opaque base first so this child viewport
        // never exposes the desktop background, regardless of the main window.
        let [r, g, b, _] = ui.visuals().panel_fill.to_srgba_unmultiplied();
        ui.painter().rect_filled(
            ui.max_rect(),
            egui::CornerRadius::ZERO,
            egui::Color32::from_rgb(r, g, b),
        );

        egui::Frame::central_panel(&viewport_ctx.global_style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                render_settings_window_contents(app, ui, &viewport_ctx);
            });

        if app.settings_close_prompt {
            render_settings_close_prompt(app, &viewport_ctx);
        }
    });

    if native_close_without_changes {
        app.finish_settings_window();
        ctx.request_repaint();
    }
}

impl ViewerApp {
    pub(crate) fn settings_have_unsaved_changes(&self) -> bool {
        // Reset All also previews runtime-only changes (for example stopping a
        // slideshow or leaving fullscreen), even when the saved values were
        // already equal to their defaults.
        if self.settings_reset_all_previewed {
            return true;
        }
        let Some(snapshot) = &self.settings_session_snapshot else {
            return false;
        };
        let current = self.settings_manager.get();
        let mut baseline = snapshot.clone();
        // These fields are maintained by the background updater rather than by
        // Settings controls; don't prompt users for updater bookkeeping changes.
        baseline.update_check_last_attempt_unix = current.update_check_last_attempt_unix;
        baseline.update_available_release_json = current.update_available_release_json.clone();
        &baseline != current
    }

    pub(crate) fn finish_settings_window(&mut self) {
        self.show_settings_menu = false;
        self.settings_window_initialized = false;
        self.settings_session_snapshot = None;
        self.settings_session_was_dirty = false;
        self.settings_runtime_snapshot = None;
        self.settings_reset_all_previewed = false;
        self.settings_close_prompt = false;
        self.settings_save_error = None;
    }

    pub(crate) fn discard_settings_session(&mut self, ctx: &egui::Context) {
        let runtime_snapshot = self.settings_runtime_snapshot.take();
        let reset_all_previewed = self.settings_reset_all_previewed;
        if let Some(mut snapshot) = self.settings_session_snapshot.take() {
            let preview_settings = self.settings_manager.get().clone();
            let updater_metadata_changed =
                snapshot.update_check_last_attempt_unix != preview_settings.update_check_last_attempt_unix
                    || snapshot.update_available_release_json != preview_settings.update_available_release_json;
            snapshot.update_check_last_attempt_unix = preview_settings.update_check_last_attempt_unix;
            snapshot.update_available_release_json = preview_settings.update_available_release_json.clone();
            self.settings_manager.restore_in_memory(
                snapshot,
                self.settings_session_was_dirty || updater_metadata_changed,
            );
            restore_preview_runtime(
                self,
                ctx,
                &preview_settings,
                runtime_snapshot,
                reset_all_previewed,
            );
        }
        self.finish_settings_window();
    }
}

fn restore_preview_runtime(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    preview_settings: &crate::settings::AppSettings,
    runtime_snapshot: Option<super::types::SettingsRuntimeSnapshot>,
    reset_all_previewed: bool,
) {
    let settings = app.settings_manager.get().clone();
    let image_reload_needed = preview_settings.texture_filter != settings.texture_filter
        || preview_settings.tile_size != settings.tile_size
        || preview_settings.virtual_texture_threshold != settings.virtual_texture_threshold;
    let image_fit_mode_changed = preview_settings.image_fit_mode != settings.image_fit_mode;

    app.load_frame_limiter_settings();
    if image_fit_mode_changed && !app.image_entries.is_empty() {
        app.b_fit_to_window = true;
    }
    app.b_ctrl_invert = settings.b_ctrl_invert;
    app.compactui = settings.compact_mode;
    app.update_cache_radius(settings.cache_radius.clamp(1, 100));
    app.cache_delta_factor = settings.cache_delta_factor;
    app.max_cache_task = settings.max_cache_task;
    app.slideshow_interval = std::time::Duration::from_millis(settings.slideshow_interval_ms);
    app.slideshow_loop = settings.slideshow_loop;
    app.slideshow_random = settings.slideshow_random;
    app.slideshow_transition = settings.slideshow_transition.clone();
    app.slideshow_motion = settings.slideshow_motion.clone();
    app.slideshow_motion_strength = settings.slideshow_motion_strength;

    ctx.set_theme(crate::ui_theme::effective_theme_preference(
        &settings.theme_preference,
        &settings.look_preference,
    ));
    crate::ui_theme::set_look_profile(ctx, &settings.look_preference);
    ctx.set_fonts(crate::ui_theme::font_definitions_for_profile(
        &settings.look_preference,
    ));
    crate::ui_theme::apply_polished_style_with_options(
        ctx,
        &settings.ui_accent_color,
        settings.ui_scale,
        settings.ui_text_scale,
    );
    crate::ui_theme::set_os_background_blur_preference(ctx, settings.os_background_blur);
    crate::ui_theme::set_disable_os_background_blur_in_fullscreen(
        ctx,
        settings.disable_os_background_blur_in_fullscreen,
    );
    crate::ui_theme::platform_backdrop::set_native_window_shape(settings.native_window_rounding);
    crate::app::ui::set_desktop_menu_bar_preference(ctx, &settings.desktop_menu_bar_preference);
    ctx.send_viewport_cmd_to(
        egui::ViewportId::ROOT,
        egui::ViewportCommand::Decorations(settings.show_titlebar),
    );
    if let Some(runtime_snapshot) = runtime_snapshot {
        // Reset All temporarily changes native window state, pinning, and
        // slideshow playback. Restore those runtime-only states on Discard.
        if reset_all_previewed {
            app.window_always_on_top = runtime_snapshot.always_on_top;
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::WindowLevel(if runtime_snapshot.always_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            );
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Fullscreen(runtime_snapshot.fullscreen),
            );
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Maximized(runtime_snapshot.maximized),
            );
            app.slideshow_enabled = runtime_snapshot.slideshow_enabled;
            app.slideshow_settings_loaded = runtime_snapshot.slideshow_settings_loaded;
            app.slideshow_has_advanced = runtime_snapshot.slideshow_has_advanced;
            app.slideshow_previous_texture = None;
            app.slideshow_previous_rect = None;
            app.slideshow_transition_started = None;
            app.slideshow_last_advance = std::time::Instant::now();
            app.power_inhibitor.set_active(runtime_snapshot.slideshow_enabled);
            app.startup_restore_maximize_pending =
                runtime_snapshot.startup_restore_maximize_pending;
            app.startup_fullscreen_handled = runtime_snapshot.startup_fullscreen_handled;
            app.startup_window_placement_handled =
                runtime_snapshot.startup_window_placement_handled;
            app.preserve_reset_window_geometry = runtime_snapshot.preserve_reset_window_geometry;
        }
    }
    crate::ui_theme::sync_os_background_blur(ctx);
    app.update_window_title(ctx);

    if image_reload_needed && !app.image_entries.is_empty() {
        app.virtual_texture = None;
        app.vt_progress = None;
        app.vt_total_tiles = 0;
        app.virtual_texture_thread = None;
        app.invalidate_compare_textures();
        if app.inspect_mode {
            app.refresh_inspect_texture(ctx);
        }
        if app.load_from_cache(app.current_index) {
            app.set_loading_state(crate::app::types::LoadingState::Idle);
            app.image_error = None;
        } else {
            app.load_current_image_preserving_cache();
        }
    }
    ctx.request_repaint();
}

fn render_settings_window_contents(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    // Keep the wider slider preference local to this Settings window. Applying it
    // to the shared Context style would also widen sliders in the main viewer.
    ui.scope(|ui| {
        ui.spacing_mut().slider_width *= 2.0;
        render_settings_window_contents_inner(app, ui, ctx);
    });
}

fn render_settings_window_contents_inner(
    app: &mut ViewerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
) {
    let page = SettingsPage::from_index(app.settings_active_page);
    let search_id = egui::Id::unique(("jpeg_viewer_settings_search", app.settings_window_generation));
    let mut search_query = ctx.data(|data| data.get_temp::<String>(search_id)).unwrap_or_default();
    let visuals = ui.visuals().clone();

    ui.vertical(|ui| {
        ui.horizontal(|header| {
            let (icon_rect, _) = header.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
            crate::app::ui::paint_svg_icon(
                header,
                icon_rect,
                crate::app::ui::SvgIcon::Settings,
                header.visuals().text_color(),
            );
            header.vertical(|title| {
                title.label(egui::RichText::new("Settings").size(17.0).strong());
                title.label(
                    egui::RichText::new("Most changes preview immediately. Startup and worker-pool options need a restart; Save writes to settings.ini.")
                        .size(11.5)
                        .color(title.visuals().weak_text_color()),
                );
            });
        });
        ui.add_space(8.0);
        ui.separator();

        // Reserve only the space the footer actually needs. Reset feedback and
        // save errors get additional room when present, avoiding both clipped
        // buttons on short windows and the oversized empty strip above the footer.
        let reset_feedback_height = if advanced::reset_all::feedback_is_visible(ctx) {
            48.0
        } else {
            0.0
        };
        let save_error_height = if app.settings_save_error.is_some() { 28.0 } else { 0.0 };
        let footer_reserved = 52.0 + reset_feedback_height + save_error_height;
        let body_height = (ui.available_height() - footer_reserved).max(0.0);
        let body_width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(body_width, body_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |body| {
                let nav_width = 184.0_f32.min((body.available_width() * 0.34).max(150.0));
                egui::Frame::new()
                    .fill(visuals.extreme_bg_color)
                    .stroke(egui::Stroke::new(0.7, visuals.widgets.noninteractive.bg_stroke.color))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(8))
                    .show(body, |nav| {
                        // A Frame inherits its parent's layout. The body is
                        // left-to-right, so use a top-down child layout for the
                        // navigation column without changing the parent body.
                        nav.with_layout(egui::Layout::top_down(egui::Align::Min), |nav| {
                            nav.set_min_width(nav_width);
                            nav.set_max_width(nav_width);
                            nav.label(egui::RichText::new("PREFERENCES").small().strong().weak());
                            nav.add_space(7.0);
                            let mut search_changed = false;
                            nav.horizontal(|search| {
                                let width = (nav_width - 48.0).max(80.0);
                                let response = search.add_sized(
                                    [width, 28.0],
                                    egui::TextEdit::singleline(&mut search_query)
                                        .hint_text("Search settings…")
                                        .margin(egui::vec2(7.0, 5.0)),
                                );
                                search_changed = response.changed();
                                if search.small_button("×").clicked() {
                                    search_query.clear();
                                    search_changed = true;
                                }
                            });
                            if search_changed {
                                ctx.data_mut(|data| data.insert_temp(search_id, search_query.clone()));
                            }
                            nav.add_space(8.0);

                            if search_query.trim().is_empty() {
                                for destination in SettingsPage::ALL {
                                    let selected = page == destination;
                                    let (rect, response) = nav.allocate_exact_size(
                                        egui::vec2(nav_width - 16.0, 34.0),
                                        egui::Sense::click(),
                                    );
                                    if selected || response.hovered() || response.is_pointer_button_down_on() {
                                        let fill = if response.is_pointer_button_down_on() {
                                            nav.visuals().widgets.active.bg_fill
                                        } else if selected {
                                            nav.visuals().selection.bg_fill
                                        } else {
                                            nav.visuals().widgets.hovered.bg_fill
                                        };
                                        nav.painter().rect_filled(rect, egui::CornerRadius::same(6), fill);
                                    }
                                    let foreground = if selected || response.hovered() {
                                        nav.visuals().text_color()
                                    } else {
                                        nav.visuals().widgets.inactive.fg_stroke.color
                                    };
                                    let icon_rect = egui::Rect::from_center_size(
                                        egui::pos2(rect.left() + 16.0, rect.center().y),
                                        egui::vec2(17.0, 17.0),
                                    );
                                    crate::app::ui::paint_svg_icon(nav, icon_rect, destination.icon(), foreground);
                                    nav.painter().text(
                                        egui::pos2(icon_rect.right() + 8.0, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        destination.label(),
                                        egui::FontId::proportional(13.0),
                                        foreground,
                                    );
                                    if response.clicked() {
                                        app.settings_active_page = destination.index();
                                    }
                                }
                            } else {
                                nav.label(egui::RichText::new("MATCHING SETTINGS").small().strong().weak());
                                nav.add_space(4.0);
                                let normalized_query = search_query.trim().to_lowercase();
                                let mut match_count = 0usize;
                                egui::ScrollArea::vertical()
                                    .id_salt(("settings_search_results", app.settings_window_generation))
                                    .auto_shrink([false, false])
                                    .show(nav, |results| {
                                        for entry in SETTINGS_SEARCH_ENTRIES.iter().filter(|entry| settings_search_matches(entry, &normalized_query)) {
                                            match_count += 1;
                                            let selected = page == entry.page;
                                            let (rect, response) = results.allocate_exact_size(
                                                egui::vec2((nav_width - 16.0).max(1.0), 42.0),
                                                egui::Sense::click(),
                                            );
                                            if selected || response.hovered() || response.is_pointer_button_down_on() {
                                                let fill = if response.is_pointer_button_down_on() {
                                                    results.visuals().widgets.active.bg_fill
                                                } else if selected {
                                                    results.visuals().selection.bg_fill
                                                } else {
                                                    results.visuals().widgets.hovered.bg_fill
                                                };
                                                results.painter().rect_filled(rect, egui::CornerRadius::same(6), fill);
                                            }
                                            let foreground = if selected || response.hovered() {
                                                results.visuals().text_color()
                                            } else {
                                                results.visuals().widgets.inactive.fg_stroke.color
                                            };
                                            let row_painter = results.painter().with_clip_rect(rect.shrink(5.0));
                                            row_painter.text(
                                                egui::pos2(rect.left() + 8.0, rect.top() + 12.0),
                                                egui::Align2::LEFT_CENTER,
                                                entry.label,
                                                egui::FontId::proportional(12.0),
                                                foreground,
                                            );
                                            row_painter.text(
                                                egui::pos2(rect.left() + 8.0, rect.top() + 29.0),
                                                egui::Align2::LEFT_CENTER,
                                                entry.page.label(),
                                                egui::FontId::proportional(10.5),
                                                results.visuals().weak_text_color(),
                                            );
                                            if response.clicked() {
                                                app.settings_active_page = entry.page.index();
                                                set_settings_search_target(ctx, settings_search_target(entry));
                                            }
                                            results.add_space(2.0);
                                        }
                                    });
                                if match_count == 0 {
                                    nav.add_space(8.0);
                                    nav.label(egui::RichText::new("No matching settings").small().weak());
                                }
                            }
                        });
                    });

                body.add_space(12.0);
                body.vertical(|content| {
                    let content_width = content.available_width();
                    content.set_min_width(content_width);
                    content.label(egui::RichText::new(page.label()).size(19.0).strong());
                    content.label(
                        egui::RichText::new(page.description())
                            .size(12.0)
                            .color(content.visuals().weak_text_color()),
                    );
                    content.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("settings_page_scroll", page.index()))
                        .auto_shrink([false, false])
                        .show(content, |content| {
                            match page {
                                SettingsPage::Look => look::render(app, content, ctx),
                                SettingsPage::General => {
                                    content.label(
                                        egui::RichText::new("Live preview is used where possible. Startup and worker-pool options take effect after restarting JPEG Viewer.")
                                            .small()
                                            .weak(),
                                    );
                                    copy_buttons::render(app, content);
                                    general::render(app, content, ctx);
                                }
                                SettingsPage::Viewing => viewing::render(app, content, ctx),
                                SettingsPage::Slideshow => slideshow::render(app, content, ctx),
                                SettingsPage::Tricks => tricks::render(app, content, ctx),
                                SettingsPage::Advanced => advanced::render(app, content, ctx),
                                SettingsPage::Updates => settings_search_anchor(content, "page_updates", |content| updates::render(app, content, ctx)),
                                SettingsPage::SettingsFile => settings_search_anchor(content, "page_settings_file", |content| settings_file::render(app, content, ctx)),
                            }
                            content.add_space(12.0);
                        });
                });
            },
        );

        ui.separator();
        if let Some(error) = &app.settings_save_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Could not save settings: {error}"),
            );
            ui.add_space(3.0);
        }
        ui.horizontal(|footer| {
            settings_search_anchor(footer, "advanced_reset_all", |footer| {
                advanced::reset_all::render(app, footer, ctx);
            });
            footer.with_layout(egui::Layout::right_to_left(egui::Align::Center), |actions| {
                let save = settings_icon_button(
                    actions,
                    crate::app::ui::SvgIcon::ContextArchive,
                    "Save",
                    egui::vec2(88.0, 34.0),
                    app.settings_manager.is_dirty() || app.settings_reset_all_previewed,
                );
                if save.clicked() {
                    match app.settings_manager.save_if_dirty() {
                        Ok(()) => {
                            app.finish_settings_window();
                            actions.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => app.settings_save_error = Some(error),
                    }
                }
                if settings_icon_button(
                    actions,
                    crate::app::ui::SvgIcon::Undo,
                    "Discard",
                    egui::vec2(90.0, 34.0),
                    true,
                )
                .clicked()
                {
                    app.discard_settings_session(ctx);
                    actions.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        ui.add_space(2.0);
    });
}

/// A compact Settings action button that paints one of the shared SVG assets
/// beside its label while preserving egui's standard button interaction states.
pub(crate) fn settings_icon_button(
    ui: &mut egui::Ui,
    icon: crate::app::ui::SvgIcon,
    label: &str,
    min_size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let font_id = egui::FontId::proportional(13.0);
    let measured = ui.painter().layout_no_wrap(
        label.to_owned(),
        font_id.clone(),
        ui.visuals().text_color(),
    );
    let content_width = 10.0 + 16.0 + 7.0 + measured.size().x + 10.0;
    let desired_size = egui::vec2(min_size.x.max(content_width), min_size.y.max(32.0));
    let response = ui.add_enabled(enabled, egui::Button::new("").min_size(desired_size));

    let foreground = if !enabled {
        ui.visuals().widgets.noninteractive.fg_stroke.color
    } else if response.is_pointer_button_down_on() {
        ui.visuals().widgets.active.fg_stroke.color
    } else if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(response.rect.left() + 10.0 + 8.0, response.rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    crate::app::ui::paint_svg_icon(ui, icon_rect, icon, foreground);
    ui.painter().text(
        egui::pos2(icon_rect.right() + 7.0, response.rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font_id,
        foreground,
    );
    response
}

fn render_settings_close_prompt(app: &mut ViewerApp, ctx: &egui::Context) {
    let mut keep_open = true;
    egui::Window::new("Unsaved changes")
        .id(egui::Id::unique(("jpeg_viewer_settings_close_prompt", app.settings_window_generation)))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut keep_open)
        .show(ctx, |ui| {
            ui.label("You have unsaved changes to your settings.");
            ui.label("Save them before closing Settings?");
            if let Some(error) = &app.settings_save_error {
                ui.colored_label(ui.visuals().error_fg_color, format!("Save failed: {error}"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save and close").clicked() {
                    match app.settings_manager.save_if_dirty() {
                        Ok(()) => {
                            app.finish_settings_window();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => app.settings_save_error = Some(error),
                    }
                }
                if ui.button("Discard changes").clicked() {
                    app.discard_settings_session(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Continue editing").clicked() {
                    app.settings_close_prompt = false;
                }
            });
        });
    if !keep_open {
        app.settings_close_prompt = false;
    }
}
