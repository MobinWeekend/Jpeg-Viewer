use eframe::egui;
//use egui::Checkbox;
//use eframe::glow::ZERO;
//use egui::Direction;
//use egui_extras::widgets::Toggle;
use image::DynamicImage;
use rayon::spawn;
use std::sync::mpsc::{channel, Receiver};
use std::path::PathBuf;
use std::fs;
use crate::settings::SettingsManager;

pub struct ViewerApp {
    texture: Option<egui::TextureHandle>,
    receiver: Option<Receiver<DynamicImage>>,
    zoom: f32,
    pan: egui::Vec2,
    current_image_path: Option<PathBuf>,
    current_directory: Option<PathBuf>,
    image_files: Vec<PathBuf>,
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
            current_image_path: None,
            current_directory: None,
            image_files: Vec::new(),
            current_index: 0,
            is_loading: false,
            is_fit_to_window: false,
            image_rect: None,
            last_window_size: None,
            is_zoom_used: false,
            is_ctrl_invert: settings.is_ctrl_invert,
            settings_manager,
        }
    }
}

impl ViewerApp {
    fn load_image(&mut self, path: PathBuf) {
        // Store the directory and find all images
        if let Some(parent) = path.parent() {
            self.current_directory = Some(parent.to_path_buf());
            
            // Get all image files in the directory
            let image_extensions = ["jpg", "jpeg", "png", "gif", "bmp", "webp"];
            let mut files: Vec<PathBuf> = fs::read_dir(parent)
                .ok()
                .into_iter()
                .flat_map(|entries| {
                    entries.filter_map(|entry| {
                        let entry = entry.ok()?;
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(ext) = path.extension() {
                                if let Some(ext_str) = ext.to_str() {
                                    if image_extensions.iter().any(|&e| e.eq_ignore_ascii_case(ext_str)) {
                                        return Some(path);
                                    }
                                }
                            }
                        }
                        None
                    })
                    .collect::<Vec<_>>()
                })
                .collect();
            
            files.sort();
            
            if let Some(index) = files.iter().position(|p| p == &path) {
                self.image_files = files;
                self.current_index = index;
                self.current_image_path = Some(path);
            }
        }
    }

    fn load_current_image(&mut self) {
        if let Some(path) = &self.current_image_path {
            self.is_loading = true;
            let (tx, rx) = channel();
            let path_clone = path.clone();

            spawn(move || {
                if let Some(img) = crate::loader::load(path_clone) {
                    let _ = tx.send(img);
                }
            });

            self.receiver = Some(rx);
        }
    }

    fn navigate_images(&mut self, direction: i32) {
        if self.image_files.is_empty() {
            return;
        }

        let len = self.image_files.len();
        let new_index = (self.current_index as i32 + direction).rem_euclid(len as i32) as usize;
        
        if new_index != self.current_index {
            self.current_index = new_index;
            if let Some(path) = self.image_files.get(new_index) {
                self.current_image_path = Some(path.clone());
                self.pan = egui::Vec2::ZERO;
                self.is_fit_to_window = true;
                self.image_rect = None;
                self.load_current_image();
            }
        }
        self.is_zoom_used = false;
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {

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
                let size = [
                    rgba.width() as usize,
                    rgba.height() as usize,
                ];
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    size,
                    rgba.as_raw(),
                );

                self.texture = Some(
                    ctx.load_texture(
                        "image",
                        color,
                        Default::default(),
                    ),
                );
                
                // Auto-zoom to fit if image is larger than window or is_fit_to_window is true
                self.is_fit_to_window = true;
                self.pan = egui::Vec2::ZERO;
                self.is_loading = false;
                self.receiver = None;
            }
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if ui.button("Open").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Images", &["jpg", "jpeg", "png", "gif", "bmp", "webp"])
                    .pick_file()
                {
                    self.load_image(path.clone());
                    self.is_zoom_used = false;
                    self.zoom = 1.0;
                    self.is_fit_to_window = true;
                    self.image_rect = None;
                    self.load_current_image();
                }
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.01..=10.0)
                        .logarithmic(true)
                        .text("Zoom"),
                );

                //let mut is_ctrl_invert = self.settings_manager.get().is_ctrl_invert;
                //ui.add(Checkbox::new(&mut self.is_ctrl_invert, "Invert Ctrl Scroll"));
                if ui.checkbox(&mut self.is_ctrl_invert, "Invert Ctrl Scroll").changed() {
                    // Save the setting when changed
                    let _ = self.settings_manager.update(|settings| {
                        settings.is_ctrl_invert = self.is_ctrl_invert;
                    });
                }

                /*
                if let Some(texture) = &self.texture {
                        let texture_limit = (texture.size_vec2() / 2.0) + ((ctx.available_rect().size() / self.zoom) / 4.0);
                ui.add(
                    egui::Slider::new(&mut self.pan.x, -texture_limit.x..=texture_limit.x)
                        .logarithmic(false)
                        .text("X"),
                );

                ui.add(
                    egui::Slider::new(&mut self.pan.y, -texture_limit.y..=texture_limit.y)
                        .logarithmic(false)
                        .text("Y"),
                );
                } */

                // Show current image info
                if let Some(path) = &self.current_image_path {
                    if let Some(file_name) = path.file_name() {
                        ui.label(format!("{} ({}/{})", 
                            file_name.to_string_lossy(),
                            self.current_index + 1,
                            self.image_files.len()
                        ));
                    }
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
                    if let Some(texture) = &self.texture {
                        let texture_size = texture.size_vec2();
                        let zoom_x = image_available_size.x / texture_size.x;
                        let zoom_y = image_available_size.y / texture_size.y;
                        let fit_zoom = zoom_x.min(zoom_y).min(1.0); // Only zoom out if needed
                        self.zoom = fit_zoom;
                        self.is_fit_to_window = false;
                    }
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

                ui.put(
                    image_rect,
                    egui::Image::new((texture.id(), display_size)),
                );
                
                let response = ui.interact(
                    viewport,                        // The area to interact with
                    ui.id().with("image"),           // Unique ID for this interaction
                    egui::Sense::drag(),             // Only detect drag gestures
                );

                if response.dragged() {
                    self.pan += response.drag_delta() / self.zoom;
                    if let Some(texture) = &self.texture {
                        let texture_limit = (texture.size_vec2() / 2.0) + ((ctx.available_rect().size() / self.zoom) / 4.0);
                        self.pan = self.pan.clamp(-texture_limit, texture_limit);
                    }
                }

                // Check for arrow key presses
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    self.navigate_images(-1);  // Go to previous image
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    self.navigate_images(1);   // Go to next image
                }

                let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
                if scroll_delta.y != 0.0 {
                    // Only navigate if the image fits OR if Shift is held AND the mouse is over the image
                    let mouse_over = response.hovered();
                    let click_toggle_key_pressed = ctx.input(|i| i.modifiers.ctrl);
                    //println!("Scroll: {}, Mouse over: {}, toggle: {}, Image fits: {}", 
                    //    scroll_delta.y, mouse_over, click_toggle_key_pressed, image_fits);     
                    if mouse_over {
                        if (!click_toggle_key_pressed && !self.is_ctrl_invert) || (click_toggle_key_pressed && self.is_ctrl_invert) {
                            let direction = if scroll_delta.y > 0.0 { -1 } else { 1 };
                            self.navigate_images(direction);
                        } else {
                            let zoom_direction: f32 = if scroll_delta.y > 0.0 {1.0} else {-1.0};
                            self.zoom += self.zoom * zoom_direction * 0.1;
                            if let Some(texture) = &self.texture {
                                let texture_limit = (texture.size_vec2() / 2.0) + ((ctx.available_rect().size() / self.zoom) / 4.0);
                                self.pan = self.pan.clamp(-texture_limit, texture_limit);
                            }
                            self.is_zoom_used = true;
                        }
                    }
                }
                // Store the response to check for scroll events
                /*let mut response = None;
                 if image_fits {
                    // Image fits, center it
                    ui.centered_and_justified(|ui| {
                        let img = ui.add(
                            egui::Image::new((texture.id(), display_size))
                                .fit_to_exact_size(display_size)
                        );
                        response = Some(img);
                    });
                } else {
                    // Image doesn't fit, show with scrollbars
                    let scroll_area = egui::ScrollArea::both()
                        .auto_shrink([false; 2]);
                    
                    response = Some(scroll_area.show(ui, |ui| {
                        ui.add(egui::Image::new((texture.id(), display_size)))
                    }).inner);
                }
                
                // Handle mouse wheel for navigation
                // Check if scroll happened and it wasn't consumed by the scroll area
                if let Some(resp) = response {
                    let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
                    if scroll_delta.y != 0.0 {
                        // Only navigate if the image fits OR if Shift is held
                        // AND the mouse is over the image
                        let mouse_over = resp.hovered();
                        
                        if (image_fits || ctx.input(|i| i.modifiers.shift)) && mouse_over {
                            let direction = if scroll_delta.y > 0.0 { -1 } else { 1 };
                            // Prevent navigation if we're still scrolling (debounce)
                            self.navigate_images(direction);
                        }
                    }
                } */
            } else {
                ui.centered_and_justified(|ui| {
                    if self.is_loading {
                        ui.label("Loading image...");
                    } else {
                        ui.label("Open an image file.");
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