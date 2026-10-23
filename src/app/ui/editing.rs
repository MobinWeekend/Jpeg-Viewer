use crate::app::editing::{
    ADJUSTMENT_PREVIEW_PIXELS, EditOutputFormat, EditSession, ImageAdjustments, MAX_EDIT_PIXELS,
    save_image_safely_with_quality,
};
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
    ExitMode,
    Discard,
}

impl ViewerApp {
    pub fn start_editing(&mut self, ctx: &egui::Context) {
        if self.edit_session.is_some() {
            self.show_edit_menu = !self.show_edit_menu;
            self.show_settings_menu = false;
            self.crop_tool_active = false;
            self.crop_drag_start_image = None;
            if self.show_edit_menu {
                self.refresh_adjustment_preview(ctx, false);
            }
            return;
        }
        if self.slideshow_enabled {
            self.slideshow_enabled = false;
            let _ = self
                .settings_manager
                .update(|settings| settings.slideshow_enabled = false);
        }
        if self.is_loading() || self.is_gif || self.virtual_texture.is_some() {
            self.edit_status = Some(
                "Editing is available for fully loaded, non-animated raster images only.".into(),
            );
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        }
        let Some(path) = self.current_image_path.clone() else {
            self.edit_status = Some(
                "This image has no directly writable file path (archive entries cannot be edited yet)."
                    .into(),
            );
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.edit_status = Some(format!("Could not read source image: {error}"));
                self.show_edit_menu = true;
                self.show_settings_menu = false;
                return;
            }
        };
        let Some(format) = crate::decoder::format_detection::detect_format(&bytes) else {
            self.edit_status = Some("The source image format could not be identified.".into());
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        };
        let (width, height) = match crate::decoder::default_registry().dimensions(&bytes, format) {
            Ok(dimensions) => dimensions,
            Err(error) => {
                self.edit_status = Some(format!("Could not inspect source dimensions: {error}"));
                self.show_edit_menu = true;
                self.show_settings_menu = false;
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
            self.show_settings_menu = false;
            return;
        }
        let decoded = match decode_bytes(&bytes) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.edit_status = Some(format!(
                    "Could not decode source image for editing: {error}"
                ));
                self.show_edit_menu = true;
                self.show_settings_menu = false;
                return;
            }
        };
        let Some(image) = RgbaImage::from_raw(decoded.width, decoded.height, decoded.data) else {
            self.edit_status = Some("Decoded image data had an invalid size.".into());
            self.show_edit_menu = true;
            self.show_settings_menu = false;
            return;
        };

        self.crop_x = 0;
        self.crop_y = 0;
        self.crop_width = image.width();
        self.crop_height = image.height();
        self.crop_tool_active = false;
        self.crop_drag_start_image = None;
        self.crop_selection = None;
        self.adjustment_gesture_active = false;
        self.show_settings_menu = false;
        self.edit_session = Some(EditSession::new(path, image));
        self.edit_status =
            Some("Editing a working copy. The source remains unchanged until you save.".into());
        self.show_edit_menu = true;
        self.refresh_edit_texture(ctx);
    }

    fn refresh_edit_texture(&mut self, ctx: &egui::Context) {
        let Some(session) = self.edit_session.as_ref() else {
            return;
        };
        let image = session.materialized_image();
        let size = [image.width() as usize, image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        self.texture =
            Some(ctx.load_texture("edited_working_copy", color, self.get_texture_options()));
        self.virtual_texture = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.b_fit_to_window = true;
        self.image_rect = None;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    /// Refresh a slider preview without changing the on-screen image size.
    /// Slider dragging uses a downsampled preview; release restores full resolution.
    fn refresh_adjustment_preview(&mut self, ctx: &egui::Context, downsample: bool) {
        let Some(session) = self.edit_session.as_ref() else {
            return;
        };
        let old_size = self.texture.as_ref().map(|texture| texture.size_vec2());
        let image = if downsample {
            session.preview_image(ADJUSTMENT_PREVIEW_PIXELS)
        } else {
            session.materialized_image()
        };
        let new_size = egui::vec2(image.width() as f32, image.height() as f32);
        let size = [image.width() as usize, image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        self.texture =
            Some(ctx.load_texture("edited_working_copy", color, self.get_texture_options()));
        self.virtual_texture = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.image_rect = None;

        if !self.b_fit_to_window {
            if let Some(old_size) = old_size.filter(|size| size.x > 0.0 && size.y > 0.0) {
                let factor = old_size.x / new_size.x.max(1.0);
                self.zoom = (self.zoom * factor).clamp(0.005, 50.0);
                if factor > 0.0 {
                    self.pan /= factor;
                }
            }
        }
    }

    fn restore_original_preview(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.edit_session.take() {
            let size = [
                session.original.width() as usize,
                session.original.height() as usize,
            ];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, session.original.as_raw());
            self.texture =
                Some(ctx.load_texture("edit_cancel_restore", color, self.get_texture_options()));
            self.b_fit_to_window = true;
            self.image_rect = None;
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
        }
        self.show_edit_menu = false;
        self.crop_tool_active = false;
        self.crop_drag_start_image = None;
        self.crop_selection = None;
        self.adjustment_gesture_active = false;
        self.confirm_discard_edits = false;
        self.confirm_overwrite_path = None;
    }

    fn save_edit_to(
        &mut self,
        ctx: &egui::Context,
        destination: &Path,
        overwrite: bool,
        is_save_as: bool,
    ) {
        let Some(output_image) = self
            .edit_session
            .as_ref()
            .map(EditSession::materialized_image)
        else {
            self.edit_status = Some("There is no active edit session.".into());
            return;
        };
        let is_copy = self
            .edit_session
            .as_ref()
            .is_some_and(|session| is_save_as && session.source_path.as_path() != destination);

        match save_image_safely_with_quality(
            &output_image,
            destination,
            overwrite,
            self.save_quality,
        ) {
            Ok(()) => {
                let saved_path = destination.to_path_buf();
                if !is_copy {
                    if let Some(session) = self.edit_session.as_mut() {
                        session.mark_saved(output_image);
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
        let format = self.edit_output_format;
        let source_name = self
            .edit_session
            .as_ref()
            .and_then(|session| session.source_path.file_stem())
            .map(|name| format!("{}-edited.{}", name.to_string_lossy(), format.extension()))
            .unwrap_or_else(|| format!("edited.{}", format.extension()));
        let selected = rfd::FileDialog::new()
            .add_filter(format.label(), &[format.extension()])
            .set_file_name(source_name)
            .save_file();
        let Some(mut path) = selected else {
            return;
        };
        if !path.extension().is_some_and(|extension| {
            extension
                .to_string_lossy()
                .eq_ignore_ascii_case(format.extension())
        }) {
            path.set_extension(format.extension());
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
            let mut action = None;
            let dimensions = self
                .edit_session
                .as_ref()
                .map(|session| session.image.dimensions());
            let can_undo = self
                .edit_session
                .as_ref()
                .is_some_and(EditSession::can_undo);
            let can_redo = self
                .edit_session
                .as_ref()
                .is_some_and(EditSession::can_redo);
            let dirty = self
                .edit_session
                .as_ref()
                .is_some_and(|session| session.dirty);
            let source_path = self
                .edit_session
                .as_ref()
                .map(|session| session.source_path.clone());
            let mut adjustments = self
                .edit_session
                .as_ref()
                .map(|session| session.adjustments)
                .unwrap_or_default();
            let original_adjustments = adjustments;
            let mut adjustments_changed = false;
            let mut adjustment_dragging = false;
            let mut adjustment_drag_stopped = false;
            let mut reset_adjustments_clicked = false;
            let mut output_format = self.edit_output_format;
            let mut quality = self.save_quality;
            let mut crop_tool_active = self.crop_tool_active;
            let mut crop_x = self.crop_x;
            let mut crop_y = self.crop_y;
            let mut crop_width = self.crop_width;
            let mut crop_height = self.crop_height;
            let mut crop_selection = self.crop_selection;

            let default_width = self
                .settings_manager
                .get()
                .settings_sidebar_width
                .clamp(280.0, 560.0);
            let panel_response = egui::SidePanel::right("edit_tools_sidebar")
                .resizable(true)
                .default_width(default_width)
                .width_range(280.0..=560.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Image editor");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Exit edit mode; keep this working copy").clicked() {
                                action = Some(EditAction::ExitMode);
                            }
                        });
                    });
                    ui.separator();

                    if let (Some((width, height)), Some(path)) = (dimensions, source_path.as_ref()) {
                        let full_filename = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let display_filename = crate::app::ui::sidebar::middle_ellipsis(
                            &full_filename,
                            (ui.available_width() / 7.2).floor() as usize,
                        );
                        ui.add(egui::Label::new(
                            egui::RichText::new(display_filename).strong(),
                        ).truncate()).on_hover_text(full_filename);
                        ui.horizontal(|ui| {
                            ui.label(format!("{width} × {height}"));
                            ui.separator();
                            ui.label(if dirty { "Unsaved changes" } else { "No unsaved changes" });
                        });
                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            if ui.add_enabled(can_undo, egui::Button::new("↶ Undo")).clicked() {
                                action = Some(EditAction::Undo);
                            }
                            if ui.add_enabled(can_redo, egui::Button::new("↷ Redo")).clicked() {
                                action = Some(EditAction::Redo);
                            }
                            if ui.button("Reset all").clicked() {
                                action = Some(EditAction::Reset);
                            }
                        });

                        ui.separator();
                        ui.label(egui::RichText::new("Transform").strong());
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("↺").clicked() { action = Some(EditAction::RotateLeft); }
                            if ui.button("↻").clicked() { action = Some(EditAction::RotateRight); }
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("↔️").clicked() { action = Some(EditAction::FlipHorizontal); }
                            if ui.button("↕️").clicked() { action = Some(EditAction::FlipVertical); }
                        });

                        ui.separator();
                        ui.label(egui::RichText::new("✂️ Crop").strong());
                        let crop_button = if crop_tool_active { "Stop crop selection" } else { "Draw crop on image" };
                        if ui.button(crop_button).clicked() {
                            crop_tool_active = !crop_tool_active;
                            if crop_tool_active {
                                crop_selection = None;
                            }
                        }
                        if crop_tool_active {
                            ui.label("Drag a rectangle over the image. Then apply it here.");
                        }
                        if let Some([x, y, width, height]) = crop_selection {
                            ui.label(format!("Selection: {width} × {height} at ({x}, {y})"));
                            ui.horizontal(|ui| {
                                if ui.button("Apply crop").clicked() {
                                    action = Some(EditAction::Crop);
                                }
                                if ui.small_button("Clear").clicked() {
                                    crop_selection = None;
                                    crop_x = 0;
                                    crop_y = 0;
                                    crop_width = dimensions.map(|d| d.0).unwrap_or(1);
                                    crop_height = dimensions.map(|d| d.1).unwrap_or(1);
                                }
                            });
                        } else {
                            ui.label("No crop selection");
                        }
                        ui.collapsing("Enter crop coordinates", |ui| {
                            let (max_width, max_height) = dimensions.unwrap_or((1, 1));
                            let mut numeric_crop_changed = false;
                            ui.horizontal(|ui| {
                                ui.label("X");
                                numeric_crop_changed |= ui.add(egui::DragValue::new(&mut crop_x).range(0..=max_width.saturating_sub(1))).changed();
                                ui.label("Y");
                                numeric_crop_changed |= ui.add(egui::DragValue::new(&mut crop_y).range(0..=max_height.saturating_sub(1))).changed();
                            });
                            ui.horizontal(|ui| {
                                ui.label("W");
                                numeric_crop_changed |= ui.add(egui::DragValue::new(&mut crop_width).range(1..=max_width.max(1))).changed();
                                ui.label("H");
                                numeric_crop_changed |= ui.add(egui::DragValue::new(&mut crop_height).range(1..=max_height.max(1))).changed();
                            });
                            if numeric_crop_changed {
                                crop_selection = Some([
                                    crop_x.min(max_width.saturating_sub(1)),
                                    crop_y.min(max_height.saturating_sub(1)),
                                    crop_width.max(1).min(max_width.saturating_sub(crop_x.min(max_width.saturating_sub(1))).max(1)),
                                    crop_height.max(1).min(max_height.saturating_sub(crop_y.min(max_height.saturating_sub(1))).max(1)),
                                ]);
                            }
                            if ui.button("Apply numeric crop").clicked() {
                                action = Some(EditAction::Crop);
                                crop_selection = Some([crop_x, crop_y, crop_width, crop_height]);
                            }
                        });

                        ui.separator();
                        ui.label(egui::RichText::new("🎨 Colour adjustments").strong());
                        let response = ui.add(egui::Slider::new(&mut adjustments.brightness, -100..=100).text("☀️ Brightness"));
                        adjustments_changed |= response.changed();
                        adjustment_dragging |= response.dragged();
                        adjustment_drag_stopped |= response.drag_stopped();

                        let response = ui.add(egui::Slider::new(&mut adjustments.contrast, -100..=100).text("◐ Contrast"));
                        adjustments_changed |= response.changed();
                        adjustment_dragging |= response.dragged();
                        adjustment_drag_stopped |= response.drag_stopped();

                        let response = ui.add(egui::Slider::new(&mut adjustments.saturation, 0..=200).text("🌈 Saturation"));
                        adjustments_changed |= response.changed();
                        adjustment_dragging |= response.dragged();
                        adjustment_drag_stopped |= response.drag_stopped();

                        let response = ui.add(egui::Slider::new(&mut adjustments.hue, -180..=180).text("🎨 Hue"));
                        adjustments_changed |= response.changed();
                        adjustment_dragging |= response.dragged();
                        adjustment_drag_stopped |= response.drag_stopped();

                        if ui.button("Reset colour adjustments").clicked() {
                            adjustments = ImageAdjustments::default();
                            reset_adjustments_clicked = true;
                            adjustments_changed = adjustments != original_adjustments;
                        }

                        ui.separator();
                        ui.label(egui::RichText::new("Export").strong());
                        egui::ComboBox::from_id_salt("edit_output_format")
                            .selected_text(output_format.label())
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for format in EditOutputFormat::ALL {
                                    ui.selectable_value(&mut output_format, format, format.label());
                                }
                            });
                        if output_format == EditOutputFormat::Jpeg {
                            ui.add(egui::Slider::new(&mut quality, 1..=100).text("JPEG quality"));
                            ui.small("Higher quality reduces compression artifacts and usually increases file size.");
                        } else {
                            ui.small("Quality is adjustable for JPEG. Other formats use their normal encoder settings.");
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Save As…").clicked() {
                                action = Some(EditAction::SaveAs);
                            }
                            if ui.button("Overwrite original…").clicked() {
                                action = Some(EditAction::SaveOverwrite);
                            }
                        });
                        if ui.button("Discard working copy…").clicked() {
                            action = Some(EditAction::Discard);
                        }
                        ui.add_space(4.0);
                        ui.small("Edits remain in memory until saved. Re-encoding can remove EXIF and other embedded metadata.");
                    } else {
                        ui.label("Choose a supported, fully loaded image file to begin editing.");
                    }

                    if let Some(status) = &self.edit_status {
                        ui.separator();
                        ui.label(status);
                    }
                });

            let width = panel_response.response.rect.width().clamp(280.0, 560.0);
            let previous_width = self.settings_manager.get().settings_sidebar_width;
            if !ctx.input(|input| input.pointer.any_down()) && (width - previous_width).abs() > 2.0
            {
                let _ = self
                    .settings_manager
                    .update(|settings| settings.settings_sidebar_width = width);
            }

            self.edit_output_format = output_format;
            self.save_quality = quality.clamp(1, 100);
            self.crop_tool_active = crop_tool_active;
            self.crop_x = crop_x;
            self.crop_y = crop_y;
            self.crop_width = crop_width.max(1);
            self.crop_height = crop_height.max(1);
            self.crop_selection = crop_selection;

            if adjustments_changed {
                if let Some(session) = self.edit_session.as_mut() {
                    if !self.adjustment_gesture_active {
                        session.record_current_state();
                    }
                    session.set_adjustments(adjustments);
                }
                self.adjustment_gesture_active = adjustment_dragging;
                self.refresh_adjustment_preview(ctx, adjustment_dragging);
                if reset_adjustments_clicked || !adjustment_dragging {
                    self.adjustment_gesture_active = false;
                }
            }
            if adjustment_drag_stopped {
                self.adjustment_gesture_active = false;
                self.refresh_adjustment_preview(ctx, false);
            }

            if let Some(action) = action {
                match action {
                    EditAction::RotateLeft => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.rotate_left();
                        }
                        self.crop_selection = None;
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::RotateRight => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.rotate_right();
                        }
                        self.crop_selection = None;
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::FlipHorizontal => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.flip_horizontal();
                        }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::FlipVertical => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.flip_vertical();
                        }
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Undo => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.undo();
                        }
                        self.crop_selection = None;
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Redo => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.redo();
                        }
                        self.crop_selection = None;
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::Crop => {
                        let result = self.edit_session.as_mut().map(|session| {
                            session.crop(
                                self.crop_x,
                                self.crop_y,
                                self.crop_width,
                                self.crop_height,
                            )
                        });
                        match result {
                            Some(Ok(())) => {
                                self.edit_status = Some("Crop applied to the working copy.".into());
                                self.crop_selection = None;
                                self.crop_tool_active = false;
                                self.refresh_edit_texture(ctx);
                            }
                            Some(Err(error)) => self.edit_status = Some(error),
                            None => {}
                        }
                    }
                    EditAction::Reset => {
                        if let Some(session) = self.edit_session.as_mut() {
                            session.reset();
                        }
                        self.crop_selection = None;
                        self.crop_tool_active = false;
                        self.refresh_edit_texture(ctx);
                    }
                    EditAction::SaveOverwrite => {
                        if let Some(session) = self.edit_session.as_ref() {
                            self.confirm_overwrite_path = Some(session.source_path.clone());
                            self.overwrite_is_save_as = false;
                        }
                    }
                    EditAction::SaveAs => self.request_save_as(ctx),
                    EditAction::ExitMode => {
                        self.show_edit_menu = false;
                        self.crop_tool_active = false;
                        self.crop_drag_start_image = None;
                        self.adjustment_gesture_active = false;
                        self.refresh_adjustment_preview(ctx, false);
                    }
                    EditAction::Discard => {
                        if dirty {
                            self.confirm_discard_edits = true;
                        } else {
                            self.restore_original_preview(ctx);
                        }
                    }
                }
            }
        }

        // Confirmation dialogs can remain visible even while the user has temporarily
        // exited edit mode.
        if self.confirm_overwrite_path.is_some() {
            let mut open = true;
            let target = self.confirm_overwrite_path.clone().unwrap();
            egui::Window::new("Confirm overwrite")
                .id(egui::Id::new("edit_overwrite_confirmation"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("Replace this existing file?\n{}", target.display()));
                    ui.label("The existing file is backed up temporarily during replacement. Embedded metadata may be lost.");
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            let is_save_as = self.overwrite_is_save_as;
                            self.save_edit_to(ctx, &target, true, is_save_as);
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
                    ui.label("Discard the working copy? The source file has not been changed unless you saved over it.");
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

    /// Convert crop tool drags from screen points into image pixel coordinates.
    pub fn handle_crop_mouse_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        image_rect: egui::Rect,
    ) {
        let Some((image_width, image_height)) = self
            .edit_session
            .as_ref()
            .map(|session| session.image.dimensions())
        else {
            return;
        };
        if response.drag_started_by(egui::PointerButton::Primary) {
            let press_origin = ctx
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if let Some(pos) = press_origin.filter(|pos| image_rect.contains(*pos)) {
                self.crop_drag_start_image =
                    Some(screen_to_image(pos, image_rect, image_width, image_height));
                self.crop_selection = None;
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            || response.drag_stopped_by(egui::PointerButton::Primary)
        {
            if let (Some(start), Some(pos)) =
                (self.crop_drag_start_image, response.interact_pointer_pos())
            {
                let current = screen_to_image(pos, image_rect, image_width, image_height);
                let selection = image_selection(start, current, image_width, image_height);
                if selection[2] > 1 && selection[3] > 1 {
                    self.crop_selection = Some(selection);
                    self.crop_x = selection[0];
                    self.crop_y = selection[1];
                    self.crop_width = selection[2];
                    self.crop_height = selection[3];
                }
            }
        }
        if response.drag_stopped() {
            self.crop_drag_start_image = None;
        }
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    /// Draw the selection and dim the image outside it, as in common crop tools.
    pub fn paint_crop_selection(&self, painter: &egui::Painter, image_rect: egui::Rect) {
        let Some([x, y, width, height]) = self.crop_selection else {
            return;
        };
        let Some((image_width, image_height)) = self
            .edit_session
            .as_ref()
            .map(|session| session.image.dimensions())
        else {
            return;
        };
        if image_width == 0 || image_height == 0 {
            return;
        }
        let sx = image_rect.width() / image_width as f32;
        let sy = image_rect.height() / image_height as f32;
        let selection = egui::Rect::from_min_max(
            egui::pos2(
                image_rect.min.x + x as f32 * sx,
                image_rect.min.y + y as f32 * sy,
            ),
            egui::pos2(
                image_rect.min.x + (x + width).min(image_width) as f32 * sx,
                image_rect.min.y + (y + height).min(image_height) as f32 * sy,
            ),
        );
        let shade = egui::Color32::from_black_alpha(115);
        if selection.min.y > image_rect.min.y {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    image_rect.min,
                    egui::pos2(image_rect.max.x, selection.min.y),
                ),
                0.0,
                shade,
            );
        }
        if selection.max.y < image_rect.max.y {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(image_rect.min.x, selection.max.y),
                    image_rect.max,
                ),
                0.0,
                shade,
            );
        }
        if selection.min.x > image_rect.min.x {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(image_rect.min.x, selection.min.y),
                    egui::pos2(selection.min.x, selection.max.y),
                ),
                0.0,
                shade,
            );
        }
        if selection.max.x < image_rect.max.x {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(selection.max.x, selection.min.y),
                    egui::pos2(image_rect.max.x, selection.max.y),
                ),
                0.0,
                shade,
            );
        }

        let border = egui::Color32::from_rgb(245, 245, 245);
        let accent = egui::Color32::from_rgb(70, 165, 255);
        let stroke = egui::Stroke::new(1.5 as f32, border);
        for (a, b) in [
            (selection.left_top(), selection.right_top()),
            (selection.right_top(), selection.right_bottom()),
            (selection.right_bottom(), selection.left_bottom()),
            (selection.left_bottom(), selection.left_top()),
        ] {
            painter.line_segment([a, b], stroke);
        }
        let handle = 5.0;
        for point in [
            selection.left_top(),
            selection.right_top(),
            selection.left_bottom(),
            selection.right_bottom(),
        ] {
            painter.rect_filled(
                egui::Rect::from_center_size(point, egui::vec2(handle, handle)),
                1.0,
                accent,
            );
        }
    }
}

fn screen_to_image(
    pos: egui::Pos2,
    image_rect: egui::Rect,
    image_width: u32,
    image_height: u32,
) -> egui::Pos2 {
    let x = ((pos.x - image_rect.min.x) / image_rect.width().max(1.0)).clamp(0.0, 1.0)
        * image_width as f32;
    let y = ((pos.y - image_rect.min.y) / image_rect.height().max(1.0)).clamp(0.0, 1.0)
        * image_height as f32;
    egui::pos2(x, y)
}

fn image_selection(
    start: egui::Pos2,
    end: egui::Pos2,
    image_width: u32,
    image_height: u32,
) -> [u32; 4] {
    let left = start.x.min(end.x).floor().clamp(0.0, image_width as f32) as u32;
    let top = start.y.min(end.y).floor().clamp(0.0, image_height as f32) as u32;
    let right = start.x.max(end.x).ceil().clamp(0.0, image_width as f32) as u32;
    let bottom = start.y.max(end.y).ceil().clamp(0.0, image_height as f32) as u32;
    [
        left.min(image_width.saturating_sub(1)),
        top.min(image_height.saturating_sub(1)),
        right.saturating_sub(left).max(1).min(image_width),
        bottom.saturating_sub(top).max(1).min(image_height),
    ]
}
