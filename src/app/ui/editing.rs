use crate::app::editing::{EditSession, MAX_EDIT_PIXELS, save_image_safely};
use crate::app::types::ViewerApp;
use crate::decoder::decode_bytes;
use eframe::egui;
use image::RgbaImage;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy)]
enum EditAction {
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
    Undo,
    Redo,
    Crop,
    Reset,
    SaveOverwrite,
    SaveAs,
    Cancel,
}

impl ViewerApp {
    pub fn start_editing(&mut self, ctx: &egui::Context) {
        if self.edit_session.is_some() {
            self.show_edit_menu = true;
            return;
        }
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            let _ = self.settings_manager.update(|settings| settings.slideshow_enabled = false);
        }
        if self.is_loading() || self.is_gif || self.virtual_texture.is_some() {
            self.edit_status = Some(
                "Editing is available for fully loaded, non-animated raster images only.".into(),
            );
            self.show_edit_menu = true;
            return;
        }
        let Some(path) = self.current_image_path.clone() else {
            self.edit_status = Some(
                "This image has no directly writable file path (archive entries cannot be edited yet)."
                    .into(),
            );
            self.show_edit_menu = true;
            return;
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.edit_status = Some(format!("Could not read source image: {error}"));
                self.show_edit_menu = true;
                return;
            }
        };
        // Inspect dimensions before full decoding where the decoder supports it,
        // avoiding a second large pixel allocation for images beyond the edit limit.
        let Some(format) = crate::decoder::format_detection::detect_format(&bytes) else {
            self.edit_status = Some("The source image format could not be identified.".into());
            self.show_edit_menu = true;
            return;
        };
        let (width, height) = match crate::decoder::default_registry().dimensions(&bytes, format) {
            Ok(dimensions) => dimensions,
            Err(error) => {
                self.edit_status = Some(format!("Could not inspect source dimensions: {error}"));
                self.show_edit_menu = true;
                return;
            }
        };
        let pixel_count = u64::from(width) * u64::from(height);
        if pixel_count > MAX_EDIT_PIXELS {
            self.edit_status = Some(format!(
                "This image is too large for the current editing history limit ({} × {}). Images up to {} megapixels are supported for editing.",
                width,
                height,
                MAX_EDIT_PIXELS / 1_000_000
            ));
            self.show_edit_menu = true;
            return;
        }
        let decoded = match decode_bytes(&bytes) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.edit_status = Some(format!("Could not decode source image for editing: {error}"));
                self.show_edit_menu = true;
                return;
            }
        };
        let Some(image) = RgbaImage::from_raw(decoded.width, decoded.height, decoded.data) else {
            self.edit_status = Some("Decoded image data had an invalid size.".into());
            self.show_edit_menu = true;
            return;
        };

        self.crop_x = 0;
        self.crop_y = 0;
        self.crop_width = image.width();
        self.crop_height = image.height();
        self.edit_session = Some(EditSession::new(path, image));
        self.edit_status = Some(
            "Working copy ready. Source file is unchanged until you explicitly save.".into(),
        );
        self.show_edit_menu = true;
        self.refresh_edit_texture(ctx);
    }

    fn refresh_edit_texture(&mut self, ctx: &egui::Context) {
        let Some(session) = self.edit_session.as_ref() else {
            return;
        };
        let size = [session.image.width() as usize, session.image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, session.image.as_raw());
        self.texture = Some(ctx.load_texture("edited_working_copy", color, self.get_texture_options()));
        self.virtual_texture = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    fn restore_original_preview(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.edit_session.take() {
            let size = [session.original.width() as usize, session.original.height() as usize];
            let color =
                egui::ColorImage::from_rgba_unmultiplied(size, session.original.as_raw());
            self.texture = Some(ctx.load_texture("edit_cancel_restore", color, self.get_texture_options()));
            self.b_fit_to_window = true;
            self.image_rect = None;
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
        }
        self.show_edit_menu = false;
        self.confirm_discard_edits = false;
        self.confirm_overwrite_path = None;
    }

    fn save_edit_to(&mut self, ctx: &egui::Context, destination: &Path, overwrite: bool, is_save_as: bool) {
        let Some(session) = self.edit_session.as_ref() else {
            self.edit_status = Some("There is no active edit session.".into());
            return;
        };
        let is_copy = is_save_as && session.source_path.as_path() != destination;
        match save_image_safely(&session.image, destination, overwrite) {
            Ok(()) => {
                let saved_path = destination.to_path_buf();
                if !is_copy {
                    if let Some(session) = self.edit_session.as_mut() {
                        session.original = session.image.clone();
                        session.dirty = false;
                    }
                    self.edit_status = Some(format!("Saved changes to {}.", saved_path.display()));
                } else {
                    self.edit_status = Some(format!(
                        "Saved a copy to {}. The original remains unchanged.",
                        saved_path.display()
                    ));
                }
                self.confirm_overwrite_path = None;
                self.overwrite_is_save_as = false;
                self.refresh_edit_texture(ctx);
            }
            Err(error) => {
                // Keep the working copy and undo history intact on every failure.
                self.edit_status = Some(error);
            }
        }
    }

    fn request_save_as(&mut self, ctx: &egui::Context) {
        let source_name = self
            .edit_session
            .as_ref()
            .and_then(|session| session.source_path.file_stem())
            .map(|name| format!("{}-edited.png", name.to_string_lossy()))
            .unwrap_or_else(|| "edited.png".to_string());
        let selected = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(source_name)
            .save_file();
        let Some(mut path) = selected else {
            return;
        };
        // Save As is intentionally PNG-only for predictable, widely supported output.
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("png"))
        {
            path.set_extension("png");
        }
        if path.exists() {
            self.confirm_overwrite_path = Some(path);
            self.overwrite_is_save_as = true;
        } else {
            self.save_edit_to(ctx, &path, false, true);
        }
    }

    pub fn render_editing_ui(&mut self, ctx: &egui::Context) {
        if self.show_edit_menu {
            let mut open = self.show_edit_menu;
            let mut action = None;
            let dimensions = self
                .edit_session
                .as_ref()
                .map(|session| session.image.dimensions());
            let can_undo = self.edit_session.as_ref().is_some_and(EditSession::can_undo);
            let can_redo = self.edit_session.as_ref().is_some_and(EditSession::can_redo);
            let dirty = self.edit_session.as_ref().is_some_and(|session| session.dirty);
            let source_path = self
                .edit_session
                .as_ref()
                .map(|session| session.source_path.clone());

            egui::Window::new("Edit image")
                .id(egui::Id::new("working_copy_editor"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(330.0)
                .show(ctx, |ui| {
                    if let (Some((width, height)), Some(path)) = (dimensions, source_path.as_ref()) {
                        ui.label(egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong());
                        ui.label(format!("Preview: {width} × {height}"));
                        ui.label(if dirty { "Unsaved edits" } else { "No unsaved edits" });
                        ui.separator();

                        ui.horizontal_wrapped(|ui| {
                            if ui.button("↶ Undo").clicked() && can_undo {
                                action = Some(EditAction::Undo);
                            }
                            if ui.button("↷ Redo").clicked() && can_redo {
                                action = Some(EditAction::Redo);
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Rotate left").clicked() { action = Some(EditAction::RotateLeft); }
                            if ui.button("Rotate right").clicked() { action = Some(EditAction::RotateRight); }
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Flip horizontal").clicked() { action = Some(EditAction::FlipHorizontal); }
                            if ui.button("Flip vertical").clicked() { action = Some(EditAction::FlipVertical); }
                        });

                        ui.separator();
                        ui.collapsing("Crop", |ui| {
                            let (max_width, max_height) = dimensions.unwrap_or((1, 1));
                            ui.horizontal(|ui| {
                                ui.label("X");
                                ui.add(egui::DragValue::new(&mut self.crop_x).range(0..=max_width.saturating_sub(1)));
                                ui.label("Y");
                                ui.add(egui::DragValue::new(&mut self.crop_y).range(0..=max_height.saturating_sub(1)));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Width");
                                ui.add(egui::DragValue::new(&mut self.crop_width).range(1..=max_width.max(1)));
                                ui.label("Height");
                                ui.add(egui::DragValue::new(&mut self.crop_height).range(1..=max_height.max(1)));
                            });
                            if ui.button("Apply crop").clicked() {
                                action = Some(EditAction::Crop);
                            }
                        });

                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Reset edits").clicked() { action = Some(EditAction::Reset); }
                            if ui.button("Save As…").clicked() { action = Some(EditAction::SaveAs); }
                            if ui.button("Overwrite original…").clicked() { action = Some(EditAction::SaveOverwrite); }
                        });
                        if ui.button("Discard edits and close").clicked() {
                            action = Some(EditAction::Cancel);
                        }
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "Saving re-encodes the image and may remove EXIF or other embedded metadata. Save As exports PNG."
                            ).small()
                        );
                    } else {
                        ui.label("Choose a supported, fully loaded image file to begin editing.");
                    }
                    if let Some(status) = &self.edit_status {
                        ui.separator();
                        ui.label(status);
                    }
                });

            if let Some(action) = action {
                match action {
                    EditAction::RotateLeft => {
                        if let Some(session) = self.edit_session.as_mut() { session.rotate_left(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::RotateRight => {
                        if let Some(session) = self.edit_session.as_mut() { session.rotate_right(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::FlipHorizontal => {
                        if let Some(session) = self.edit_session.as_mut() { session.flip_horizontal(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::FlipVertical => {
                        if let Some(session) = self.edit_session.as_mut() { session.flip_vertical(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Undo => {
                        if let Some(session) = self.edit_session.as_mut() { session.undo(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Redo => {
                        if let Some(session) = self.edit_session.as_mut() { session.redo(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Crop => {
                        let result = self.edit_session.as_mut().map(|session| {
                            session.crop(self.crop_x, self.crop_y, self.crop_width, self.crop_height)
                        });
                        match result {
                            Some(Ok(())) => {
                                self.edit_status = Some("Crop applied to the working copy.".into());
                                self.refresh_edit_texture(ctx);
                            }
                            Some(Err(error)) => self.edit_status = Some(error),
                            None => {}
                        }
                    }
                    EditAction::Reset => {
                        if let Some(session) = self.edit_session.as_mut() { session.reset(); }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::SaveOverwrite => {
                        if let Some(session) = self.edit_session.as_ref() {
                            self.confirm_overwrite_path = Some(session.source_path.clone());
                            self.overwrite_is_save_as = false;
                        }
                    }
                    EditAction::SaveAs => self.request_save_as(ctx),
                    EditAction::Cancel => {
                        if dirty {
                            self.confirm_discard_edits = true;
                        } else {
                            self.restore_original_preview(ctx);
                        }
                    }
                }
            }

            if !open {
                if self.edit_session.as_ref().is_some_and(|session| session.dirty) {
                    self.confirm_discard_edits = true;
                    self.show_edit_menu = true;
                } else {
                    self.restore_original_preview(ctx);
                }
            } else {
                self.show_edit_menu = true;
            }
        }

        if self.confirm_overwrite_path.is_some() {
            let mut open = true;
            egui::Window::new("Confirm overwrite")
                .id(egui::Id::new("edit_overwrite_confirmation"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    let target = self.confirm_overwrite_path.as_ref().unwrap();
                    ui.label(format!("Replace this existing file?\n{}", target.display()));
                    ui.label("The file will be backed up temporarily during replacement. Embedded metadata may be lost.");
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            let path = self.confirm_overwrite_path.clone().unwrap();
                            let is_save_as = self.overwrite_is_save_as;
                            self.save_edit_to(ctx, &path, true, is_save_as);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_overwrite_path = None;
                            self.overwrite_is_save_as = false;
                        }
                    });
                });
            if !open {
                self.confirm_overwrite_path = None;
                self.overwrite_is_save_as = false;
            }
        }

        if self.confirm_discard_edits {
            egui::Window::new("Discard unsaved edits?")
                .id(egui::Id::new("discard_edit_confirmation"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Discard the working copy? The original file has not been changed.");
                    ui.horizontal(|ui| {
                        if ui.button("Discard").clicked() {
                            self.restore_original_preview(ctx);
                        }
                        if ui.button("Keep editing").clicked() {
                            self.confirm_discard_edits = false;
                            self.show_edit_menu = true;
                        }
                    });
                });
        }
    }
}

