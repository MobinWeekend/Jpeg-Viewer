use crate::app::types::ViewerApp;
use eframe::egui;

const PROJECT_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer";
const RELEASES_URL: &str = "https://github.com/MobinWeekend/Jpeg-Viewer/releases";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum HelpTab {
    #[default]
    Overview,
    Keyboard,
    Mouse,
    Tips,
}

impl ViewerApp {
    pub fn render_help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help_menu;
        let content_rect = ctx.available_rect();
        // Keep the window comfortable on normal displays, but allow its content to
        // remain usable in a compact 512x512 window.
        let default_size = egui::vec2(
            content_rect.width().clamp(512.0, 760.0),
            content_rect.height().clamp(512.0, 720.0),
        );
        let default_pos = egui::pos2(
            (content_rect.center().x - default_size.x * 0.5).max(content_rect.left()),
            (content_rect.center().y - default_size.y * 0.5).max(content_rect.top()),
        );

        let generation = crate::app::ui::desktop_menu_bar_generation_for_help(ctx);
        let tab_id = egui::Id::new(("jpeg_viewer_help_tab", generation));
        let mut active_tab = ctx
            .data(|data| data.get_temp::<HelpTab>(tab_id))
            .unwrap_or_default();
        let previous_tab = active_tab;

        egui::Window::new("Help & About · JPEG Viewer")
            .id(egui::Id::new(("jpeg_viewer_help_window", generation)))
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .movable(true)
            .default_pos(default_pos)
            .default_size(default_size)
            .min_size([512.0, 512.0])
            .max_size([960.0, 960.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // Compact identity block: keep navigation and reference content visible
                // without a large hero panel consuming the small window's height.
                egui::Frame::group(ui.style())
                    .fill(ui.visuals().faint_bg_color)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("🖼 JPEG Viewer")
                                        .size(19.0)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new("Fast, lightweight image browsing · By Mobin Sasanpour")
                                        .small()
                                        .weak(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "v{}",
                                            env!("CARGO_PKG_VERSION")
                                        ))
                                        .strong()
                                        .color(ui.visuals().hyperlink_color),
                                    );
                                },
                            );
                        });
                        ui.add_space(3.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("GitHub ↗").clicked() {
                                open_external(PROJECT_URL, "GitHub");
                            }
                            if ui.small_button("Releases ↗").clicked() {
                                open_external(RELEASES_URL, "GitHub Releases");
                            }
                            if ui.link("Instagram").clicked() {
                                open_external("https://instagram.com/artofweekend", "Instagram");
                            }
                            if ui.link("X").clicked() {
                                open_external("https://x.com/artofweekend", "X");
                            }
                        });
                    });

                ui.add_space(5.0);
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut active_tab,
                        HelpTab::Overview,
                        egui::RichText::new("About").size(12.5),
                    ).on_hover_text("Features and getting started");
                    ui.selectable_value(
                        &mut active_tab,
                        HelpTab::Keyboard,
                        egui::RichText::new("⌨ Keyboard").size(12.5),
                    ).on_hover_text("Keyboard shortcuts");
                    ui.selectable_value(
                        &mut active_tab,
                        HelpTab::Mouse,
                        egui::RichText::new("🖱 Mouse").size(12.5),
                    ).on_hover_text("Mouse, touchpad, and window controls");
                    ui.selectable_value(
                        &mut active_tab,
                        HelpTab::Tips,
                        egui::RichText::new("Tips").size(12.5),
                    ).on_hover_text("Useful workflows and settings");
                });
                ui.separator();

                // Reserve only a small footer; the selected section gets the remaining
                // height and can scroll independently from the other tabs.
                const FOOTER_HEIGHT: f32 = 22.0;
                let scroll_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt(("jpeg_viewer_help_content", generation, active_tab))
                    .auto_shrink([false; 2])
                    .max_height(scroll_height)
                    .show(ui, |ui| match active_tab {
                        HelpTab::Overview => Self::render_help_overview(ui),
                        HelpTab::Keyboard => Self::render_keyboard_help(ui),
                        HelpTab::Mouse => Self::render_mouse_help(ui),
                        HelpTab::Tips => Self::render_help_tips(ui),
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Made with ♥ using Rust and egui")
                            .small()
                            .weak(),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                    .small()
                                    .weak(),
                            );
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
        ui.add_space(3.0);
        ui.label(
            egui::RichText::new("A focused viewer for your image library")
                .size(17.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(
                "Browse, compare, organize, and enjoy images without turning the viewer into a heavy editing suite.",
            )
            .small()
            .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(6.0);
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

    fn render_keyboard_help(ui: &mut egui::Ui) {
        ui.add_space(3.0);
        ui.label(egui::RichText::new("Keyboard shortcuts").size(17.0).strong());
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
        ui.add_space(3.0);
        ui.label(egui::RichText::new("Mouse & touch controls").size(17.0).strong());
        ui.label(
            egui::RichText::new("A quick reference for navigation, zooming, panning, and window control.")
                .small()
                .weak(),
        );
        ui.add_space(5.0);

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
        ui.add_space(3.0);
        ui.label(egui::RichText::new("Tips & workflow").size(17.0).strong());
        ui.label(
            egui::RichText::new("Small settings and shortcuts that make day-to-day browsing smoother.")
                .small()
                .weak(),
        );
        ui.add_space(5.0);

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
