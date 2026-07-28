use crate::archive::{scan_7z, scan_rar, scan_zip};
use crate::gif_animation::GifAnimation;
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
use trash;

// Define LoadedImage enum here since it's not in loader.rs
pub enum LoadedImage {
    Static(DynamicImage),
    Animated(GifAnimation, bool), // (GifAnimation, is_preview)
}

pub struct ViewerApp {
    texture: Option<egui::TextureHandle>,
    gif_animation: Option<GifAnimation>,
    receiver: Option<Receiver<LoadedImage>>,
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
    pub settings_manager: SettingsManager,
    pub input_bindings: InputBindings,
    logo_texture: Option<egui::TextureHandle>,
    pub show_delete_confirmation: bool,
    delete_key_was_pressed: bool,
    is_gif: bool,
    full_gif_receiver: Option<Receiver<GifAnimation>>,
}

impl Default for ViewerApp {
    fn default() -> Self {
        let settings_manager = SettingsManager::new();
        let settings = settings_manager.get().clone();

        Self {
            texture: None,
            gif_animation: None,
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
            settings_manager: SettingsManager::new(),
            image_entries: Vec::new(),
            logo_texture: None,
            input_bindings: InputBindings::default(),
            show_delete_confirmation: false,
            delete_key_was_pressed: false,
            is_gif: false,
            full_gif_receiver: None,
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

    // Update the load_current_image method
    fn load_current_image(&mut self) {
        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        self.b_is_loading = true;
        self.gif_animation = None;
        self.is_gif = false;
        self.texture = None;

        let (tx, rx) = channel();

        spawn(move || {
            let loaded = match entry {
                ImageEntry::File(path) => {
                    if let Some(ext) = path.extension() {
                        if ext.eq_ignore_ascii_case("gif") {
                            // Load preview first
                            if let Some(gif) = crate::loader::load_gif_preview(path) {
                                Some(LoadedImage::Animated(gif, true))
                            } else {
                                None
                            }
                        } else {
                            crate::loader::load(path).map(LoadedImage::Static)
                        }
                    } else {
                        crate::loader::load(path).map(LoadedImage::Static)
                    }
                }
                ImageEntry::Zip(zip) => {
                    if zip.name.to_lowercase().ends_with(".gif") {
                        crate::loader::load_zip_gif_preview(zip)
                            .map(|g| LoadedImage::Animated(g, true))
                    } else {
                        crate::loader::load_zip_image(zip).map(LoadedImage::Static)
                    }
                }
                ImageEntry::S7z(s7z) => {
                    if s7z.name.to_lowercase().ends_with(".gif") {
                        crate::loader::load_7z_gif_preview(s7z)
                            .map(|g| LoadedImage::Animated(g, true))
                    } else {
                        crate::loader::load_7z_image(s7z).map(LoadedImage::Static)
                    }
                }
                ImageEntry::Rar(rar) => {
                    if rar.name.to_lowercase().ends_with(".gif") {
                        crate::loader::load_rar_gif_preview(rar)
                            .map(|g| LoadedImage::Animated(g, true))
                    } else {
                        crate::loader::load_rar_image(rar).map(LoadedImage::Static)
                    }
                }
            };

            if let Some(img) = loaded {
                let _ = tx.send(img);
            }
        });

        self.receiver = Some(rx);
    }

    fn update_gif_texture(&mut self, ctx: &egui::Context) {
        if let Some(gif) = &mut self.gif_animation {
            if let Some(frame) = gif.get_current_frame() {
                let size = [frame.width() as usize, frame.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());

                self.texture = Some(ctx.load_texture("gif_frame", color_image, Default::default()));

                if gif.is_playing {
                    ctx.request_repaint();
                }
            }
        }
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
            self.gif_animation = None;
            self.is_gif = false;
            self.texture = None;
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
        self.gif_animation = None;
        self.is_gif = false;
        self.texture = None;
    }

    fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.image_entries = entries;
        self.current_index = current_index;
        self.b_fit_to_window = true;
        self.gif_animation = None;
        self.is_gif = false;
        self.texture = None;
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
                self.b_zoom_used = false;
            }
            ViewerCommand::OpenFile => {
                self.open_file_dialog();
            }
            ViewerCommand::ToggleFullscreen => {
                self.toggle_fullscreen(ctx);
                self.b_fit_to_window = false;
            }
            ViewerCommand::JumpToFirst => {
                if !self.image_entries.is_empty() {
                    self.current_index = 0;
                    self.b_fit_to_window = true;
                    self.image_rect = None;
                    self.gif_animation = None;
                    self.is_gif = false;
                    self.texture = None;
                    self.load_current_image();
                }
            }
            ViewerCommand::JumpToLast => {
                if !self.image_entries.is_empty() {
                    self.current_index = self.image_entries.len() - 1;
                    self.b_fit_to_window = true;
                    self.image_rect = None;
                    self.gif_animation = None;
                    self.is_gif = false;
                    self.texture = None;
                    self.load_current_image();
                }
            }
            ViewerCommand::ToggleGifPlay => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.toggle_play();
                }
            }
            ViewerCommand::GifSpeedHalf => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.set_speed(0.5);
                }
            }
            ViewerCommand::GifSpeedUp => {
                if let Some(gif) = &mut self.gif_animation {
                    gif.set_speed(2.0);
                }
            }
        }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        self.b_zoom_used = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.texture = None;

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

    pub fn save_window_state(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let pos = [rect.min.x, rect.min.y];
            let size = [rect.width(), rect.height()];

            let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            if !is_fullscreen && size[0] > 100.0 && size[1] > 100.0 {
                if let Err(e) = self.settings_manager.update_window_state(pos, size) {
                    eprintln!("Failed to save window state: {}", e);
                }
            }
        }
    }

    fn delete_current_image(&mut self) {
        if self.image_entries.is_empty() {
            return;
        }

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => return,
        };

        let file_path = match entry {
            ImageEntry::File(path) => path,
            _ => {
                println!("Cannot delete images inside archives directly");
                return;
            }
        };

        if !file_path.exists() {
            println!("File no longer exists on disk");
            self.image_entries.remove(self.current_index);
            if self.current_index >= self.image_entries.len() {
                self.current_index = 0;
            }
            if !self.image_entries.is_empty() {
                self.load_current_image();
            } else {
                self.texture = None;
            }
            return;
        }

        match trash::delete(&file_path) {
            Ok(_) => {
                println!("Moved to trash: {:?}", file_path);
                self.image_entries.remove(self.current_index);
                if self.image_entries.is_empty() {
                    self.texture = None;
                    self.current_index = 0;
                } else if self.current_index >= self.image_entries.len() {
                    self.current_index = self.image_entries.len() - 1;
                    self.load_current_image();
                } else {
                    self.load_current_image();
                }
                self.b_fit_to_window = true;
                self.image_rect = None;
                self.b_zoom_used = false;
            }
            Err(e) => {
                eprintln!("Failed to move to trash: {}", e);
            }
        }
    }
}

// =========== update  ===========

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Initialize logo texture
        if self.logo_texture.is_none() {
            let image = image::load_from_memory(include_bytes!("../assets/icon.ico"))
                .unwrap()
                .into_rgba8();

            let size = [image.width() as usize, image.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            self.logo_texture = Some(ctx.load_texture("logo", color_image, Default::default()));
        }

        // almighty esc key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

            if fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            } else {
                self.save_window_state(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            return;
        }

        // Standard keyboard shortcuts
        for command in handle_keyboard(ctx, &self.input_bindings) {
            self.handle_command(ctx, command);
        }

        // Delete key - trigger on key release
        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete));
        let delete_released = ctx.input(|i| i.key_released(egui::Key::Delete));

        if delete_pressed {
            self.delete_key_was_pressed = true;
        } else if delete_released && self.delete_key_was_pressed {
            self.delete_current_image();
            self.delete_key_was_pressed = false;
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
            if let Ok(loaded_image) = rx.try_recv() {
                match loaded_image {
                    LoadedImage::Static(img) => {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        self.texture = Some(ctx.load_texture("image", color, Default::default()));
                        self.gif_animation = None;
                        self.is_gif = false;
                    }
                    LoadedImage::Animated(gif, is_preview) => {
                        // Store the preview GIF
                        self.gif_animation = Some(gif);
                        self.is_gif = true;

                        // If it's a preview, spawn a background task to load the full GIF
                        if is_preview {
                            let entry = self.image_entries[self.current_index].clone();
                            let (full_tx, full_rx) = channel();

                            spawn(move || {
                                let full_gif = match entry {
                                    ImageEntry::File(path) => crate::loader::load_gif(path),
                                    ImageEntry::Zip(zip) => crate::loader::load_zip_gif(zip),
                                    ImageEntry::S7z(s7z) => crate::loader::load_7z_gif(s7z),
                                    ImageEntry::Rar(rar) => crate::loader::load_rar_gif(rar),
                                };
                                if let Some(gif) = full_gif {
                                    let _ = full_tx.send(gif);
                                }
                            });

                            // Store the receiver to check for the full GIF later
                            // We'll need to store this in the struct
                            self.full_gif_receiver = Some(full_rx);
                        }
                    }
                }
                self.b_fit_to_window = true;
                self.b_is_loading = false;
                self.receiver = None;
            }
        }

        // Check for full GIF upgrade
        if let Some(rx) = &self.full_gif_receiver {
            if let Ok(full_gif) = rx.try_recv() {
                if let Some(gif) = &mut self.gif_animation {
                    gif.upgrade_to_full(full_gif);
                    // The GIF is now fully loaded and will start playing
                }
                self.full_gif_receiver = None;
            }
        }

        // Update GIF animation (only if we have a GIF)
        if self.is_gif {
            self.update_gif_texture(ctx);
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

                // GIF controls - only show when a GIF is loaded
                if self.is_gif {
                    if let Some(gif) = &mut self.gif_animation {
                        if gif.is_animated() {
                            ui.add_space(10.0);
                            ui.label("GIF:");

                            // Play/Pause button
                            if ui.button(if gif.is_playing { "⏸" } else { "▶" }).clicked() {
                                gif.toggle_play();
                            }

                            // Speed slider - logarithmic from 0.1 to 10.0
                            ui.label("Speed:");

                            // Map speed value to logarithmic scale
                            // We'll use the slider's built-in logarithmic support
                            let mut speed = gif.speed_multiplier;

                            // Create a slider with logarithmic scaling
                            let speed_slider = egui::Slider::new(&mut speed, 0.1..=10.0)
                                .logarithmic(true)
                                .text("x")
                                .smallest_positive(0.1)
                                .step_by(0.01);

                            if ui.add(speed_slider).changed() {
                                gif.set_speed(speed);
                            }

                            // Frame counter
                            ui.label(format!(
                                "Frame {}/{}",
                                gif.get_current_frame_index() + 1,
                                gif.frame_count()
                            ));

                            ui.add_space(10.0);
                            ui.label("|");
                        }
                    }
                }

                if ui
                    .checkbox(&mut self.b_ctrl_invert, "Invert Ctrl Scroll | ")
                    .changed()
                {
                    let _ = self.settings_manager.update(|settings| {
                        settings.b_ctrl_invert = self.b_ctrl_invert;
                    });
                }

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

                if self.b_fit_to_window {
                    let texture_size = texture.size_vec2();
                    let zoom_x = image_available_size.x / texture_size.x;
                    let zoom_y = image_available_size.y / texture_size.y;
                    let fit_zoom = zoom_x.min(zoom_y).min(1.0);
                    self.zoom = fit_zoom;
                    self.pan = egui::Vec2::ZERO;
                    self.b_fit_to_window = false;
                }

                let display_size = texture_size * self.zoom;
                let viewport = ui.max_rect();
                let image_rect = egui::Rect::from_center_size(
                    viewport.center() + self.pan * self.zoom,
                    display_size,
                );

                ui.put(image_rect, egui::Image::new((texture.id(), display_size)));

                let response = ui.interact(viewport, ui.id().with("image"), egui::Sense::drag());

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
                        self.pan += delta / self.zoom;
                        if let Some(texture) = &self.texture {
                            let texture_limit = (texture.size_vec2() / 2.0)
                                + ((ctx.available_rect().size() / self.zoom) / 4.0);
                            self.pan = self.pan.clamp(-texture_limit, texture_limit);
                        }
                    }

                    if right_down {
                        self.zoom *= 1.0 + delta.y * -0.01;
                        self.zoom = self.zoom.clamp(0.01, 10.0);
                        self.b_zoom_used = true;
                    }
                }
            } else if self.is_gif {
                // If we have a GIF but no texture yet, show loading
                if self.b_is_loading {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading GIF...");
                    });
                } else if self.texture.is_none() && self.is_gif {
                    // Texture not yet created, show loading
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading GIF frame...");
                    });
                }
            } else {
                if self.b_is_loading {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading image...");
                    });
                } else {
                    let available = ui.available_height();
                    let content_height = 128.0 + 16.0 + 60.0;

                    ui.add_space((available - content_height).max(0.0) * 0.5);

                    ui.vertical_centered(|ui| {
                        if let Some(icon) = &self.logo_texture {
                            ui.image((icon.id(), egui::vec2(128.0, 128.0)));
                            ui.add_space(16.0);
                        }

                        ui.label(
                            egui::RichText::new(
                                "Press Ctrl+O or drag and drop a photo, folder\n\
                                 or a .zip, .7z, or .rar archive containing your photos.",
                            )
                            .size(24.0),
                        );
                    });
                }
            }
        });

        ctx.request_repaint();

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_window_state(ctx);
        }

        if self.show_delete_confirmation {
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Move this file to the Trash/Recycle Bin?");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            self.show_delete_confirmation = false;
                            self.delete_current_image();
                        }
                        if ui.button("No").clicked() {
                            self.show_delete_confirmation = false;
                        }
                    });
                });
        }
    }
}

impl Drop for ViewerApp {
    fn drop(&mut self) {
        if let Err(e) = self.settings_manager.save() {
            eprintln!("Failed to save settings: {}", e);
        }
    }
}
