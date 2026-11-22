use super::*;

impl PopoutManager {
    pub(super) fn render_jvp_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.save_dialog.take() {
            let mut keep_open = true;
            let mut close_dialog = false;
            let mut save_requested = false;
            egui::Window::new("Save Pop-out Preset (.jvp)")
                .id(egui::Id::new(("popout_jvp_save_dialog", dialog.tab_id)))
                .collapsible(false)
                .resizable(false)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label("Choose how the current image is stored in this preset:");
                    ui.label(
                        egui::RichText::new("Bundling preserves the original file, including animation data.")
                            .weak()
                            .size(10.0),
                    );
                    ui.add_enabled_ui(dialog.image_entry.is_some(), |ui| {
                        ui.radio_value(
                            &mut dialog.choice,
                            SaveImageChoice::Bundle,
                            "Bundle image into the .jvp (portable)",
                        );
                        ui.radio_value(
                            &mut dialog.choice,
                            SaveImageChoice::Link,
                            "Link to the image on this drive",
                        );
                    });
                    ui.radio_value(
                        &mut dialog.choice,
                        SaveImageChoice::PresetOnly,
                        "Save options only (no image)",
                    );
                    if dialog.image_entry.is_none() {
                        ui.label(egui::RichText::new("No current image entry is available; only options can be saved.").weak().size(10.0));
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save as…").clicked() {
                            save_requested = true;
                            close_dialog = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                    });
                });
            if close_dialog {
                keep_open = false;
            }
            if save_requested {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JPEG Viewer Pop-out preset", &["jvp"])
                    .set_file_name("Pop-out.jvp")
                    .save_file()
                {
                    let path = ensure_jvp_extension(path);
                    match jvp::save_jvp(
                        &path,
                        &dialog.preset,
                        dialog.image_entry.as_ref(),
                        dialog.choice,
                    ) {
                        Ok(()) => {
                            self.status_message = Some(format!("Saved preset: {}", path.display()));
                        }
                        Err(error) => {
                            self.status_message = Some(format!("Could not save .jvp: {error}"));
                        }
                    }
                }
            } else if keep_open {
                self.save_dialog = Some(dialog);
            }
        }

        if let Some(dialog) = self.load_dialog.take() {
            let mut keep_open = true;
            let mut close_dialog = false;
            let mut load_image = false;
            let mut options_only = false;
            egui::Window::new("Load Pop-out Preset (.jvp)")
                .id(egui::Id::new(("popout_jvp_load_dialog", dialog.tab_id)))
                .collapsible(false)
                .resizable(false)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    if dialog.document.has_image() {
                        ui.label("This preset contains an image entry.");
                        ui.label("Do you also want to load that image into an Image tab?");
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Load image + options").clicked() {
                                load_image = true;
                                close_dialog = true;
                            }
                            if ui.button("Options only").clicked() {
                                options_only = true;
                                close_dialog = true;
                            }
                        });
                    } else {
                        ui.label("This preset contains options only.");
                        if ui.button("Apply options").clicked() {
                            options_only = true;
                            close_dialog = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            if close_dialog {
                keep_open = false;
            }

            if load_image {
                let preset = dialog.document.preset();
                match dialog.document.resolve_image() {
                    Ok(Some(entry)) => {
                        self.pending_image_open = Some((entry, preset));
                        self.status_message = Some("Opening the image from the preset…".into());
                    }
                    Ok(None) => {
                        self.assign_preset(dialog.tab_id, preset, ctx);
                        self.status_message = Some("Loaded pop-out options.".into());
                    }
                    Err(error) => {
                        self.status_message = Some(format!("Could not load the preset image: {error}"));
                    }
                }
            } else if options_only {
                self.assign_preset(dialog.tab_id, dialog.document.preset(), ctx);
                self.status_message = Some("Loaded pop-out options without changing the image.".into());
            } else if keep_open {
                self.load_dialog = Some(dialog);
            }
        }
    }
}
