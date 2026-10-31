use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    pub(crate) fn prepare_inspect_source(&mut self, ctx: &egui::Context) {
        let Some(path) = self.current_image_path.clone() else {
            self.edit_status = Some("Inspect requires an image with a readable file path.".into());
            self.inspect_mode = false;
            return;
        };
        let decoded = std::fs::read(&path)
            .ok()
            .and_then(|bytes| crate::decoder::decode_bytes(&bytes).ok());
        let Some(decoded) = decoded else {
            self.edit_status = Some("Could not decode image channels for Inspect.".into());
            self.inspect_mode = false;
            return;
        };
        let Some(source) = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.data)
        else {
            self.inspect_mode = false;
            return;
        };
        self.inspect_source = Some(source);
        self.inspect_source_path = Some(path);
        self.refresh_inspect_texture(ctx);
    }

    pub(crate) fn refresh_inspect_texture(&mut self, ctx: &egui::Context) {
        let Some(source) = self.inspect_source.as_ref() else {
            return;
        };
        let channels = self.inspect_channels;
        let enabled_count = channels.iter().filter(|enabled| **enabled).count();
        let only = channels.iter().position(|enabled| *enabled);
        let mut output = source.clone();
        for pixel in output.pixels_mut() {
            let original = [pixel[0], pixel[1], pixel[2], pixel[3]];
            if enabled_count == 1 {
                let index = only.unwrap_or(0);
                let value = original[index];
                if self.inspect_colorized && index < 3 {
                    pixel[0] = if index == 0 { value } else { 0 };
                    pixel[1] = if index == 1 { value } else { 0 };
                    pixel[2] = if index == 2 { value } else { 0 };
                    pixel[3] = 255;
                } else {
                    pixel[0] = value;
                    pixel[1] = value;
                    pixel[2] = value;
                    pixel[3] = 255;
                }
            } else {
                pixel[0] = if channels[0] { original[0] } else { 0 };
                pixel[1] = if channels[1] { original[1] } else { 0 };
                pixel[2] = if channels[2] { original[2] } else { 0 };
                pixel[3] = if channels[3] { original[3] } else { 255 };
            }
        }
        let size = [output.width() as usize, output.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, output.as_raw());
        let texture_options = self.get_texture_options();
        if let Some(texture) = self.inspect_texture.as_mut() {
            texture.set(color, texture_options);
        } else {
            self.inspect_texture =
                Some(ctx.load_texture("inspect_channels", color, texture_options));
        }
    }
}
