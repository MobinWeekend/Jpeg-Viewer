use super::helpers::{format_file_size, icon_button};
use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    /// Render the metadata button and its popup.
    pub fn file_metadata_button(&mut self, ui: &mut egui::Ui) {
        let button_text = if let Some(metadata) = &self.file_metadata {
            let size = metadata
                .file_size
                .map(format_file_size)
                .unwrap_or_else(|| "—".to_string());

            let resolution = metadata
                .dimensions
                .map(|(w, h)| format!("{}×{}", w, h))
                .unwrap_or_else(|| "—".to_string());

            let aspect_ratio = metadata.aspect_ratio.as_deref().unwrap_or("");
            let created = metadata.created.as_deref().unwrap_or("");

            format!(
                "{}  -  {}  {}  -  {}",
                size, resolution, aspect_ratio, created
            )
        } else {
            "Info".to_string()
        };

        let response = icon_button(ui, &button_text, "File Info");

        if response.clicked() {
            self.file_metadata_open = !self.file_metadata_open;
        }

        if !self.file_metadata_open {
            return;
        }

        let ctx = ui.ctx();
        let popup_pos = response.rect.left_bottom() + egui::vec2(0.0, 8.0);

        let popup_response = egui::Area::new(egui::Id::new("file_metadata_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .max_height(900.0)
                            .max_width(320.0)
                            .auto_shrink([true, true])
                            .show(ui, |ui| {
                                if let Some(metadata) = &self.file_metadata {
                                    self.render_file_metadata(ui, metadata);
                                }
                            });
                    });
            });

        // Close when clicking anywhere outside the button or popup.
        if ctx.input(|input| input.pointer.any_click()) {
            if let Some(pointer_pos) = ctx.input(|input| input.pointer.interact_pos()) {
                let clicked_button = response.rect.contains(pointer_pos);
                let clicked_popup = popup_response.response.rect.contains(pointer_pos);

                if !clicked_button && !clicked_popup {
                    self.file_metadata_open = false;
                }
            }
        }
    }
}
