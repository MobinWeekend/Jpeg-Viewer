use crate::app::types::ViewerApp;
use eframe::egui;

const PROJECT_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer";
const RELEASES_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer/releases";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
enum HelpTab {
    #[default]
    Overview,
    Keyboard,
    Mouse,
    Tips,
    CargoVersions,
}

fn help_nav_item(
    ui: &mut egui::Ui,
    active_tab: &mut HelpTab,
    tab: HelpTab,
    icon: crate::app::ui::SvgIcon,
    label: &str,
) {
    let selected = *active_tab == tab;
    let font_id = egui::FontId::proportional(12.5);
    let text_color = if selected {
        ui.visuals().text_color()
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id.clone(), text_color);
    let width = galley.size().x + 43.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::click());

    if selected || response.hovered() || response.is_pointer_button_down_on() {
        let visuals = if response.is_pointer_button_down_on() {
            &ui.visuals().widgets.active
        } else if selected {
            &ui.visuals().widgets.open
        } else {
            &ui.visuals().widgets.hovered
        };
        let fill = if visuals.bg_fill == egui::Color32::TRANSPARENT {
            crate::ui_theme::with_alpha(
                ui.visuals().selection.bg_fill,
                if selected { 112 } else { 64 },
            )
        } else {
            visuals.bg_fill
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(7), fill);
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    let foreground = if selected || response.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    crate::app::ui::paint_svg_icon(ui, icon_rect, icon, foreground);
    ui.painter().galley(
        egui::pos2(icon_rect.right() + 7.0, rect.center().y - galley.size().y * 0.5),
        galley,
        foreground,
    );

    if selected {
        let indicator = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 12.0, rect.bottom() - 2.0),
            egui::vec2(rect.width() - 24.0, 2.0),
        );
        ui.painter().rect_filled(
            indicator,
            egui::CornerRadius::same(1),
            ui.visuals().selection.stroke.color,
        );
    }

    if response.clicked() {
        *active_tab = tab;
    }
}

impl ViewerApp {
    pub fn render_help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help_menu;
        let viewport = ctx.viewport_rect();
        let available_width = (viewport.width() - 24.0).max(420.0);
        let available_height = (viewport.height() - 40.0).max(360.0);
        let default_size = egui::vec2(
            (viewport.width() * 0.72).clamp(560.0, 900.0).min(available_width),
            (viewport.height() * 0.72).clamp(470.0, 740.0).min(available_height),
        );
        let default_pos = egui::pos2(
            (viewport.center().x - default_size.x * 0.5).max(viewport.left() + 8.0),
            (viewport.center().y - default_size.y * 0.5).max(viewport.top() + 8.0),
        );

        let generation = crate::app::ui::desktop_menu_bar_generation_for_help(ctx);
        let tab_id = egui::Id::unique(("jpeg_viewer_help_tab", generation));
        let mut active_tab = ctx
            .data(|data| data.get_temp::<HelpTab>(tab_id))
            .unwrap_or_default();
        let previous_tab = active_tab;

        egui::Window::new("Help & About · JPEG Viewer")
            .id(egui::Id::unique(("jpeg_viewer_help_window", generation)))
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .movable(true)
            .default_pos(default_pos)
            .default_size(default_size)
            .min_size([420.0, 360.0])
            .max_size([1120.0, 900.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let available = ui.available_size_before_wrap();
                let width = available.x.max(1.0);
                let height = available.y.max(1.0);
                let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
                let weak_text = ui.visuals().weak_text_color();

                ui.horizontal(|header| {
                    header.spacing_mut().item_spacing.x = 10.0;
                    let (logo_rect, _) = header.allocate_exact_size(
                        egui::vec2(34.0, 34.0),
                        egui::Sense::hover(),
                    );
                    crate::app::ui::paint_svg_icon(
                        header,
                        logo_rect,
                        crate::app::ui::SvgIcon::AppLogo,
                        header.visuals().text_color(),
                    );
                    header.vertical(|brand| {
                        brand.label(
                            egui::RichText::new("JPEG Viewer")
                                .size(16.0)
                                .strong(),
                        );
                        brand.label(
                            egui::RichText::new("by Mobin Sasanpour")
                                .size(11.5)
                                .color(weak_text),
                        );
                    });
                    header.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |meta| {
                            egui::Frame::new()
                                .fill(meta.visuals().faint_bg_color)
                                .stroke(egui::Stroke::new(0.7, border))
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .show(meta, |meta| {
                                    meta.label(
                                        egui::RichText::new(format!(
                                            "v{}",
                                            env!("CARGO_PKG_VERSION")
                                        ))
                                        .monospace()
                                        .size(11.0)
                                        .color(meta.visuals().weak_text_color()),
                                    );
                                });
                        },
                    );
                });

                ui.add_space(10.0);
                ui.horizontal_wrapped(|tabs| {
                    tabs.spacing_mut().item_spacing.x = 5.0;
                    help_nav_item(
                        tabs,
                        &mut active_tab,
                        HelpTab::Overview,
                        crate::app::ui::SvgIcon::Home,
                        "Overview",
                    );
                    help_nav_item(
                        tabs,
                        &mut active_tab,
                        HelpTab::Keyboard,
                        crate::app::ui::SvgIcon::Input,
                        "Keyboard",
                    );
                    help_nav_item(
                        tabs,
                        &mut active_tab,
                        HelpTab::Mouse,
                        crate::app::ui::SvgIcon::Inspect,
                        "Mouse",
                    );
                    help_nav_item(
                        tabs,
                        &mut active_tab,
                        HelpTab::Tips,
                        crate::app::ui::SvgIcon::Image,
                        "Tips",
                    );
                    help_nav_item(
                        tabs,
                        &mut active_tab,
                        HelpTab::CargoVersions,
                        crate::app::ui::SvgIcon::SettingsFile,
                        "Cargo versions",
                    );
                });

                ui.add_space(5.0);
                ui.separator();
                ui.add_space(8.0);

                let (heading, subtitle) = match active_tab {
                    HelpTab::Overview => (
                        "A focused viewer for your image library",
                        "Browse, compare, organize, and make quick edits without leaving the viewer.",
                    ),
                    HelpTab::Keyboard => (
                        "Keyboard shortcuts",
                        "Quick reference for navigation, zoom, display, workspace, and file actions.",
                    ),
                    HelpTab::Mouse => (
                        "Mouse & touch controls",
                        "Pointer gestures for navigation, zooming, panning, and window control.",
                    ),
                    HelpTab::Tips => (
                        "Tips & workflows",
                        "Settings and small workflow improvements for everyday use.",
                    ),
                    HelpTab::CargoVersions => (
                        "Cargo & build versions",
                        "The pinned Rust toolchain and resolved GUI/windowing crate versions used by this build.",
                    ),
                };
                ui.label(egui::RichText::new(heading).size(19.0).strong());
                ui.label(egui::RichText::new(subtitle).size(12.5).color(weak_text));
                ui.add_space(8.0);

                // At the minimum window width, the fifth tab wraps to a second row.
                // Reserve that extra row so the footer remains visible and the article
                // keeps its own scroll area instead of pushing the window content out.
                let wrapped_tab_height = if width < 470.0 { 34.0 } else { 0.0 };
                let body_height = (height - 174.0 - wrapped_tab_height).max(100.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, body_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |body| {
                        body.set_min_width(width);
                        body.set_max_width(width);
                        egui::Frame::new()
                            .fill(body.visuals().extreme_bg_color)
                            .stroke(egui::Stroke::new(0.7, border))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin::same(13))
                            .show(body, |panel| {
                                egui::ScrollArea::vertical()
                                    .id_salt(("jpeg_viewer_help_content", generation, active_tab))
                                    .auto_shrink([false, false])
                                    .show(panel, |content| match active_tab {
                                        HelpTab::Overview => Self::render_help_overview(content),
                                        HelpTab::Keyboard => Self::render_keyboard_help(content),
                                        HelpTab::Mouse => Self::render_mouse_help(content),
                                        HelpTab::Tips => Self::render_help_tips(content),
                                        HelpTab::CargoVersions => {
                                            Self::render_cargo_versions(content)
                                        }
                                    });
                            });
                    },
                );

                ui.add_space(5.0);
                ui.horizontal(|footer| {
                    footer.label(
                        egui::RichText::new("Project")
                            .small()
                            .weak(),
                    );
                    footer.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |links| {
                            for (label, url) in [
                                ("Releases ↗", RELEASES_URL),
                                ("GitHub ↗", PROJECT_URL),
                            ] {
                                let response = links.add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .size(11.5)
                                            .color(links.visuals().hyperlink_color),
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE),
                                );
                                if response.clicked() {
                                    open_external(url, label);
                                }
                            }
                        },
                    );
                });
            });

        if active_tab != previous_tab {
            ctx.data_mut(|data| data.insert_temp(tab_id, active_tab));
        }
        self.show_help_menu = open;
    }

    fn render_help_overview(ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(egui::RichText::new("FEATURES").small().strong().color(ui.visuals().weak_text_color()));

        // Two columns are useful on wide windows; a single readable list is much
        // easier to scan at 512px and avoids squeezing descriptions into narrow cards.
        // some feel stupid to boast about and considering I want the user to quickly see the keyboard shortcuts, I commented most out :)
        let features = [
            "Browse common raster, animated, vector, and camera RAW formats",
            "Open images inside ZIP, 7z, and RAR archives without extracting them",
            "Browse, Queue, Favorites, and Info sidebar tabs",
            "Firefox-style workspace tabs with drag-and-drop and keyboard reordering",
            "Image-tab pop-outs with click-through, opacity, monitor-aware DVD and Ball movement, right-drag throws, hover controls, and looping GIF playback",
            "Local and Folder Masonry galleries with size-adaptive thumbnails, sorting, recursive folder sections, multi-select, and a centered eye marker on the current image",
            "Masonry Ctrl+click enters Select mode; plain clicks toggle entries while Select mode is active, with hover and selection feedback shown independently",
            "Loaded Folder Masonry views update after file moves and deletes",
            "Compare from the desktop menu bar or hamburger; drop images or add multiple files with the picker",
            "Independent comparison workspaces with per-pane removal, synchronized pan/zoom, right-click anchor placement, split reordering, and anchor-based rotation/alignment",
            "Save and load .jvq queues; reorder queue and favorite entries",
            "Edit supported raster files: crop, rotate, flip, adjust color, undo/redo, and export",
            "GIF playback controls and configurable slideshow transitions and motion",
            "Desktop looks for JPEG Viewer, COSMIC, GNOME / Adwaita, and Windows 98",
            "UI scaling, custom accents, transparency checkerboard, and optional OS/DE glass",
            "Background decoding, preloading, cache controls, and tiled rendering for very large images",
            "Optional single-instance mode forwards later file opens and multi-file selections to the running window",
            "Adjustable main-view padding, double-left-click image fitting, 20% pan visibility clamp, and automatic or one-time window fitting around the current image",
            "File actions include Open With, locate, rename, copy path, and trash where supported",
        ];

        if ui.available_width() >= 680.0 {
            ui.columns(2, |columns| {
                for (index, feature) in features.iter().enumerate() {
                    let column = index % 2;
                    columns[column].horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("•").strong().color(ui.visuals().hyperlink_color));
                        ui.label(egui::RichText::new(*feature).size(13.0));
                    });
                    columns[column].add_space(3.0);
                }
            });
        } else {
            for feature in features {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("•").strong().color(ui.visuals().hyperlink_color));
                    ui.label(egui::RichText::new(feature).size(13.0));
                });
                ui.add_space(2.0);
            }
        }

        ui.add_space(5.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("GETTING STARTED").small().strong());
                ui.add_space(3.0);
                for item in [
                    "Drag an image, folder, or supported archive into the window to open it.",
                    "Use File → Open Folder + Subfolders to include nested folders.",
                    "Open Compare from the desktop menu bar or hamburger; drop images or choose Add Images…",
                    "Open an Image tab's POP-OUT status button to show the right-hand controls sidebar for its floating window, click-through, size, DVD and Ball tricks",
                    "In Ball mode, right-drag the pop-out image to throw it; left-drag moves the window",
                    "Press F5 to open Settings, or F1 to return to this help window.",
                    "Use the Keyboard and Mouse tabs for the complete control reference.",
                ] {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("•").color(ui.visuals().hyperlink_color));
                        ui.label(egui::RichText::new(item).size(13.0));
                    });
                }
            });
        ui.add_space(4.0);
    }

    fn render_cargo_versions(ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("VERSIONS USED BY THIS PROJECT")
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(7.0);

        let rust_version = env!("JPEG_VIEWER_RUST_VERSION");
        let eframe_version = env!("JPEG_VIEWER_EFRAME_VERSION");
        let egui_version = env!("JPEG_VIEWER_EGUI_VERSION");
        let winit_version = env!("JPEG_VIEWER_WINIT_VERSION");

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Rust toolchain").strong().size(13.5));
                Self::render_version_row(
                    ui,
                    "Rust",
                    rust_version,
                    "Pinned in rust-toolchain.toml",
                );
            });
        ui.add_space(7.0);

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("GUI & windowing crates").strong().size(13.5));
                ui.add_space(3.0);
                Self::render_version_row(ui, "eframe", eframe_version, "Resolved in Cargo.lock");
                Self::render_version_row(ui, "egui", egui_version, "Resolved in Cargo.lock");
                Self::render_version_row(ui, "winit", winit_version, "Resolved in Cargo.lock");
            });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Rust is pinned in rust-toolchain.toml. The crate versions are resolved from Cargo.lock. Cargo itself is supplied with the Rust toolchain and is not separately pinned by this project.",
            )
            .small()
            .weak(),
        );
    }

    fn render_version_row(ui: &mut egui::Ui, name: &str, version: &str, source: &str) {
        ui.horizontal(|row| {
            row.vertical(|details| {
                details.label(egui::RichText::new(name).size(12.5).strong());
                details.label(egui::RichText::new(source).size(10.5).weak());
            });
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |value| {
                egui::Frame::new()
                    .fill(value.visuals().faint_bg_color)
                    .stroke(egui::Stroke::new(0.7, value.visuals().widgets.noninteractive.bg_stroke.color))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(value, |value| {
                        value.label(
                            egui::RichText::new(version)
                                .monospace()
                                .size(12.5)
                                .strong(),
                        );
                    });
            });
        });
        ui.add_space(5.0);
    }

    fn render_keyboard_help(ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Shortcuts are paused while a text field has keyboard focus.")
                .small()
                .weak(),
        );
        ui.add_space(5.0);

        Self::render_shortcut_group(
            ui,
            "Navigation",
            "help_navigation",
            &[
                ("◀ / ▶", "Previous / Next image"),
                ("A / D", "Previous / Next image"),
                ("Home", "Jump to first image"),
                ("End", "Jump to last image"),
                ("Ctrl+◀ / Ctrl+▶", "Jump to first / last image"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Zoom & view",
            "help_zoom_view",
            &[
                ("+ / -", "Zoom in / Zoom out"),
                ("W / ↑", "Reset zoom to 100%"),
                ("S / ↓ / 0", "Fit image to window"),
                ("Ctrl+Alt+W", "Wrap window around the current image"),
                ("Scroll", "Navigate images; Ctrl + scroll zooms"),
                ("Double-left-click", "Fit image to window"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Display & general",
            "help_display",
            &[
                ("F11 / F / Enter", "Toggle fullscreen"),
                ("F5", "Open Settings"),
                ("T / F3", "Toggle file sidebar"),
                ("F1", "Open this help window"),
                ("Escape", "Exit fullscreen / Close window"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Workspace tabs",
            "help_workspace_tabs",
            &[
                ("Ctrl+PageUp / PageDown", "Switch to previous / next tab"),
                ("Ctrl+Shift+PageUp / PageDown", "Move active tab left / right"),
                ("Ctrl+Shift+Left / Right", "Alternative tab-reordering shortcuts"),
                ("Drag tab", "Reorder workspace tabs, as in Firefox"),
                ("+", "Choose an image for a separate workspace tab"),
            ],
        );
        ui.label(
            egui::RichText::new(
                "Home is part of the primary Viewer workspace. The + button is in the navigation overlay. When enabled, workspace tabs share the custom title-bar row; in fullscreen they return to the main view.",
            )
            .small()
            .weak(),
        );
        Self::render_shortcut_group(
            ui,
            "GIF controls",
            "help_gif",
            &[
                ("Space", "Play / Pause GIF animation"),
                ("[", "Slow down GIF (0.5x speed)"),
                ("]", "Speed up GIF (2x speed)"),
                ("P", "Reset GIF speed to 1x"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Slideshow",
            "help_slideshow",
            &[
                ("L", "Toggle slideshow on/off"),
                (",", "Slower speed (longer interval)"),
                (".", "Faster speed (shorter interval)"),
                ("Settings", "Choose interval, loop, transition, motion, and strength"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Collections & editing",
            "help_collections",
            &[
                ("Sidebar", "Browse List, Queue, Favorites, or Info"),
                ("Queue", "Build and save a viewing sequence as a .jvq file"),
                ("Favorites", "Keep a persistent, reorderable image list"),
                ("Edit mode", "Crop, rotate, flip, adjust color, undo/redo, and export"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "File management",
            "help_file_management",
            &[
                ("Ctrl+O", "Open file dialog"),
                ("Ctrl+Shift+O", "Open folder dialog"),
                ("Delete", "Move current image to trash"),
                ("Ctrl+C", "Copy image to clipboard"),
                ("Ctrl+Shift+C", "Copy file path to clipboard"),
            ],
        );
    }

    fn render_mouse_help(ui: &mut egui::Ui) {
        ui.add_space(2.0);

        Self::render_shortcut_group(
            ui,
            "Image navigation & zoom",
            "help_mouse_navigation",
            &[
                ("Scroll", "Navigate between images"),
                ("Ctrl + Scroll", "Zoom in / out"),
                ("Right Drag", "Zoom in / out"),
                ("Double-click Right", "Fit image to window"),
                ("Touchpad scroll", "Navigate images; Ctrl + scroll zooms unless inverted in Settings"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Pointer & window",
            "help_mouse_window",
            &[
                ("Left Drag on image", "Pan image (cursor: grab / grabbing)"),
                ("Left Drag outside image", "Move window in windowed mode"),
                ("Double-click Left", "Open file dialog (on empty screen)"),
                ("Middle Click", "Toggle fullscreen"),
                ("Alt + Left Drag", "Move window from anywhere in the application (toggle in General settings)"),
                ("Shift + Left Drag", "Resize the nearest window corner from anywhere in the application (toggle in General settings)"),
                ("Custom frame corners", "Resize the window when native decorations are disabled"),
            ],
        );
        Self::render_shortcut_group(
            ui,
            "Sidebar & lists",
            "help_mouse_lists",
            &[
                ("Sidebar thumbnails", "Select to open; drag to reorder Queue/Favorites entries"),
                ("Entry number", "Enter a position to move the item in supported lists"),
            ],
        );
    }

    fn render_help_tips(ui: &mut egui::Ui) {
        ui.add_space(2.0);

        let tips = [
            "Drag an image, folder, or supported archive into the window to open it.",
            "Use File → Open Folder + Subfolders to include images from every nested folder.",
            "Settings → General → Input can make folder drag-and-drop include all subfolders.",
            "Use Settings → Look to choose Automatic, JPEG Viewer, COSMIC, GNOME / Adwaita, or Windows 98 styling.",
            "Windows 98 look includes an optional dial-up-style image reveal and adjustable connection speed.",
            "OS/DE blur or glass depends on compositor support; it can be disabled for fullscreen.",
            "Disable native window decorations to use the custom frame and its resize edges/corners.",
            "Use Remap missing root when a saved queue's folder tree has moved.",
            "Pass one or more file paths on launch: jpeg_viewer image.jpg image2.png.",
            "Enable Settings → General → Startup → Run only one instance to forward later launches to the existing window; restart after changing it.",
            "Select multiple files in your file manager to open that selection without indexing neighboring files when single-instance mode is enabled.",
            "Double-left-click the image to fit it to the view. Panning keeps at least 20% of the image visible.",
            "Settings → Viewing controls image padding and automatic window sizing; View → Wrap Window Around Image (Ctrl+Alt+W) performs a one-time fit.",
            "If the file extension does not match the detected format, the toolbar may offer a rename suggestion.",
            "Very large images can use tiled virtual textures; tune Advanced settings if memory use is high.",
        ];

        for (index, tip) in tips.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:02}", index + 1))
                        .monospace()
                        .small()
                        .strong()
                        .color(ui.visuals().hyperlink_color),
                );
                ui.label(egui::RichText::new(*tip).size(13.0));
            });
            ui.add_space(4.0);
        }
    }

    fn render_shortcut_group(
        ui: &mut egui::Ui,
        title: &str,
        id: &str,
        rows: &[(&str, &str)],
    ) {
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(7))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(title).strong().size(13.5));
                ui.add_space(2.0);
                let key_width = (ui.available_width() * 0.34).clamp(92.0, 150.0);
                egui::Grid::new(id)
                    .num_columns(2)
                    .spacing([8.0, 3.0])
                    .show(ui, |ui| {
                        for (key, description) in rows {
                            ui.add_sized(
                                [key_width, 0.0],
                                egui::Label::new(
                                    egui::RichText::new(*key)
                                        .strong()
                                        .size(12.5)
                                        .color(ui.visuals().hyperlink_color),
                                )
                                .wrap(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(*description).size(12.5),
                                )
                                .wrap(),
                            );
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(5.0);
    }

    pub fn toggle_help_menu(&mut self, ctx: &egui::Context) {
        if !self.show_help_menu {
            crate::app::ui::bump_help_window_generation(ctx);
        }
        self.show_help_menu = !self.show_help_menu;
    }
}

fn open_external(url: &str, label: &str) {
    if let Err(error) = open::that(url) {
        eprintln!("Could not open {label}: {error}");
    }
}
