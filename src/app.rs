use crate::archive::{scan_7z, scan_rar, scan_zip};
use crate::helpers::{ARCHIVE_EXT, IMAGE_EXT};
use crate::helpers::{get_extension, is_supported_image};
use crate::image_entry::ImageEntry;
use crate::settings::SettingsManager;
use crate::shortcuts::{InputBindings, ViewerCommand, handle_keyboard, handle_mouse};
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
    b_is_loading: bool,
    b_fit_to_window: bool,
    image_rect: Option<egui::Rect>,
    last_window_size: Option<egui::Vec2>,
    b_zoom_used: bool,
    b_ctrl_invert: bool,
    settings_manager: SettingsManager,
    pub input_bindings: InputBindings,
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
            b_is_loading: false,
            b_fit_to_window: false,
            image_rect: None,
            last_window_size: None,
            b_zoom_used: false,
            b_ctrl_invert: settings.b_ctrl_invert,
            settings_manager,
            image_entries: Vec::new(),
            input_bindings: InputBindings::default(),
        }
    }
}

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

        self.b_is_loading = true;

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
            self.b_fit_to_window = true;
            self.image_rect = None;
            self.load_current_image();
        }
        self.b_zoom_used = false;
    }

    fn load_directory(&mut self, path: &PathBuf) {
        self.current_directory = Some(path.clone());
        let files = crate::loader::load_directory_images(path);
        if files.is_empty() {
            println!("No images found in directory: {:?}", path);
            return;
        }
        let entries = files.into_iter().map(ImageEntry::File).collect();

        self.set_image_entries(entries, 0);
        self.zoom = 1.0;
    }

    // handeling entries
    fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.image_entries = entries;
        self.current_index = current_index;
        self.b_fit_to_window = true;
        self.load_current_image();
    }

    fn handle_command(&mut self, ctx: &egui::Context, command: ViewerCommand) {
        match command {
            ViewerCommand::NextImage => {
                self.navigate_images(1);
            }

            ViewerCommand::PreviousImage => {
                self.navigate_images(-1);
            }

            ViewerCommand::ZoomIn => {
                self.zoom *= 1.1;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }

            ViewerCommand::ZoomOut => {
                self.zoom /= 1.1;
                self.zoom = self.zoom.clamp(0.01, 10.0);
                self.b_zoom_used = true;
            }

            ViewerCommand::ResetZoom => {
                self.zoom = 1.0;
            }

            ViewerCommand::MakeFit => {
                self.b_fit_to_window = true;
                self.b_zoom_used = false; // Reset zoom usage to allow fit-to-window recalculation
            }

            ViewerCommand::OpenFile => {
                self.open_file_dialog();
            }
            ViewerCommand::ToggleFullscreen => {
                self.toggle_fullscreen(ctx);
                self.b_fit_to_window = false;
            } /*
              // this will used later when i implement a close button in the ui
              ViewerCommand::Close => {
                  let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

                  if is_fullscreen {
                      ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                  } else {
                      ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                  }
              }
               */
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        self.b_zoom_used = false;
        self.zoom = 1.0;
        self.b_fit_to_window = true;
        self.image_rect = None;

        if path.is_dir() {
            self.load_directory(&path);
            return;
        }

        match get_extension(&path).as_deref() {
            Some("zip") => {
                self.set_image_entries(scan_zip(&path), 0);
            }

            Some("7z") => {
                self.set_image_entries(scan_7z(&path), 0);
            }

            Some("rar") => {
                self.set_image_entries(scan_rar(&path), 0);
            }

            Some(_) if is_supported_image(&path) => {
                self.load_image(path);
            }

            _ => {
                println!("Unsupported file: {:?}", path);
            }
        }
    }

    fn open_file_dialog(&mut self) {
        let mut extensions = Vec::new();
        extensions.extend_from_slice(IMAGE_EXT);
        extensions.extend_from_slice(ARCHIVE_EXT);

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images and Archives", &extensions)
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
    }
}

// =========== update  ===========

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // almighty esc key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

            if fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            return;
        }

        // keys
        for command in handle_keyboard(ctx, &self.input_bindings) {
            self.handle_command(ctx, command);
        }

        // mouse buttons
        for command in handle_mouse(
            ctx,
            &self.input_bindings,
            ctx.is_pointer_over_area(),
            self.b_ctrl_invert,
        ) {
            self.handle_command(ctx, command);
        }

        // drag & drop
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());

        for file in dropped_files {
            if let Some(path) = file.path {
                self.open_path(path);
            }
        }

        // check for window resize
        let current_size = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size());
        if let (Some(prev), Some(curr)) = (self.last_window_size, current_size) {
            if prev != curr && !self.b_zoom_used {
                self.b_fit_to_window = true;
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

                // Auto-zoom to fit if image is larger than window or b_fit_to_window is true
                self.b_fit_to_window = true;
                self.b_is_loading = false;
                self.receiver = None;
            }
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                }

                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.01..=10.0)
                        .logarithmic(true)
                        .text("Zoom | "),
                );

                if ui
                    .checkbox(&mut self.b_ctrl_invert, "Invert Ctrl Scroll | ")
                    .changed()
                {
                    // Save the setting when changed
                    let _ = self.settings_manager.update(|settings| {
                        settings.b_ctrl_invert = self.b_ctrl_invert;
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
                if !self.b_ctrl_invert {
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

                // Auto-fit to window if b_fit_to_window is true
                if self.b_fit_to_window {
                    // ^ when true, checks the image to see if it needs fiting inside the window
                    let texture_size = texture.size_vec2();
                    let zoom_x = image_available_size.x / texture_size.x;
                    let zoom_y = image_available_size.y / texture_size.y;
                    let fit_zoom = zoom_x.min(zoom_y).min(1.0); // Only zoom out if needed
                    self.zoom = fit_zoom;
                    self.pan = egui::Vec2::ZERO; // Reset pan when fitting to window
                    self.b_fit_to_window = false;
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

                for command in handle_mouse(
                    ctx,
                    &self.input_bindings,
                    response.hovered(),
                    self.b_ctrl_invert,
                ) {
                    self.handle_command(ctx, command);
                }

                if response.dragged() {
                    let (left_down, right_down, delta) = ctx.input(|i| {
                        (
                            i.pointer.button_down(egui::PointerButton::Primary),
                            i.pointer.button_down(egui::PointerButton::Secondary),
                            i.pointer.delta(),
                        )
                    });

                    if left_down {
                        // Pan
                        self.pan += delta / self.zoom;

                        if let Some(texture) = &self.texture {
                            let texture_limit = (texture.size_vec2() / 2.0)
                                + ((ctx.available_rect().size() / self.zoom) / 4.0);

                            self.pan = self.pan.clamp(-texture_limit, texture_limit);
                        }
                    }

                    if right_down {
                        // Zoom
                        self.zoom *= 1.0 + delta.y * -0.01;
                        self.zoom = self.zoom.clamp(0.01, 10.0);
                        self.b_zoom_used = true;
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    if self.b_is_loading {
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
