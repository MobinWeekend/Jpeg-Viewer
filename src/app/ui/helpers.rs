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

    /// Keep a useful portion of the image inside the viewport without trapping
    /// large, zoomed-in images inside the viewport. The minimum visible amount
    /// is 20% of the smaller of the image and viewport dimensions, independently
    /// on each axis. This keeps the bound valid even when the image is much larger
    /// than the viewport.
    pub fn clamp_pan_to_viewport(&mut self, viewport: egui::Rect, texture_size: egui::Vec2) {
        let zoom = self.zoom.max(0.001);
        let image_size = texture_size * zoom;
        let image_center = viewport.center() + self.pan * zoom;

        let min_visible_x = image_size.x.min(viewport.width()).max(0.0) * 0.20;
        let min_visible_y = image_size.y.min(viewport.height()).max(0.0) * 0.20;
        let min_center_x = viewport.left() + min_visible_x - image_size.x * 0.5;
        let max_center_x = viewport.right() - min_visible_x + image_size.x * 0.5;
        let min_center_y = viewport.top() + min_visible_y - image_size.y * 0.5;
        let max_center_y = viewport.bottom() - min_visible_y + image_size.y * 0.5;

        let clamped_center = egui::pos2(
            image_center.x.clamp(min_center_x, max_center_x),
            image_center.y.clamp(min_center_y, max_center_y),
        );
        self.pan += (clamped_center - image_center) / zoom;
    }

    /// Get the image rectangle centered in available space
    pub fn get_image_rect(&self, texture_size: egui::Vec2, center: egui::Pos2) -> egui::Rect {
        let display_size = texture_size * self.zoom;
        egui::Rect::from_center_size(center + self.pan * self.zoom, display_size)
    }
}

/*
pub fn render_drag_area(ctx: &egui::Context, ui: &mut egui::Ui) {
    let drag_response = ui.interact(
        ui.max_rect(),
        ui.scope_id().with("window_drag"),
        egui::Sense::click_and_drag(),
    );

    if drag_response.double_clicked() {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}
 */

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes_f = bytes as f64;

    if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.2} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Paint the theme-tinted SVG sidebar/drawer icon. `show_sidebar` points the
/// arrow into the window; `false` points it back toward the left edge.
pub fn paint_sidebar_drawer_icon(ui: &egui::Ui, rect: egui::Rect, show_sidebar: bool) {
    let icon = if show_sidebar {
        super::svg_icons::SvgIcon::SidebarShow
    } else {
        super::svg_icons::SvgIcon::SidebarHide
    };
    super::svg_icons::paint_svg_icon(ui, rect, icon, ui.visuals().text_color());
}
