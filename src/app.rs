use crate::settings::SettingsManager;
use crate::archive::{scan_zip, scan_7z, scan_rar};
use crate::image_entry::ImageEntry;
use eframe::egui;
use image::DynamicImage;
use rayon::spawn;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};


pub struct ViewerApp {
    texture: Option<egui::TextureHandle>,
    receiver: Option<Receiver<DynamicImage>>,
    zoom: f32,
    pan: egui::Vec2,
    current_directory: Option<PathBuf>,
    image_entries: Vec<ImageEntry>,
    current_index: usize,
    is_loading: bool,
    is_fit_to_window: bool,
    image_rect: Option<egui::Rect>,
    last_window_size: Option<egui::Vec2>,
    is_zoom_used: bool,
    is_ctrl_invert: bool,
    settings_manager: SettingsManager,
}

impl Default for ViewerApp {
    fn default() -> Self {
        let settings_manager = SettingsManager::new();
        let settings = settings_manager.get().clone();

        Self {
            texture: None,
            receiver: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            current_directory: None,
            current_index: 0,
            is_loading: false,
            is_fit_to_window: false,
            image_rect: None,
            last_window_size: None,
            is_zoom_used: false,
            is_ctrl_invert: settings.is_ctrl_invert,
            settings_manager,
            image_entries: Vec::new(),
        }
    }
}

// ====== ViewerApp  ======
impl ViewerApp {
    fn load_image(&mut self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            self.current_directory = Some(parent.to_path_buf());

            let files = crate::loader::load_directory_images(parent);

            if let Some(index) = files.iter().position(|p| p == &path) {
                let entries = files.into_iter().map(ImageEntry::File).collect();

                self.set_image_entries(entries, index);
            }
        }
    }

    fn load_current_image(&mut self) {
        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        self.is_loading = true;

        let (tx, rx) = channel();

        spawn(move || {
            let image = match entry {
                ImageEntry::File(path) => crate::loader::load(path),
                ImageEntry::Zip(zip) => crate::loader::load_zip_image(zip),
                ImageEntry::S7z(s7z) => crate::loader::load_7z_image(s7z),
                ImageEntry::Rar(rar) => crate::loader::load_rar_image(rar),
            };

            if let Some(img) = image {
                let _ = tx.send(img);
            }
        });

        self.receiver = Some(rx);
    }

    fn navigate_images(&mut self, direction: i32) {
        if self.image_entries.is_empty() {
            return;
        }

        let len = self.image_entries.len();
        let new_index = (self.current_index as i32 + direction).rem_euclid(len as i32) as usize;

        if new_index != self.current_index {
            self.current_index = new_index;
            self.pan = egui::Vec2::ZERO;
            self.is_fit_to_window = true;
            self.image_rect = None;
            self.load_current_image();
        }
        self.is_zoom_used = false;
    }

    fn load_directory(&mut self, path: &PathBuf) {
        self.current_directory = Some(path.clone());
        let files = crate::loader::load_directory_images(path);
        if files.is_empty() {
            println!("No images found in directory: {:?}", path);
            return;
        }
        let entries = files
        .into_iter()
        .map(ImageEntry::File)
        .collect();

        self.set_image_entries(entries, 0);
        self.zoom = 1.0;
    }


    // handeling entries
    fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.image_entries = entries;
        self.current_index = current_index;
        self.pan = egui::Vec2::ZERO;
        self.is_fit_to_window = true;
        self.load_current_image();
    }
}

// ====== eframe  ======
impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // drag & drop
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());

        if !dropped_files.is_empty() {
            for file in dropped_files {
                if let Some(path) = file.path {
                    if path.is_dir() {
                        self.load_directory(&path);
                    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        match ext.to_ascii_lowercase().as_str() {
                            "zip" => {
                                self.set_image_entries(scan_zip(&path), 0);
                            }
                            "7z" => {
                                self.set_image_entries(scan_7z(&path), 0);
                            }
                            "rar" => {
                                self.set_image_entries(scan_rar(&path), 0);
                            }

                            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => {
                                self.load_image(path);
                            }

                            _ => {}
                        }
                    }
                }
            }
        }

        // check for window resize
        let current_size = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size());
        if let (Some(prev), Some(curr)) = (self.last_window_size, current_size) {
            if prev != curr && !self.is_zoom_used {
                self.is_fit_to_window = true;
            }
        }
        self.last_window_size = current_size;

        // Check for loaded image
        if let Some(rx) = &self.receiver {
            if let Ok(img) = rx.try_recv() {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());

                self.texture = Some(ctx.load_texture("image", color, Default::default()));

                // Auto-zoom to fit if image is larger than window or is_fit_to_window is true
                self.is_fit_to_window = true;
                self.pan = egui::Vec2::ZERO;
                self.is_loading = false;
                self.receiver = None;
            }
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["jpg", "jpeg", "png", "gif", "bmp", "webp"])
                        .pick_file()
                    {
                        self.is_zoom_used = false;
                        self.zoom = 1.0;
                        self.is_fit_to_window = true;
                        self.image_rect = None;

                        self.load_image(path);
                    }
                }

                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.01..=10.0)
                        .logarithmic(true)
                        .text("Zoom | "),
                );

                if ui
                    .checkbox(&mut self.is_ctrl_invert, "Invert Ctrl Scroll | ")
                    .changed()
                {
                    // Save the setting when changed
                    let _ = self.settings_manager.update(|settings| {
                        settings.is_ctrl_invert = self.is_ctrl_invert;
                    });
                }

                // Show current image info
                if let Some(entry) = self.image_entries.get(self.current_index) {
                    let name = match entry {
                        ImageEntry::File(path) => path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),

                        ImageEntry::Zip(zip) => zip.name.clone(),
                        ImageEntry::S7z(s7z) => s7z.name.clone(),
                        ImageEntry::Rar(rar) => rar.name.clone(),
                    };

                    ui.label(format!(
                        "{} ({}/{})",
                        name,
                        self.current_index + 1,
                        self.image_entries.len()
                    ));
                }
                if !self.is_ctrl_invert {
                    ui.label("Scroll to navigate | Zoom with ctrl + Scroll");
                } else {
                    ui.label(" ctrl + Scroll to navigate | Zoom with Scroll");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                let texture_size = texture.size_vec2();
                let image_available_size = ui.available_size();
                if self.is_fit_to_window {
                    // ^ when true, checks the image to see if it needs fiting inside the window
                    let texture_size = texture.size_vec2();
                    let zoom_x = image_available_size.x / texture_size.x;
                    let zoom_y = image_available_size.y / texture_size.y;
                    let fit_zoom = zoom_x.min(zoom_y).min(1.0); // Only zoom out if needed
                    self.zoom = fit_zoom;
                    self.is_fit_to_window = false;
                }

                let display_size = texture_size * self.zoom;
                //let image_fits =
                //    display_size.x <= image_available_size.x + 0.5 &&
                //    display_size.y <= image_available_size.y + 0.5;

                let viewport = ui.max_rect();

                let image_rect = egui::Rect::from_center_size(
                    viewport.center() + self.pan * self.zoom,
                    display_size,
                );

                ui.put(image_rect, egui::Image::new((texture.id(), display_size)));

                let response = ui.interact(
                    viewport,              // The area to interact with
                    ui.id().with("image"), // Unique ID for this interaction
                    egui::Sense::drag(),   // Only detect drag gestures
                );

                if response.dragged() {
                    self.pan += response.drag_delta() / self.zoom;
                    if let Some(texture) = &self.texture {
                        let texture_limit = (texture.size_vec2() / 2.0)
                            + ((ctx.available_rect().size() / self.zoom) / 4.0);
                        self.pan = self.pan.clamp(-texture_limit, texture_limit);
                    }
                }

                // Check for arrow key presses
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    self.navigate_images(-1); // Go to previous image
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    self.navigate_images(1); // Go to next image
                }

                let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
                if scroll_delta.y != 0.0 {
                    // Only navigate if the image fits OR if Shift is held AND the mouse is over the image
                    let mouse_over = response.hovered();
                    let click_toggle_key_pressed = ctx.input(|i| i.modifiers.ctrl);
                    //println!("Scroll: {}, Mouse over: {}, toggwle: {}, Image fits: {}",
                    //    scroll_delta.y, mouse_over, click_toggle_key_pressed, image_fits);
                    if mouse_over {
                        if (!click_toggle_key_pressed && !self.is_ctrl_invert)
                            || (click_toggle_key_pressed && self.is_ctrl_invert)
                        {
                            let direction = if scroll_delta.y > 0.0 { -1 } else { 1 };
                            self.navigate_images(direction);
                        } else {
                            let zoom_direction: f32 = if scroll_delta.y > 0.0 { 1.0 } else { -1.0 };
                            self.zoom += self.zoom * zoom_direction * 0.1;
                            self.zoom = self.zoom.clamp(0.01, 10.0);
                            if let Some(texture) = &self.texture {
                                let texture_limit = (texture.size_vec2() / 2.0)
                                    + ((ctx.available_rect().size() / self.zoom) / 4.0);
                                self.pan = self.pan.clamp(-texture_limit, texture_limit);
                            }
                            self.is_zoom_used = true;
                        }
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    if self.is_loading {
                        ui.label("Loading image...");
                    } else {
                        ui.label(
                            egui::RichText::new(
                                "Open or drag and drop a photo, folder \n\
                                or a (.zip, .7z and .rar) file containing your photos. 😊",
                            )
                            .size(32.0),
                        );
                    }
                });
            }
        });
        ctx.request_repaint();
    }
}

impl Drop for ViewerApp {
    fn drop(&mut self) {
        if let Err(e) = self.settings_manager.save() {
            eprintln!("Failed to save settings: {}", e);
        }
    }
}
