use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help_menu;
        let close_requested = false;
        let content_rect = ctx.available_rect();
        let default_size = egui::vec2(512.0, 512.0);
        let default_pos = egui::pos2(
            (content_rect.center().x - default_size.x * 0.5).max(content_rect.left()),
            (content_rect.center().y - default_size.y * 0.5).max(content_rect.top()),
        );

        let generation = crate::app::ui::desktop_menu_bar_generation_for_help(ctx);
        egui::Window::new("Help - JPEG Viewer")
            .id(egui::Id::new(("jpeg_viewer_help_window", generation)))
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .movable(true)
            .default_pos(default_pos)
            .default_size(default_size)
            .min_size([400.0, 400.0])
            .max_size([700.0, 800.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // Keep the application version pinned to the help window's bottom edge.
                const FOOTER_HEIGHT: f32 = 28.0;
                let scroll_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(scroll_height)
                    .show(ui, |ui| {
                        ui.add_space(8.0);

                        // ========== HEADER ==========
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("🖼 JPEG Viewer").size(28.0).strong());
                            ui.label(egui::RichText::new("A fast, lightweight image viewer").size(16.0));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("By Mobin Sasanpour").size(13.0).italics());
                            ui.label(egui::RichText::new("@artofweekend").size(13.0).italics().color(ui.visuals().hyperlink_color));

                            // Social links
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);

                                // Use the theme's contrast-checked hyperlink color on all surfaces.
                                let link_color = ui.visuals().hyperlink_color;

                                // Instagram
                                let instagram_text = egui::RichText::new("📸  Instagram")
                                    .size(13.0)
                                    .color(link_color);

                                let instagram_link = ui.link(instagram_text);

                                if instagram_link.clicked() {
                                    if let Err(err) = open::that("https://instagram.com/artofweekend") {
                                        eprintln!("Failed to open Instagram: {}", err);
                                    }
                                }

                                ui.add_space(16.0);

                                // X (Twitter)
                                let x_text = egui::RichText::new("𝕏 X (Twitter)")
                                    .size(13.0)
                                    .color(link_color);

                                let x_link = ui.link(x_text);

                                if x_link.clicked() {
                                    if let Err(err) = open::that("https://x.com/artofweekend") {
                                        eprintln!("Failed to open X: {}", err);
                                    }
                                }

                                ui.add_space(16.0);

                                // GitHub
                                let github_text = egui::RichText::new("💻  GitHub")
                                    .size(13.0)
                                    .color(link_color);

                                let github_link = ui.link(github_text);

                                if github_link.clicked() {
                                    if let Err(err) = open::that("https://github.com/MobinWeekend/Jpeg-Viewer") {
                                        eprintln!("Failed to open GitHub: {}", err);
                                    }
                                }

                                ui.add_space(8.0);
                            });
                        });

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // ========== FEATURES ==========
                        ui.label(egui::RichText::new("Features").size(18.0).strong());
                        ui.add_space(8.0);

                        // some feel stupid to boast about and considering I want the user to quickly see the keyboard shortcuts, I commented most out :)
                        let features = vec![
                            "• Browse common raster, animated, vector, and camera RAW formats",
                            "• Open images inside ZIP, 7z, and RAR archives without extracting them",
                            "• Browse, Queue, Favorites, and Info sidebar tabs",
                            "• Save and load .jvq queues; reorder queue and favorite entries",
                            "• Edit supported raster files: crop, rotate, flip, adjust color, undo/redo, and export",
                            "• GIF playback controls and configurable slideshow transitions and motion",
                            "• Desktop looks for JPEG Viewer, COSMIC, GNOME / Adwaita, and Windows 98",
                            "• UI scaling, custom accents, transparency checkerboard, and optional OS/DE glass",
                            "• Background decoding, preloading, cache controls, and tiled rendering for very large images",
                            "• File actions include Open With, locate, rename, copy path, and trash where supported",
                        ];

                        for feature in features {
                            ui.label(egui::RichText::new(feature).size(13.0));
                            ui.add_space(2.0);
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // ========== KEYBOARD SHORTCUTS ==========
                        ui.label(egui::RichText::new("Keyboard Shortcuts").size(18.0).strong());
                        ui.add_space(8.0);

                        // Navigation
                        ui.label(egui::RichText::new("Navigation").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "◀ / ▶", "Previous / Next image");
                        Self::render_shortcut_row(ui, "A / D", "Previous / Next image");
                        Self::render_shortcut_row(ui, "Home", "Jump to first image");
                        Self::render_shortcut_row(ui, "End", "Jump to last image");
                        Self::render_shortcut_row(ui, "Ctrl+◀ / Ctrl+▶", "Jump to first / last image");

                        ui.add_space(4.0);

                        // Zoom & View
                        ui.label(egui::RichText::new("Zoom & View").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "+ / -", "Zoom in / Zoom out");
                        Self::render_shortcut_row(ui, "W / ↑", "Reset zoom to 100%");
                        Self::render_shortcut_row(ui, "S / ↓ / 0", "Fit image to window");
                        Self::render_shortcut_row(ui, "Scroll", "Navigate images (with Ctrl: Zoom)");

                        ui.add_space(4.0);

                        // Display
                        ui.label(egui::RichText::new("Display").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "F11 / F / Enter", "Toggle fullscreen");
                        Self::render_shortcut_row(ui, "Tab", "Open settings menu");
                        Self::render_shortcut_row(ui, "F1", "Open this help window");

                        ui.add_space(4.0);

                        // GIF Controls
                        ui.label(egui::RichText::new("GIF Controls").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "Space", "Play / Pause GIF animation");
                        Self::render_shortcut_row(ui, "[", "Slow down GIF (0.5x speed)");
                        Self::render_shortcut_row(ui, "]", "Speed up GIF (2x speed)");
                        Self::render_shortcut_row(ui, "P", "Reset GIF speed to 1x");

                        ui.add_space(4.0);

                        // Slideshow
                        ui.label(egui::RichText::new("Slideshow").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "L", "Toggle slideshow on/off");
                        Self::render_shortcut_row(ui, ",", "Slideshow - slower speed (longer interval)");
                        Self::render_shortcut_row(ui, ".", "Slideshow - faster speed (shorter interval)");
                        Self::render_shortcut_row(ui, "Settings", "Choose interval, loop, transition, motion, and motion strength");

                        ui.add_space(4.0);

                        // Editing and collections
                        ui.label(egui::RichText::new("Collections & Editing").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "Sidebar", "Browse the current list, Queue, Favorites, or Info");
                        Self::render_shortcut_row(ui, "Queue", "Build and save a viewing sequence as a .jvq file");
                        Self::render_shortcut_row(ui, "Favorites", "Keep a persistent, reorderable image list");
                        Self::render_shortcut_row(ui, "Edit mode", "Crop, rotate, flip, adjust color, undo/redo, and export supported files");

                        ui.add_space(4.0);

                        // File Management
                        ui.label(egui::RichText::new("File Management").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "Ctrl+O", "Open file dialog");
                        Self::render_shortcut_row(ui, "Ctrl+Shift+O", "Open folder dialog");
                        Self::render_shortcut_row(ui, "Delete", "Move current image to trash");
                        Self::render_shortcut_row(ui, "Ctrl+C", "Copy image to clipboard");
                        Self::render_shortcut_row(ui, "Ctrl+Shift+C", "Copy file path to clipboard");

                        ui.add_space(4.0);

                        // General
                        ui.label(egui::RichText::new("General").size(14.0).strong());
                        ui.add_space(4.0);
                        Self::render_shortcut_row(ui, "Escape", "Exit fullscreen / Close window");

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // ========== MOUSE SHORTCUTS ==========
                        ui.label(egui::RichText::new("Mouse Controls").size(18.0).strong());
                        ui.add_space(8.0);

                        Self::render_mouse_row(ui, "Scroll", "Navigate between images");
                        Self::render_mouse_row(ui, "Ctrl + Scroll", "Zoom in/out");
                        Self::render_mouse_row(ui, "Left Drag", "Pan image");
                        Self::render_mouse_row(ui, "Right Drag", "Zoom in/out");
                        Self::render_mouse_row(ui, "Double-click Left", "Open file dialog (on empty screen)");
                        Self::render_mouse_row(ui, "Middle Click", "Toggle fullscreen");
                        Self::render_mouse_row(ui, "Double-click Right", "Fit image to window");
                        Self::render_mouse_row(ui, "Alt + Left Drag", "Move window");
                        Self::render_mouse_row(ui, "Ctrl + Left Drag", "Resize window");
                        Self::render_mouse_row(ui, "Touchpad scroll", "Navigate images; Ctrl + scroll zooms unless inverted in Settings");
                        Self::render_mouse_row(ui, "Sidebar thumbnails", "Select to open; drag to reorder Queue/Favorites entries");
                        Self::render_mouse_row(ui, "Entry number", "Enter a position to move the item in supported lists");
                        Self::render_mouse_row(ui, "Custom frame corners", "Resize the window when native decorations are disabled");

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // ========== TIPS ==========
                        ui.label(egui::RichText::new("💡 Tips").size(18.0).strong());
                        ui.add_space(8.0);

                        let tips = vec![
                            "• Drag an image, folder, or supported archive into the window to open it",
                            "• Use Settings → Look to choose Automatic, JPEG Viewer, COSMIC, GNOME / Adwaita, or Windows 98 styling",
                            "• Windows 98 look includes an optional dial-up-style image reveal and adjustable connection speed",
                            "• OS/DE blur or glass depends on compositor support; it can be disabled for fullscreen",
                            "• Disable native window decorations to use the custom frame and its resize edges/corners",
                            "• Use Remap missing root when a saved queue's folder tree has moved",
                            "• Pass a file path on launch: jpeg_viewer image.jpg",
                            "• If the file extension does not match the detected format, the toolbar may offer a rename suggestion",
                            "• Very large images can use tiled virtual textures; tune Advanced settings if memory use is high",
                        ];

                        for tip in tips {
                            ui.label(egui::RichText::new(tip).size(13.0));
                            ui.add_space(2.0);
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // ========== FOOTER ==========
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("Made with ♥ using Rust and egui").size(12.0));
                        });

                        ui.add_space(8.0);
                    });
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .size(12.0)
                            .weak(),
                    );
                });
            });

        // Update the actual state after the window closes
        if close_requested {
            self.show_help_menu = false;
        } else {
            self.show_help_menu = open;
        }
    }

    pub fn toggle_help_menu(&mut self, ctx: &egui::Context) {
        if !self.show_help_menu {
            crate::app::ui::bump_help_window_generation(ctx);
        }
        self.show_help_menu = !self.show_help_menu;
    }

    // Helper function to render a shortcut row
    fn render_shortcut_row(ui: &mut egui::Ui, key: &str, description: &str) {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(key).strong().color(ui.visuals().hyperlink_color));
            ui.add_space(8.0);
            ui.label("=");
            ui.add_space(8.0);
            ui.label(description);
        });
        ui.add_space(2.0);
    }

    // Helper function to render a mouse control row
    fn render_mouse_row(ui: &mut egui::Ui, control: &str, action: &str) {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(control).strong().color(ui.visuals().hyperlink_color));
            ui.add_space(8.0);
            ui.label("=");
            ui.add_space(8.0);
            ui.label(action);
        });
        ui.add_space(2.0);
    }
}
