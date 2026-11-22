use super::*;

impl PopoutWindow {
    pub(super) fn paint_image(
        &self,
        ui: &egui::Ui,
        texture: &egui::TextureHandle,
        rect: egui::Rect,
        tint: egui::Color32,
    ) {
        if self.rotation_radians.abs() < 0.001 {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                tint,
            );
            return;
        }
        let center = rect.center();
        let half = rect.size() * 0.5;
        let (sin, cos) = self.rotation_radians.sin_cos();
        let rotate = |x: f32, y: f32| {
            center + egui::vec2(x * cos - y * sin, x * sin + y * cos)
        };
        let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
        mesh.vertices = vec![
            egui::epaint::Vertex {
                pos: rotate(-half.x, -half.y),
                uv: egui::pos2(0.0, 0.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(half.x, -half.y),
                uv: egui::pos2(1.0, 0.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(half.x, half.y),
                uv: egui::pos2(1.0, 1.0),
                color: tint,
            },
            egui::epaint::Vertex {
                pos: rotate(-half.x, half.y),
                uv: egui::pos2(0.0, 1.0),
                color: tint,
            },
        ];
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}
