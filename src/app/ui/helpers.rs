use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    /// Calculate the fit zoom for the image
    pub fn calculate_fit_zoom(&mut self, texture_size: egui::Vec2, available: egui::Vec2) -> bool {
        // Check if we should actually apply fit
        let should_fit = {
            let width = texture_size.x;
            let height = texture_size.y;
            let ratio = if width > height {
                height / width
            } else {
                width / height
            };
            // Only apply fit if ratio is >= 0.1 (not extreme aspect ratio)
            ratio >= 0.1
        };
        let zoom_x = available.x / texture_size.x;
        let zoom_y = available.y / texture_size.y;
        if should_fit {
            let fit_zoom = zoom_x.min(zoom_y).min(1.0);
            self.zoom = fit_zoom;
            self.pan = egui::Vec2::ZERO;
            true // Applied fit all sides
        } else {
            // For extreme ratios
            let max_fit_zoom = zoom_x.max(zoom_y).min(1.0);
            self.zoom = max_fit_zoom;
            let snap_to_top = (texture_size.y / 2.0) - (available.y / 2.0) / max_fit_zoom;
            self.pan = egui::Vec2::new(0.0, snap_to_top);
            false // fit to one side
        }
    }

    /// Get the image rectangle centered in available space
    pub fn get_image_rect(&self, texture_size: egui::Vec2, center: egui::Pos2) -> egui::Rect {
        let display_size = texture_size * self.zoom;
        egui::Rect::from_center_size(center + self.pan * self.zoom, display_size)
    }
}
pub fn render_drag_area(ctx: &egui::Context, ui: &mut egui::Ui) {
    let drag_response = ui.interact(
        ui.max_rect(),
        ui.id().with("window_drag"),
        egui::Sense::click_and_drag(),
    );

    if drag_response.double_clicked() {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}
