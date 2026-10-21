//! Resizable file and image-information sidebar.
use crate::app::types::ViewerApp;
use crate::image_entry::ImageEntry;
use eframe::egui;

pub fn render_left_sidebar(app: &mut ViewerApp, ctx: &egui::Context) {
    if !app.settings_manager.get().show_file_sidebar {
        app.queue_drop_rect = None;
        return;
    }

    app.collect_sidebar_thumbnails(ctx);
    let default_width = app
        .settings_manager
        .get()
        .file_sidebar_width
        .clamp(180.0, 1024.0);
    let response = egui::SidePanel::left("file_sidebar")
        .resizable(true)
        .default_width(default_width)
        .width_range(180.0..=1024.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("JPEG Viewer");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Hide file sidebar").clicked() {
                        let _ = app.settings_manager.update(|settings| settings.show_file_sidebar = false);
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.selectable_label(!app.sidebar_queue_tab, "Browse").clicked() {
                    app.sidebar_queue_tab = false;
                    app.queue_drop_rect = None;
                }
                if ui.selectable_label(app.sidebar_queue_tab, format!("Queue ({})", app.queue_paths.len())).clicked() {
                    app.sidebar_queue_tab = true;
                }
                if !app.sidebar_queue_tab && ui.small_button("Info").clicked() {
                    app.file_metadata_open = !app.file_metadata_open;
                }
            });
            ui.separator();

            if app.sidebar_queue_tab {
                app.render_queue_sidebar(ui, ctx);
            } else {
                app.queue_drop_rect = None;
                ui.label(format!("{} images", app.image_entries.len()));
                let entries = app.image_entries.clone();
                let available = ui.available_width();
                let columns = if available >= 320.0 { 2 } else { 1 };
                let mut selected = None;
                egui::ScrollArea::vertical()
                    .id_salt("file_sidebar_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("file_sidebar_thumbnail_grid")
                            .num_columns(columns)
                            .spacing(egui::vec2(8.0, 10.0))
                            .show(ui, |ui| {
                                for (index, entry) in entries.iter().enumerate() {
                                    let name = entry_name(entry);
                                    let clicked = match entry {
                                        ImageEntry::File(path) => {
                                            let key = format!("file:{}", if path.is_absolute() {
                                                path.display().to_string()
                                            } else {
                                                std::env::current_dir().unwrap_or_default().join(path).display().to_string()
                                            });
                                            app.render_thumbnail_card(
                                                ui, path, &key, &name, index == app.current_index, 112.0
                                            )
                                        }
                                        _ => {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(ui.available_width().min(124.0).max(80.0), 88.0),
                                                egui::Sense::click()
                                            );
                                            ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.noninteractive.bg_fill);
                                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Archive entry", egui::FontId::proportional(11.0), ui.visuals().weak_text_color());
                                            let text = ui.add_sized([rect.width(), 32.0], egui::Button::selectable(index == app.current_index, middle_ellipsis(&name, (rect.width() / 6.0) as usize)));
                                            response.clicked() || text.on_hover_text(&name).clicked()
                                        }
                                    };
                                    if clicked && index != app.current_index {
                                        if app.edit_session.as_ref().is_some_and(|session| session.dirty) {
                                            app.edit_status = Some("Save or discard unsaved edits before changing images.".into());
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
                                    if (index + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                                if entries.len() % columns != 0 {
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(index) = selected {
                    app.current_index = index;
                    app.load_current_image_with_cache();
                }

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
