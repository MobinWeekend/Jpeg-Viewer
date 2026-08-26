use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub fn render_help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help_menu;
        let close_requested = false;

        egui::Window::new("Help - JPEG Viewer")
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .default_size([512.0, 512.0])
            .min_size([400.0, 400.0])
            .max_size([700.0, 800.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(8.0);

                        // ========== HEADER ==========
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📷 JPEG Viewer").size(28.0).strong());
                            ui.label(egui::RichText::new("A fast, lightweight image viewer").size(16.0));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("By Mobin Sasanpour").size(13.0).italics());
                            ui.label(egui::RichText::new("@artofweekend").size(13.0).italics().color(egui::Color32::LIGHT_BLUE));

                            // Social links
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);

                                // Theme-aware link colors
                                let dark_mode = ui.visuals().dark_mode;

                                let instagram_color = if dark_mode {
                                    egui::Color32::from_rgb(255, 90, 140)
                                } else {
                                    egui::Color32::from_rgb(190, 45, 105)
                                };

                                let x_color = if dark_mode {
                                    egui::Color32::from_rgb(220, 220, 220)
                                } else {
                                    egui::Color32::from_rgb(60, 60, 60)
                                };

                                let github_color = if dark_mode {
                                    egui::Color32::from_rgb(130, 180, 230)
                                } else {
                                    egui::Color32::from_rgb(55, 105, 160)
                                };

                                // Instagram
                                let instagram_text = egui::RichText::new("📸  Instagram")
                                    .size(13.0)
                                    .color(instagram_color);

                                let instagram_link = ui.link(instagram_text);

                                if instagram_link.clicked() {
                                    if let Err(err) = open::that("https://instagram.com/artofweekend") {
                                        eprintln!("Failed to open Instagram: {}", err);
                                    }
                                }

                                ui.add_space(16.0);

                                // X (Twitter)
                                let x_text = egui::RichText::new("🐦  X (Twitter)")
                                    .size(13.0)
                                    .color(x_color);

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
                                    .color(github_color);

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
                            "• View images from your computer, folders, and archives (.zip, .7z, .rar)",
                            "• Support for all major image formats (JPEG, PNG, GIF, WebP, BMP, TIFF, and more)",
                            "• Full GIF animation support with playback controls and speed adjustment",
                            //"• Slideshow mode with adjustable timing, loop, and random order options",
                            "• Smart image caching for smooth navigation through large image collections",
                            //"• Preloading of adjacent images for instant switching",
                            //"• Zoom, pan, and fit-to-window controls",
                            //"• Fullscreen mode for distraction-free viewing",
                            //"• Drag and drop support for images and folders",
                            "• Archive scanning without extracting files",
                            //"• Delete images to trash/recycle bin",
                            //"• Adjustable frame limiter for optimal performance and battery life",
                            //"• Automatic file type detection and rename suggestion",
                            "• Support for extremely large images using tiled virtual texturing",
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

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // ========== TIPS ==========
                        ui.label(egui::RichText::new("💡 Tips").size(18.0).strong());
                        ui.add_space(8.0);

                        let tips = vec![
                            "• Drag and drop images, folders, or archive files directly into the window",
                            "• The app remembers your window position and settings between sessions",
                            "• Adjust cache radius in settings for smoother navigation on large collections",
                            "• GIFs load a preview frame first, then the full animation in the background",
                            "• Use the settings menu to customize keyboard shortcuts and performance",
                            "• Start the app with an image path as an argument: jpeg_viewer image.jpg",
                            "• If the app detects a mismatch between file extension and actual content, a rename suggestion will appear in the toolbar.",
                            "• Very large images are rendered using tiled virtual textures for smooth performance.",
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
            });

        // Update the actual state after the window closes
        if close_requested {
            self.show_help_menu = false;
        } else {
            self.show_help_menu = open;
        }
    }

    pub fn toggle_help_menu(&mut self) {
        self.show_help_menu = !self.show_help_menu;
    }

    // Helper function to render a shortcut row
    fn render_shortcut_row(ui: &mut egui::Ui, key: &str, description: &str) {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.colored_label(egui::Color32::LIGHT_GREEN, key);
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
            ui.colored_label(egui::Color32::LIGHT_BLUE, control);
            ui.add_space(8.0);
            ui.label("=");
            ui.add_space(8.0);
            ui.label(action);
        });
        ui.add_space(2.0);
    }
}
