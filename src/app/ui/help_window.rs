use crate::app::types::ViewerApp;
use eframe::egui;

const PROJECT_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer";
const RELEASES_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer/releases";
const INSTAGRAM_URL: &str = "https://www.instagram.com/artofweekend/";
const X_URL: &str = "https://x.com/ArtOfWeekend";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
enum HelpTab {
    #[default]
    Overview,
    Keyboard,
    Mouse,
    Tips,
    CargoVersions,
    About,
}

fn is_keyboard_key_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }

    let normalized = token.to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "ctrl"
            | "control"
            | "shift"
            | "alt"
            | "super"
            | "meta"
            | "win"
            | "cmd"
            | "command"
            | "option"
            | "home"
            | "end"
            | "pageup"
            | "pagedown"
            | "enter"
            | "return"
            | "space"
            | "escape"
            | "esc"
            | "delete"
            | "backspace"
            | "tab"
            | "insert"
            | "capslock"
            | "printscreen"
            | "pause"
            | "◀"
            | "▶"
            | "←"
            | "→"
            | "↑"
            | "↓"
    ) {
        return true;
    }

    if let Some(function_number) = normalized.strip_prefix('f') {
        if let Ok(number) = function_number.parse::<u8>() {
            if (1..=24).contains(&number) {
                return true;
            }
        }
    }

    // Single letters, digits, and punctuation represent physical keys too.
    token.chars().count() == 1
}

fn render_keycap(ui: &mut egui::Ui, key: &str) {
    let visuals = ui.visuals();
    let fill = if visuals.widgets.inactive.bg_fill == egui::Color32::TRANSPARENT {
        visuals.faint_bg_color
    } else {
        visuals.widgets.inactive.bg_fill
    };
    let outline = visuals.widgets.noninteractive.bg_stroke;

    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(outline.width.max(0.8), outline.color))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |key_ui| {
            key_ui.label(
                egui::RichText::new(key)
                    .monospace()
                    .strong()
                    .size(11.5)
                    .color(key_ui.visuals().text_color()),
            );
        });
}

fn render_shortcut_label(ui: &mut egui::Ui, shortcut: &str, key_width: f32) {
    // A bare plus is itself a shortcut key, not a combination separator.
    let contains_keycap = shortcut.trim() == "+"
        || shortcut
            .split(|character| character == '/' || character == '+')
            .any(is_keyboard_key_token);

    if !contains_keycap {
        ui.add_sized(
            [key_width, 0.0],
            egui::Label::new(
                egui::RichText::new(shortcut)
                    .strong()
                    .size(12.5)
                    .color(ui.visuals().hyperlink_color),
            )
            .wrap(),
        );
        return;
    }

    let estimated_width = shortcut
        .split(|character| character == '/' || character == '+')
        .map(str::trim)
        .map(|token| {
            let token_width = token.chars().count() as f32 * 6.8 + 8.0;
            token_width
        })
        .sum::<f32>()
        + shortcut.matches('+').count() as f32 * 7.0
        + shortcut.matches('/').count() as f32 * 7.0;
    let row_height = if estimated_width > key_width { 40.0 } else { 24.0 };

    ui.allocate_ui_with_layout(
        egui::vec2(key_width, row_height),
        egui::Layout::top_down(egui::Align::Min),
        |key_ui| {
            key_ui.spacing_mut().item_spacing.x = 2.0;
            key_ui.horizontal_wrapped(|row| {
                row.spacing_mut().item_spacing.x = 2.0;
                if shortcut.trim() == "+" {
                    render_keycap(row, "+");
                    return;
                }

                for (alternative_index, alternative) in shortcut.split('/').enumerate() {
                    if alternative_index > 0 {
                        row.label(
                            egui::RichText::new("/")
                                .size(10.5)
                                .weak(),
                        );
                    }

                    for (token_index, token) in alternative.split('+').enumerate() {
                        let token = token.trim();
                        if token.is_empty() {
                            continue;
                        }
                        if token_index > 0 {
                            row.label(
                                egui::RichText::new("+")
                                    .size(10.5)
                                    .weak(),
                            );
                        }
                        if is_keyboard_key_token(token) {
                            render_keycap(row, token);
                        } else {
                            row.label(
                                egui::RichText::new(token)
                                    .size(11.0)
                                    .weak(),
                            );
                        }
                    }
                }
            });
        },
    );
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
    let width = ui.available_width().max(galley.size().x + 43.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 36.0), egui::Sense::click());

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
        // A vertical marker reinforces that this is a left-side section list,
        // not a horizontal tab strip.
        let indicator = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, rect.top() + 7.0),
            egui::vec2(2.0, rect.height() - 14.0),
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
        if !self.show_help_menu {
            return;
        }
        let generation = crate::app::ui::desktop_menu_bar_generation_for_help(ctx);
        let viewport_id = egui::ViewportId::from_hash_of(("jpeg_viewer_help_window", generation));
        let default_size = egui::vec2(900.0, 650.0);
        let initialized_id = egui::Id::unique(("jpeg_viewer_help_viewport_initialized", generation));
        let initialized = ctx.data(|data| data.get_temp::<bool>(initialized_id).unwrap_or(false));
        let mut builder = egui::ViewportBuilder::default()
            .with_app_id("com.jpegviewer.JpegViewer")
            .with_icon(crate::application_window_icon_data())
            .with_title("Help & About · JPEG Viewer")
            .with_min_inner_size([600.0, 450.0])
            .with_resizable(true)
            .with_decorations(true)
            // Help is an opaque utility window, independent of the main
            // viewer's glass/backdrop configuration.
            .with_transparent(false)
            .with_clamp_size_to_monitor_size(true);
        if !initialized {
            // Supply geometry only once so users can freely move and resize the
            // native window without the builder resetting it on the next frame.
            builder = builder
                .with_inner_size(default_size)
                .with_position(crate::app::settings_menu::centered_on_main_monitor(ctx, default_size))
                .with_active(true);
            ctx.data_mut(|data| data.insert_temp(initialized_id, true));
        }
        let mut close_requested = false;
        let mut close_button_clicked = false;
        let tab_id = egui::Id::unique(("jpeg_viewer_help_tab", generation));
        let mut active_tab = ctx
            .data(|data| data.get_temp::<HelpTab>(tab_id))
            .unwrap_or_default();
        let previous_tab = active_tab;

        ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
                let viewport_ctx = ui.ctx().clone();
                if viewport_ctx.input(|input| input.viewport().close_requested()) {
                    close_requested = true;
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }

                // Keep Help fully opaque even while the root viewport clears
                // transparently for the main window's backdrop or Pop-outs.
                let [r, g, b, _] = ui.visuals().panel_fill.to_srgba_unmultiplied();
                ui.painter().rect_filled(
                    ui.max_rect(),
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgb(r, g, b),
                );

                // Give Help a consistent top and side inset. Keep the existing
                // footer/bottom spacing unchanged.
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 16,
                        bottom: 0,
                    })
                    .show(ui, |ui| {
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
                            crate::app::ui::paint_application_icon(header, logo_rect);
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
                                    if meta
                                        .add(egui::Button::new("×").min_size(egui::vec2(30.0, 30.0)))
                                        .on_hover_text("Close Help")
                                        .clicked()
                                    {
                                        close_button_clicked = true;
                                        meta.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
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
                ui.separator();
                ui.add_space(8.0);

                let body_height = (height - 105.0).max(180.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, body_height),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |body| {
                        let nav_width = 184.0_f32.min((body.available_width() * 0.34).max(150.0));
                        egui::Frame::new()
                            .fill(body.visuals().extreme_bg_color)
                            .stroke(egui::Stroke::new(0.7, border))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin::same(8))
                            .show(body, |nav| {
                                // The content area is laid out horizontally;
                                // use a top-down child layout for the left nav.
                                nav.with_layout(egui::Layout::top_down(egui::Align::Min), |nav| {
                                    nav.set_min_width(nav_width);
                                    nav.set_max_width(nav_width);
                                    nav.label(egui::RichText::new("HELP & SUPPORT").small().strong().weak());
                                    nav.add_space(7.0);
                                    help_nav_item(nav, &mut active_tab, HelpTab::Overview, crate::app::ui::SvgIcon::Home, "Overview");
                                    help_nav_item(nav, &mut active_tab, HelpTab::Keyboard, crate::app::ui::SvgIcon::Input, "Keyboard");
                                    help_nav_item(nav, &mut active_tab, HelpTab::Mouse, crate::app::ui::SvgIcon::Inspect, "Mouse & touch");
                                    help_nav_item(nav, &mut active_tab, HelpTab::Tips, crate::app::ui::SvgIcon::Image, "Tips & workflows");
                                    nav.add_space(7.0);
                                    nav.separator();
                                    nav.add_space(5.0);
                                    help_nav_item(nav, &mut active_tab, HelpTab::CargoVersions, crate::app::ui::SvgIcon::SettingsFile, "Cargo versions");
                                    help_nav_item(nav, &mut active_tab, HelpTab::About, crate::app::ui::SvgIcon::AppLogo, "About JPEG Viewer");
                                });
                            });

                        body.add_space(12.0);
                        body.vertical(|content| {
                            let content_width = content.available_width();
                            content.set_min_width(content_width);
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
                                HelpTab::About => (
                                    "About JPEG Viewer",
                                    "A GPU-accelerated Rust image viewer focused on a clean, responsive viewing experience.",
                                ),
                            };
                            content.label(egui::RichText::new(heading).size(19.0).strong());
                            content.label(egui::RichText::new(subtitle).size(12.5).color(weak_text));
                            content.add_space(8.0);
                            egui::Frame::new()
                                .fill(content.visuals().extreme_bg_color)
                                .stroke(egui::Stroke::new(0.7, border))
                                .corner_radius(egui::CornerRadius::same(9))
                                .inner_margin(egui::Margin::same(13))
                                .show(content, |panel| {
                                    egui::ScrollArea::vertical()
                                        .id_salt(("jpeg_viewer_help_content", generation, active_tab))
                                        .auto_shrink([false, false])
                                        .show(panel, |article| match active_tab {
                                            HelpTab::Overview => Self::render_help_overview(article),
                                            HelpTab::Keyboard => Self::render_keyboard_help(article),
                                            HelpTab::Mouse => Self::render_mouse_help(article),
                                            HelpTab::Tips => Self::render_help_tips(article),
                                            HelpTab::CargoVersions => Self::render_cargo_versions(article),
                                            HelpTab::About => Self::render_about(article),
                                        });
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
            });

        if close_button_clicked {
            close_requested = true;
        }

        if active_tab != previous_tab {
            ctx.data_mut(|data| data.insert_temp(tab_id, active_tab));
        }
        if close_requested {
            self.show_help_menu = false;
            ctx.data_mut(|data| data.remove::<bool>(initialized_id));
        }
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
                    "Press F5 to open Settings. Press F1 in the viewer to open Help & About.",
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

    fn render_about(ui: &mut egui::Ui) {
        ui.add_space(2.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(13))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (logo_rect, _) = ui.allocate_exact_size(
                        egui::vec2(42.0, 42.0),
                        egui::Sense::hover(),
                    );
                    crate::app::ui::paint_application_icon(ui, logo_rect);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("JPEG Viewer").size(17.0).strong());
                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).monospace().weak());
                        ui.label(egui::RichText::new("by Mobin Sasanpour").small().weak());
                    });
                });
                ui.add_space(8.0);
                ui.label("A GPU-accelerated image viewer built with Rust and egui. Designed for browsing image collections with responsive performance, practical controls, and a focused interface.");
            });
        ui.add_space(10.0);
        ui.label(egui::RichText::new("PROJECT").small().strong().color(ui.visuals().weak_text_color()));
        ui.label("Source code, releases, issue tracking, and project contributions are hosted on GitHub.");
        ui.add_space(7.0);
        ui.horizontal_wrapped(|ui| {
            for (label, url) in [("GitHub repository ↗", PROJECT_URL), ("Download releases ↗", RELEASES_URL)] {
                let response = ui.add(egui::Button::new(label));
                if response.clicked() {
                    open_external(url, label);
                }
            }
        });
        ui.add_space(10.0);
        ui.label(egui::RichText::new("SOCIAL").small().strong().color(ui.visuals().weak_text_color()));
        ui.horizontal_wrapped(|ui| {
            for (label, url) in [("Instagram ↗", INSTAGRAM_URL), ("X (Twitter) ↗", X_URL)] {
                let response = ui.add(egui::Button::new(label));
                if response.clicked() {
                    open_external(url, label);
                }
            }
        });
        ui.add_space(10.0);
        ui.label(egui::RichText::new("CREDITS").small().strong().color(ui.visuals().weak_text_color()));
        ui.label("Created by Mobin Sasanpour. Built with the Rust ecosystem, eframe, and egui, alongside the image decoding and platform libraries listed in the project manifests.");
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
                ("F2", "Rename current filesystem image"),
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
                let key_width = (ui.available_width() * 0.42).clamp(112.0, 220.0);
                egui::Grid::new(id)
                    .num_columns(2)
                    .spacing([8.0, 3.0])
                    .show(ui, |ui| {
                        for (key, description) in rows {
                            render_shortcut_label(ui, key, key_width);
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
        if self.show_help_menu {
            let generation = crate::app::ui::desktop_menu_bar_generation_for_help(ctx);
            let id = egui::ViewportId::from_hash_of(("jpeg_viewer_help_window", generation));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
            ctx.request_repaint();
            return;
        }
        crate::app::ui::bump_help_window_generation(ctx);
        self.show_help_menu = true;
        ctx.request_repaint();
    }
}

fn open_external(url: &str, label: &str) {
    if let Err(error) = open::that(url) {
        eprintln!("Could not open {label}: {error}");
    }
}
