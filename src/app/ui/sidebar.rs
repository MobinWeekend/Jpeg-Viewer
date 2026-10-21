//! Resizable file and image-information sidebar.
use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use eframe::egui;

pub fn render_left_sidebar(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.settings_manager.get().show_file_sidebar {
        return;
    }

    let default_width = app
        .settings_manager
        .get()
        .file_sidebar_width
        .clamp(180.0, 520.0);
    let response = egui::SidePanel::left("file_sidebar")
        .resizable(true)
        .default_width(default_width)
        .width_range(180.0..=520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Browse");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Hide file sidebar")
                        .clicked()
                    {
                        let _ = app
                            .settings_manager
                            .update(|settings| settings.show_file_sidebar = false);
                    }
                });
            });
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(format!("{} images", app.image_entries.len()));
                if ui.small_button("Info").clicked() {
                    app.file_metadata_open = !app.file_metadata_open;
                }
            });

            egui::ScrollArea::vertical()
                .id_salt("file_sidebar_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut selected = None;
                    for (index, entry) in app.image_entries.iter().enumerate() {
                        let full_name = entry_name(entry);
                        let available = ui.available_width().max(40.0);
                        let display =
                            middle_ellipsis(&full_name, (available / 7.2).floor() as usize);
                        let response = ui
                            .add_sized(
                                [available, 24.0],
                                egui::Button::selectable(index == app.current_index, display),
                            )
                            .on_hover_text(&full_name);
                        if response.clicked() && index != app.current_index {
                            if app.edit_session.as_ref().is_some_and(|session| session.dirty) {
                                app.edit_status = Some(
                                    "Save or discard unsaved edits before changing images.".into(),
                                );
                                app.show_edit_menu = true;
                                app.show_settings_menu = false;
                            } else {
                                app.edit_session = None;
                                app.show_edit_menu = false;
                                app.crop_tool_active = false;
                                app.crop_drag_start_image = None;
                                app.crop_selection = None;
                                selected = Some(index);
                            }
                        }
                    }
                    if let Some(index) = selected {
                        app.current_index = index;
                        app.load_current_image_with_cache();
                    }
                });

            if app.file_metadata_open {
                ui.separator();
                ui.heading("Image information");
                egui::ScrollArea::vertical()
                    .id_salt("file_sidebar_metadata")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        if let Some(metadata) = app.file_metadata.clone() {
                            app.render_file_metadata(ui, &metadata);
                        } else {
                            ui.label("Metadata is not available yet.");
                        }
                    });
            }
        });

    let width = response.response.rect.width().clamp(180.0, 520.0);
    let previous = app.settings_manager.get().file_sidebar_width;
    if !ctx.input(|input| input.pointer.any_down()) && (width - previous).abs() > 2.0 {
        let _ = app
            .settings_manager
            .update(|settings| settings.file_sidebar_width = width);
    }
}

fn entry_name(entry: &ImageEntry) -> String {
    match entry {
        ImageEntry::File(path) => path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        ImageEntry::Zip(entry) => entry.name.clone(),
        ImageEntry::S7z(entry) => entry.name.clone(),
        ImageEntry::Rar(entry) => entry.name.clone(),
    }
}

pub(super) fn middle_ellipsis(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars || max_chars < 5 {
        return if chars.len() <= max_chars {
            text.to_string()
        } else {
            chars.into_iter().take(max_chars).collect()
        };
    }
    let keep = max_chars - 3;
    let left = (keep * 3 + 2) / 5;
    let right = keep - left;
    format!(
        "{}...{}",
        chars[..left].iter().collect::<String>(),
        chars[chars.len() - right..].iter().collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::middle_ellipsis;

    #[test]
    fn long_names_are_middle_truncated() {
        assert_eq!(
            middle_ellipsis("somephotoIhavethatIshotLongAgoNamed012.jpeg", 27),
            "somephotoIhave...ed012.jpeg"
        );
    }

    #[test]
    fn short_names_are_unchanged() {
        assert_eq!(middle_ellipsis("cat.png", 20), "cat.png");
    }
}
