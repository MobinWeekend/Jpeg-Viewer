//! Compact, single-row application menu bar for desktop-style Look profiles.
//!
//! The widgets are rendered by egui so the menu bar remains available on every
//! platform. Automatic mode enables it for COSMIC and GNOME profiles; the user
//! can force it on or off from Settings > Look.

use crate::app::types::{LoadingState, ViewerApp};
use crate::shortcuts::ViewerCommand;
use eframe::egui;

const MENU_BAR_LAYOUT_HEIGHT_ID: &str = "jpeg_viewer_desktop_menu_bar_layout_height";
const MENU_BAR_INLINE_ID: &str = "jpeg_viewer_desktop_menu_bar_inline_in_titlebar";
const MENU_BAR_PREF_ID: &str = "jpeg_viewer_desktop_menu_bar_preference";
const HELP_WINDOW_GENERATION_ID: &str = "jpeg_viewer_help_window_generation";

/// Cache the persisted preference in the egui context so geometry and overlay
/// helpers that only receive a Context can use the same setting.
pub fn set_desktop_menu_bar_preference(ctx: &egui::Context, preference: &str) {
    let safe = match preference {
        "on" | "off" => preference,
        _ => "auto",
    };
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(MENU_BAR_PREF_ID), safe.to_owned());
    });
}

pub fn uses_desktop_menu_bar(ctx: &egui::Context) -> bool {
    // Fullscreen hides the menu strip, independently of the saved preference.
    if ctx.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
        return false;
    }
    let profile = crate::ui_theme::current_look_profile(ctx);
    let preference = ctx.data(|data| {
        data.get_temp::<String>(egui::Id::new(MENU_BAR_PREF_ID))
            .unwrap_or_else(|| "auto".to_owned())
    });
    // An explicit Off setting always wins. Otherwise the Windows 98 profile
    // keeps its classic menu bar, while Automatic follows supported desktop looks.
    match preference.as_str() {
        "off" => false,
        "on" => true,
        _ => profile == "win98" || matches!(profile, "cosmic" | "gnome"),
    }
}

/// Whether the desktop menu belongs to the custom title-bar row instead of a
/// separate top panel. This is the layout used when custom-title-bar navigation
/// is disabled: the menu occupies the left side of the title bar.
pub fn desktop_menu_bar_in_custom_titlebar(app: &ViewerApp, ctx: &egui::Context) -> bool {
    !ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
        && !app.settings_manager.get().show_titlebar
        && !app.settings_manager.get().custom_titlebar_navigation
        && uses_desktop_menu_bar(ctx)
}

/// Total vertical space occupied by top chrome. When both panels exist, egui's
/// panel response reports only the menu panel's own height, so add the measured
/// custom title-bar height as well. In inline mode the title bar already includes
/// the menu and must only be counted once.
pub fn desktop_menu_bar_height(ctx: &egui::Context) -> f32 {
    if !uses_desktop_menu_bar(ctx) {
        return 0.0;
    }
    let (menu_height, inline) = ctx.data(|data| {
        (
            data.get_temp::<f32>(egui::Id::new(MENU_BAR_LAYOUT_HEIGHT_ID))
                .unwrap_or(0.0),
            data.get_temp::<bool>(egui::Id::new(MENU_BAR_INLINE_ID))
                .unwrap_or(false),
        )
    });
    let titlebar_height = crate::app::ui::custom_titlebar_height(ctx);
    if inline {
        titlebar_height
    } else {
        titlebar_height + menu_height
    }
}

/// Render the desktop menu in its own row only when it is not embedded in the
/// custom title bar. The inline flag is refreshed every frame to prevent stale
/// geometry after settings or fullscreen changes.
pub fn render_desktop_menu_bar(app: &mut ViewerApp, ctx: &egui::Context) {
    let inline = desktop_menu_bar_in_custom_titlebar(app, ctx);
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(MENU_BAR_INLINE_ID), inline);
    });
    if !uses_desktop_menu_bar(ctx) || inline {
        return;
    }

    // The custom title bar is registered as its own top panel before this one,
    // so egui naturally places the menu bar immediately beneath it. No guessed
    // title-bar or menu-bar heights are needed.
    let profile = crate::ui_theme::current_look_profile(ctx);
    // The menu strip shares the main-view background, including its configured
    // opacity when OS/DE backdrop blur is enabled.
    let dark_mode = ctx.style().visuals.dark_mode;
    let settings = app.settings_manager.get();
    let background = crate::ui_theme::main_view_surface_color(
        ctx,
        &settings.main_view_background,
        settings.main_view_background_opacity,
    );
    let mut visuals = ctx.style().visuals.clone();
    visuals.panel_fill = background;
    visuals.window_fill = background;
    if crate::ui_theme::os_background_blur_enabled(ctx) && profile != "win98" {
        let alpha = |color: egui::Color32, opacity: u8| {
            let [r, g, b, _] = color.to_srgba_unmultiplied();
            egui::Color32::from_rgba_unmultiplied(r, g, b, opacity)
        };
        visuals.panel_fill = alpha(visuals.panel_fill, 224);
        visuals.window_fill = alpha(visuals.window_fill, 250);
        visuals.extreme_bg_color = alpha(visuals.extreme_bg_color, 250);
        visuals.faint_bg_color = alpha(visuals.faint_bg_color, 232);
        visuals.code_bg_color = alpha(visuals.code_bg_color, 238);
        visuals.text_edit_bg_color = Some(alpha(visuals.text_edit_bg_color(), 250));
        visuals.widgets.noninteractive.bg_fill = alpha(visuals.widgets.noninteractive.bg_fill, 228);
        visuals.widgets.inactive.bg_fill = alpha(visuals.widgets.inactive.bg_fill, 246);
        visuals.widgets.inactive.weak_bg_fill = alpha(visuals.widgets.inactive.weak_bg_fill, 242);
        visuals.widgets.hovered.bg_fill = alpha(visuals.widgets.hovered.bg_fill, 248);
        visuals.widgets.hovered.weak_bg_fill = alpha(visuals.widgets.hovered.weak_bg_fill, 248);
        visuals.widgets.active.bg_fill = alpha(visuals.widgets.active.bg_fill, 250);
        visuals.widgets.active.weak_bg_fill = alpha(visuals.widgets.active.weak_bg_fill, 250);
        visuals.widgets.open.bg_fill = alpha(visuals.widgets.open.bg_fill, 248);
        visuals.widgets.open.weak_bg_fill = alpha(visuals.widgets.open.weak_bg_fill, 248);
    }
    if profile == "win98" {
        let face = if dark_mode {
            egui::Color32::from_rgb(38, 40, 43)
        } else {
            egui::Color32::from_rgb(192, 192, 192)
        };
        let sunken = if dark_mode {
            egui::Color32::from_rgb(48, 50, 54)
        } else {
            egui::Color32::from_rgb(212, 208, 200)
        };
        let inner = if dark_mode {
            egui::Color32::from_rgb(25, 27, 30)
        } else {
            egui::Color32::WHITE
        };
        let border = if dark_mode {
            egui::Color32::from_rgb(82, 85, 90)
        } else {
            egui::Color32::from_rgb(128, 128, 128)
        };
        let accent = crate::ui_theme::main_accent_color(ctx);
        let hover = if dark_mode {
            accent.linear_multiply(0.48)
        } else {
            accent
        };
        let text = if dark_mode {
            egui::Color32::from_rgb(238, 240, 243)
        } else {
            egui::Color32::BLACK
        };
        visuals.dark_mode = dark_mode;
        visuals.panel_fill = face;
        visuals.window_fill = face;
        visuals.extreme_bg_color = inner;
        visuals.faint_bg_color = sunken;
        visuals.code_bg_color = inner;
        visuals.text_edit_bg_color = Some(inner);
        visuals.widgets.noninteractive.bg_fill = face;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.inactive.bg_fill = face;
        visuals.widgets.inactive.weak_bg_fill = face;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.hovered.bg_fill = hover;
        visuals.widgets.hovered.weak_bg_fill = hover;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark_mode || crate::ui_theme::contrast_prefers_light_text(hover) {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            },
        );
        visuals.widgets.active.bg_fill = if dark_mode {
            accent.linear_multiply(0.34)
        } else {
            sunken
        };
        visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.open.bg_fill = if dark_mode { hover } else { sunken };
        visuals.widgets.open.weak_bg_fill = visuals.widgets.open.bg_fill;
        visuals.widgets.open.fg_stroke = egui::Stroke::new(
            1.0_f32,
            if dark_mode {
                egui::Color32::WHITE
            } else {
                text
            },
        );
        visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::ZERO;
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
        visuals.widgets.active.corner_radius = egui::CornerRadius::ZERO;
        visuals.widgets.open.corner_radius = egui::CornerRadius::ZERO;
        // MenuBar temporarily installs its own visuals on the child UI, so set
        // popup radii here as well as in the app-wide theme. Otherwise context and
        // menu popups can retain egui's rounded defaults in the Win98 profile.
        visuals.window_corner_radius = egui::CornerRadius::ZERO;
        visuals.menu_corner_radius = egui::CornerRadius::ZERO;
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 0],
            blur: 0,
            spread: 0,
            color: egui::Color32::TRANSPARENT,
        };
        visuals.popup_shadow = visuals.window_shadow;
        visuals.window_stroke = egui::Stroke::new(
            1.0_f32,
            if dark_mode {
                egui::Color32::from_rgb(13, 14, 16)
            } else {
                egui::Color32::BLACK
            },
        );
        visuals.hyperlink_color = if dark_mode {
            egui::Color32::from_rgb(151, 190, 255)
        } else {
            egui::Color32::from_rgb(0, 0, 238)
        };
    }
    // Keep the panel surface aligned with the main view even when the Win98
    // profile customizes individual menu-item widget fills.
    visuals.panel_fill = background;
    // Status and secondary labels stay fully legible on the menu.
    visuals.weak_text_alpha = 1.0;
    let fill = background;
    let bar_visuals = visuals.clone();

    // Keep the top panel only as a dynamic-height layout reservation. The
    // visible menu frame is a content-wrapping frame, not a full-window strip.
    let panel_response = egui::TopBottomPanel::top("jpeg_viewer_desktop_menu_bar")
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::ZERO)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |panel_ui| {
            panel_ui.style_mut().visuals = bar_visuals.clone();
            let total_width = panel_ui.available_width().max(0.0);
            let compact_labels = total_width < 520.0;
            let is_loading = app.is_loading();
            let row_height = panel_ui.spacing().interact_size.y;
            let bar_frame = egui::Frame::new()
                .fill(fill)
                .inner_margin(if profile == "win98" {
                    egui::Margin::ZERO
                } else {
                    egui::Margin::symmetric(8, 0)
                })
                .stroke(egui::Stroke::NONE);

            let bar_response = bar_frame.show(panel_ui, |bar_ui| {
                // The panel and frame reserve only their measured content size.
                // No width constraint, clipping viewport, or nested MenuBar is
                // needed: the top panel already supplies the available window area.
                bar_ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                bar_ui.horizontal(|row| {
                    row.spacing_mut().item_spacing.x = if compact_labels { 6.0 } else { 18.0 };
                    row.spacing_mut().item_spacing.y = 0.0;
                    row.spacing_mut().button_padding.x = if compact_labels { 8.0 } else { 12.0 };
                    row.spacing_mut().button_padding.y = 0.0;
                    row.spacing_mut().interact_size.y = row_height;
                    render_menus(app, ctx, row, compact_labels);

                    // In standalone mode the indicator is part of the menu row;
                    // inline-titlebar mode skips this panel and places it in the view.
                    if is_loading {
                        row.add_space(4.0);
                        let spinner = row.add(egui::Spinner::new().size(14.0));
                        spinner.on_hover_text(loading_status_text(app));
                        if total_width >= 700.0 {
                            row.add_space(4.0);
                            row.label(egui::RichText::new(loading_status_text(app)).small().weak());
                        }
                    }
                });
            });
            bar_response.response.rect.height()
        });
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(MENU_BAR_LAYOUT_HEIGHT_ID),
            panel_response.response.rect.height(),
        );
    });
}

/// Draw the desktop menu widgets inside a caller-owned title-bar row. The menu
/// takes only the width its items use; excess width remains available for the
/// title. There is intentionally no horizontal scroll area.
pub fn render_desktop_menu_bar_in_titlebar(
    app: &mut ViewerApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    row_height: f32,
    available_width: f32,
) {
    if !uses_desktop_menu_bar(ctx) || available_width <= 0.0 {
        return;
    }

    // This helper is deliberately content-sized. Do not give it the title bar's
    // remaining width or put it in a width-constrained child UI: either can make
    // the menu consume the title's drag region. Loading feedback belongs in the
    // main view while the menu is inline, not beside these menu items.
    let compact_labels = available_width < 540.0;
    let previous_style = ui.style().as_ref().clone();
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.spacing_mut().item_spacing.x = if compact_labels { 6.0 } else { 12.0 };
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.spacing_mut().button_padding.x = if compact_labels { 8.0 } else { 12.0 };
    ui.spacing_mut().button_padding.y = 0.0;
    ui.spacing_mut().interact_size.y = row_height;
    // The caller already owns a horizontal title-bar row. Render menu buttons
    // directly into it instead of nesting another MenuBar container, whose
    // allocation can expand to the full available width and erase the title area.
    render_menus(app, ctx, ui, compact_labels);
    *ui.style_mut() = previous_style;
}

fn loading_status_text(app: &ViewerApp) -> String {
    match app.loading_state {
        LoadingState::Indexing => "Indexing…".to_owned(),
        LoadingState::LoadingFullGif => "Loading GIF…".to_owned(),
        LoadingState::VirtualTextureLoading => {
            if let Some(progress) = app.vt_progress {
                if progress.total_tiles > 0 {
                    let percent = (progress.prepared_tiles.saturating_mul(100)
                        / progress.total_tiles)
                        .min(100);
                    return format!("Loading {percent}%");
                }
            }
            "Loading image…".to_owned()
        }
        LoadingState::Loading => "Loading…".to_owned(),
        LoadingState::Idle => String::new(),
    }
}

fn render_menus(app: &mut ViewerApp, ctx: &egui::Context, ui: &mut egui::Ui, compact_labels: bool) {
    // Keep the popup's normal fills and hover feedback, but remove widget
    // outlines from every command inside the dropdowns. The top-level triggers
    // use a separate fully-transparent style below.
    let mut popup_style = ui.style().as_ref().clone();
    popup_style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    popup_style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    popup_style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    popup_style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    popup_style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;

    // The top-level menu triggers are transparent in every state. Their
    // horizontal button padding is intentionally kept slightly wider to make
    // File/Edit/View/etc. easier to target without making the bar bulky.
    let mut trigger_style = popup_style.clone();
    // MenuButton builds its trigger from Button::new, which otherwise inherits
    // `visuals.button_frame` even when the individual widget fills/strokes are
    // transparent. Disable the frame at the style level as well so no theme can
    // reintroduce a solid button surface or outline.
    trigger_style.visuals.button_frame = false;
    trigger_style.visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    trigger_style.visuals.widgets.noninteractive.bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    trigger_style.visuals.widgets.hovered.bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    trigger_style.visuals.widgets.active.bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.active.weak_bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    trigger_style.visuals.widgets.open.bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.open.weak_bg_fill = egui::Color32::TRANSPARENT;
    trigger_style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
    *ui.style_mut() = trigger_style;

    // Keep conventional menu order: File, Edit, View, workflow tools, Help.
    ui.menu_button("File", |ui| {
        *ui.style_mut() = popup_style.clone();
        if ui.button("Open File…").clicked() {
            app.handle_command(ctx, ViewerCommand::OpenFile);
            ui.close();
        }
        if ui.button("Open Folder…").clicked() {
            app.handle_command(ctx, ViewerCommand::OpenFolder);
            ui.close();
        }
        let has_path = app
            .current_image_path
            .as_ref()
            .is_some_and(|path| path.is_file());
        if ui
            .add_enabled(has_path, egui::Button::new("Open With…"))
            .clicked()
        {
            if let Some(path) = app.current_image_path.as_deref() {
                if let Err(error) = crate::app::external_open::open_with(path) {
                    app.queue_status =
                        Some(format!("Could not open the Open With dialog: {error}"));
                }
            }
            ui.close();
        }
        let can_delete_current = matches!(
            app.image_entries.get(app.current_index),
            Some(crate::image_entry::ImageEntry::File(_))
        );
        if ui
            .add_enabled(can_delete_current, egui::Button::new("Delete Image…"))
            .clicked()
        {
            app.show_delete_confirmation = true;
            ui.close();
        }
        ui.separator();
        if ui.button("Settings…").clicked() {
            crate::app::ui::toggle_settings_menu(app);
            ui.close();
        }
        ui.separator();
        if ui.button("Quit").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ui.close();
        }
    });

    ui.menu_button("Edit", |ui| {
        *ui.style_mut() = popup_style.clone();
        let can_edit = !app.image_entries.is_empty() && !app.is_loading();
        if ui
            .add_enabled(can_edit, egui::Button::new("Edit Image…"))
            .clicked()
        {
            app.start_editing(ctx);
            ui.close();
        }
        if ui
            .add_enabled(can_edit, egui::Button::new("Crop Image…"))
            .clicked()
        {
            app.start_crop_tool(ctx);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(can_edit, egui::Button::new("Rotate Clockwise"))
            .clicked()
        {
            app.rotate_current_clockwise(ctx);
            ui.close();
        }
        if ui
            .add_enabled(can_edit, egui::Button::new("Rotate Counterclockwise"))
            .clicked()
        {
            app.rotate_current_counterclockwise(ctx);
            ui.close();
        }
    });

    ui.menu_button("View", |ui| {
        *ui.style_mut() = popup_style.clone();
        if ui.button("Browse Sidebar").clicked() {
            select_sidebar_tab(app, "browse");
            ui.close();
        }
        if ui.button("Queue Sidebar").clicked() {
            select_sidebar_tab(app, "queue");
            ui.close();
        }
        if ui.button("Favorites Sidebar").clicked() {
            select_sidebar_tab(app, "favorites");
            ui.close();
        }
        if ui.button("File Information").clicked() {
            select_sidebar_tab(app, "info");
            ui.close();
        }
        if ui
            .add_enabled(
                !app.image_entries.is_empty(),
                egui::Button::new(if app.inspect_mode {
                    "Inspect ✓"
                } else {
                    "Inspect"
                }),
            )
            .clicked()
        {
            app.toggle_inspect_mode(ctx);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!app.image_entries.is_empty(), egui::Button::new("Zoom In"))
            .clicked()
        {
            app.handle_command(ctx, ViewerCommand::ZoomIn);
            ui.close();
        }
        if ui
            .add_enabled(!app.image_entries.is_empty(), egui::Button::new("Zoom Out"))
            .clicked()
        {
            app.handle_command(ctx, ViewerCommand::ZoomOut);
            ui.close();
        }
        if ui
            .add_enabled(
                !app.image_entries.is_empty(),
                egui::Button::new("Fit to Window"),
            )
            .clicked()
        {
            app.handle_command(ctx, ViewerCommand::MakeFit);
            ui.close();
        }
        if ui
            .add_enabled(
                !app.image_entries.is_empty(),
                egui::Button::new("Actual Size (100%)"),
            )
            .clicked()
        {
            app.handle_command(ctx, ViewerCommand::ResetZoom);
            ui.close();
        }
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        if ui
            .button(if fullscreen {
                "Exit Fullscreen"
            } else {
                "Fullscreen"
            })
            .clicked()
        {
            app.handle_command(ctx, ViewerCommand::ToggleFullscreen);
            ui.close();
        }
        if app.is_gif && ui.button("Play / Pause GIF").clicked() {
            app.handle_command(ctx, ViewerCommand::ToggleGifPlay);
            ui.close();
        }
        if ui.button("Compact Controls").clicked() {
            app.handle_command(ctx, ViewerCommand::ToggleCompact);
            ui.close();
        }
    });

    ui.menu_button("Queue", |ui| {
        *ui.style_mut() = popup_style.clone();
        // Keep queue actions flat rather than nesting submenus. This follows
        // desktop menu guidance to group related actions with separators and
        // avoid nested menu hierarchies that are difficult to navigate.
        let has_image = !app.image_entries.is_empty();
        if ui.button("Add Files…").clicked() {
            if let Some(paths) = rfd::FileDialog::new().pick_files() {
                let mut added = 0usize;
                for path in paths {
                    added += usize::from(app.add_queue_path(path));
                }
                app.queue_status = Some(format!("Added {added} file(s)."));
            }
            ui.close();
        }
        if ui
            .add_enabled(has_image, egui::Button::new("Add Current Image"))
            .clicked()
        {
            app.add_current_image_to_queue();
            ui.close();
        }
        if ui
            .add_enabled(has_image, egui::Button::new("Add Current View Images"))
            .clicked()
        {
            app.add_current_folder_images();
            ui.close();
        }
        if ui.button("Use Browse List If Queue Is Empty").clicked() {
            if app.queue_paths.is_empty() {
                app.add_current_folder_images();
            } else {
                app.queue_status =
                    Some("Queue is not empty; existing items were preserved.".into());
            }
            ui.close();
        }
        if ui.button("Add Folder Recursively…").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let added = app.add_folder_recursively(&folder);
                app.queue_status = Some(format!("Added {added} image(s) from folder tree."));
            }
            ui.close();
        }
        if ui.button("Add Folder as One Entry…").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let added = app.add_queue_path(folder);
                app.queue_status = Some(if added {
                    "Added folder as one queue entry.".into()
                } else {
                    "Folder is already in the queue.".into()
                });
            }
            ui.close();
        }

        ui.separator();
        if ui
            .add_enabled(
                !app.queue_paths.is_empty(),
                egui::Button::new("Open Queue in Viewer"),
            )
            .clicked()
        {
            app.open_queue_in_viewer(ctx);
            ui.close();
        }
        if ui
            .add_enabled(
                app.queue_previous_entries.is_some(),
                egui::Button::new("Return to Previous View"),
            )
            .clicked()
        {
            app.return_to_previous_view(ctx);
            ui.close();
        }
        ui.separator();
        if ui.button("Save Queue…").clicked() {
            app.save_queue_dialog();
            ui.close();
        }
        if ui.button("Load Queue…").clicked() {
            app.load_queue_dialog();
            ui.close();
        }
        if ui.button("Export Queue Paths…").clicked() {
            app.export_queue_paths();
            ui.close();
        }
        ui.separator();
        let mut repeat = app.queue_repeat;
        if ui.checkbox(&mut repeat, "Repeat Queue").changed() {
            app.queue_repeat = repeat;
        }
        if ui
            .add_enabled(
                !app.queue_paths.is_empty(),
                egui::Button::new("Clear Queue"),
            )
            .clicked()
        {
            if app.queue_mode && !app.return_to_previous_view(ctx) {
                ui.close();
                return;
            }
            app.queue_paths.clear();
            app.queue_root = None;
            app.queue_mode = false;
            app.queue_status = Some("Queue cleared.".into());
            ui.close();
        }
        if let Some(status) = app.queue_status.as_deref() {
            ui.separator();
            ui.label(egui::RichText::new(status).small().weak());
        }
    });

    ui.menu_button(if compact_labels { "Fav." } else { "Favorites" }, |ui| {
        *ui.style_mut() = popup_style.clone();
        app.ensure_favorites_loaded();
        if ui
            .add_enabled(
                !app.image_entries.is_empty(),
                egui::Button::new("Toggle Current Image Favorite"),
            )
            .clicked()
        {
            app.add_current_image_to_favorites();
            ui.close();
        }
        if ui.button("Add Files…").clicked() {
            if let Some(paths) = rfd::FileDialog::new().pick_files() {
                for path in paths {
                    app.add_favorite_path(path);
                }
            }
            ui.close();
        }
        if ui.button("Add Folder as Entry…").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                app.add_favorite_path(folder);
            }
            ui.close();
        }
        if ui.button("Remove Unavailable Favorites").clicked() {
            let before = app.favorite_paths.len();
            app.favorite_paths.retain(|path| path.exists());
            app.save_favorites_for_ui();
            app.queue_status = Some(format!(
                "Removed {} unavailable favorite(s).",
                before - app.favorite_paths.len()
            ));
            ui.close();
        }
        ui.separator();
        if ui.button("Show Favorites Sidebar").clicked() {
            select_sidebar_tab(app, "favorites");
            ui.close();
        }
        ui.label(format!("{} saved item(s)", app.favorite_paths.len()));
    });

    ui.menu_button("Help", |ui| {
        *ui.style_mut() = popup_style.clone();
        if ui.button("Help and About JPEG Viewer…").clicked() {
            app.toggle_help_menu(ctx);
            ui.close();
        }
    });

    *ui.style_mut() = popup_style;
}

fn select_sidebar_tab(app: &mut ViewerApp, tab: &str) {
    let _ = app
        .settings_manager
        .update(|settings| settings.show_file_sidebar = true);
    app.sidebar_queue_tab = tab == "queue";
    app.sidebar_favorites_tab = tab == "favorites";
    app.sidebar_info_tab = tab == "info";
    app.queue_drop_rect = None;
    if tab == "favorites" {
        app.ensure_favorites_loaded();
    }
}

/// Incremented on each opening so the Help window starts centered each time,
/// but remains movable for the duration of that opening.
pub fn desktop_menu_bar_generation_for_help(ctx: &egui::Context) -> u64 {
    ctx.data(|data| {
        data.get_temp::<u64>(egui::Id::new(HELP_WINDOW_GENERATION_ID))
            .unwrap_or(0)
    })
}

pub fn bump_help_window_generation(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let id = egui::Id::new(HELP_WINDOW_GENERATION_ID);
        let next = data.get_temp::<u64>(id).unwrap_or(0).wrapping_add(1);
        data.insert_temp(id, next);
    });
}
