use crate::app::types::ViewerApp;
use crate::decoder::default_registry;
use crate::decoder::format_detection::detect_format;
use crate::image_core::ImageFormat;
use crate::image_entry::ImageEntry;
use chrono::{DateTime, Local};
use eframe::egui;
use exif::{Reader as ExifReader, Value as ExifValue};
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug, Default)]
pub struct FileMetadata {
    pub name: Option<String>,
    pub extension: Option<String>,
    pub source: Option<String>,
    pub file_size: Option<u64>,
    pub modified: Option<String>,
    pub created: Option<String>,
    pub accessed: Option<String>,
    pub read_only: Option<bool>,
    pub format: Option<ImageFormat>,
    pub dimensions: Option<(u32, u32)>,
    pub aspect_ratio: Option<String>,
    pub megapixels: Option<f64>,
    pub pixel_count: Option<u64>,
    pub decoded_color: Option<String>,
    pub decoded_bit_depth: Option<String>,
    pub estimated_uncompressed_size: Option<u64>,
    pub animated: Option<bool>,
    pub frame_count: Option<usize>,
    pub dpi: Option<(f64, f64)>,
    pub exif: Vec<(String, String)>,
    pub mismatch: bool,
    pub current_extension: Option<String>,
}

impl ViewerApp {
    pub fn build_file_metadata(&self) -> FileMetadata {
        let Some(entry) = self.image_entries.get(self.current_index) else {
            return FileMetadata::default();
        };

        let bytes = read_entry_bytes(entry).ok();
        let format = bytes.as_deref().and_then(detect_format);
        let dimensions = bytes.as_deref().and_then(|bytes| {
            format.and_then(|format| default_registry().dimensions(bytes, format).ok())
        });

        let mut metadata = FileMetadata {
            name: Some(self.get_current_filename()),
            format,
            dimensions,
            ..Default::default()
        };

        let source_path = match entry {
            ImageEntry::File(path) => Some(path.clone()),
            ImageEntry::Zip(zip) => Some(zip.archive_path.clone()),
            ImageEntry::S7z(s7z) => Some(s7z.archive_path.clone()),
            ImageEntry::Rar(rar) => Some(rar.archive_path.clone()),
        };

        metadata.source = match entry {
            ImageEntry::File(_) => source_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            ImageEntry::Zip(zip) => Some(format!(
                "{}#{}",
                zip.archive_path.to_string_lossy(),
                zip.name
            )),
            ImageEntry::S7z(s7z) => Some(format!(
                "{}#{}",
                s7z.archive_path.to_string_lossy(),
                s7z.name
            )),
            ImageEntry::Rar(rar) => Some(format!(
                "{}#{}",
                rar.archive_path.to_string_lossy(),
                rar.name
            )),
        };

        if let ImageEntry::File(path) = entry {
            metadata.extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());

            metadata.current_extension = metadata.extension.clone();

            if let Ok(fs) = std::fs::metadata(path) {
                metadata.file_size = Some(fs.len());
                metadata.modified = fs.modified().ok().map(Self::format_metadata_system_time);
                metadata.created = fs.created().ok().map(Self::format_metadata_system_time);
                metadata.accessed = fs.accessed().ok().map(Self::format_metadata_system_time);
                metadata.read_only = Some(fs.permissions().readonly());
            }
        } else {
            metadata.extension = metadata.name.as_deref().and_then(|name| {
                PathBuf::from(name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase())
            });
            metadata.file_size = bytes.as_ref().map(|b| b.len() as u64);
        }

        if let Some((w, h)) = dimensions {
            if w > 0 && h > 0 {
                let pixels = w as u64 * h as u64;
                metadata.pixel_count = Some(pixels);
                metadata.megapixels = Some(pixels as f64 / 1_000_000.0);
                metadata.estimated_uncompressed_size = Some(pixels.saturating_mul(4));

                if let Some(label) = crate::app::aspect_ratio::AspectRatio::get_label(w, h) {
                    metadata.aspect_ratio = Some(label.to_string());
                }
            }
        }

        if let Some(bytes) = bytes.as_deref() {
            if let Some(format) = format {
                if let Ok(Some(color)) = default_registry().color_info(bytes, format) {
                    metadata.decoded_color = Some(color.description);
                    metadata.decoded_bit_depth =
                        Some(format!("{}-bit per channel", color.bits_per_channel));
                }
            }
            if matches!(format, Some(ImageFormat::Gif)) {
                if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes(bytes) {
                    metadata.frame_count = Some(gif.frame_count());
                    metadata.animated = Some(gif.is_animated());
                }
            }

            let (exif_fields, dpi) = read_exif(bytes);
            metadata.exif = exif_fields;
            metadata.dpi = dpi;
        }

        if let Some(detection) = &self.file_type_detection {
            metadata.mismatch = detection.mismatch;
            metadata.current_extension = detection.current_extension.clone();
            if metadata.format.is_none() {
                metadata.format = Some(detection.detected_format);
            }
        }

        metadata
    }

    pub fn render_file_metadata(&self, ui: &mut egui::Ui, metadata: &FileMetadata) {
        ui.set_max_width(420.0);

        if let Some(name) = &metadata.name {
            metadata_row(ui, "Name", name);
        }
        if let Some(extension) = &metadata.extension {
            metadata_row(ui, "Extension", &format!(".{extension}"));
        }
        if let Some(source) = &metadata.source {
            metadata_row(ui, "Source", source);
        }
        if let Some(size) = metadata.file_size {
            metadata_row(ui, "File Size", &Self::format_metadata_file_size(size));
        }
        if let Some(format) = metadata.format {
            metadata_row(ui, "Format", &format!("{format:?}"));
        }
        if let Some((w, h)) = metadata.dimensions {
            metadata_row(ui, "Dimensions", &format!("{w}×{h}"));
        }
        if let Some(ratio) = &metadata.aspect_ratio {
            metadata_row(ui, "Aspect Ratio", ratio);
        }
        if let Some(mp) = metadata.megapixels {
            metadata_row(ui, "Pixels", &format!("{mp:.2} MP"));
        }
        if let Some(pixels) = metadata.pixel_count {
            metadata_row(ui, "Pixel Count", &format_number(pixels));
        }
        if let Some(color) = &metadata.decoded_color {
            metadata_row(ui, "Decoded Color", color);
        }
        if let Some(depth) = &metadata.decoded_bit_depth {
            metadata_row(ui, "Decoded Depth", depth);
        }
        if let Some(size) = metadata.estimated_uncompressed_size {
            metadata_row(ui, "RGBA Size", &Self::format_metadata_file_size(size));
        }
        if let Some(animated) = metadata.animated {
            metadata_row(ui, "Animated", if animated { "Yes" } else { "No" });
        }
        if let Some(frames) = metadata.frame_count {
            metadata_row(ui, "Frames", &frames.to_string());
        }
        if let Some((x, y)) = metadata.dpi {
            metadata_row(ui, "Resolution", &format!("{x:.0} × {y:.0} DPI"));
        }
        if let Some(modified) = &metadata.modified {
            metadata_row(ui, "Modified", modified);
        }
        if let Some(created) = &metadata.created {
            metadata_row(ui, "Created", created);
        }
        if let Some(accessed) = &metadata.accessed {
            metadata_row(ui, "Accessed", accessed);
        }
        if let Some(read_only) = metadata.read_only {
            metadata_row(ui, "Read-only", if read_only { "Yes" } else { "No" });
        }

        if metadata.mismatch {
            let current = metadata.current_extension.as_deref().unwrap_or("(none)");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "⚠ Extension '{}' does not match detected format",
                    current
                ))
                .size(11.0)
                .color(egui::Color32::YELLOW),
            );
        }

        if !metadata.exif.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("EXIF").size(12.0).strong());
            ui.add_space(4.0);

            for (name, value) in &metadata.exif {
                metadata_row(ui, name, value);
            }
        }
    }

    fn format_metadata_file_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{bytes} B")
        }
    }

    fn format_metadata_system_time(time: SystemTime) -> String {
        let datetime: DateTime<Local> = time.into();
        datetime.format("%b %d, %Y at %I:%M %p").to_string()
    }
}

fn metadata_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.label(egui::RichText::new(label).size(11.0).weak());
        ui.add_space(8.0);
        ui.label(egui::RichText::new(value).size(12.0));
    });
    ui.add_space(3.0);
}

fn format_number(value: u64) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn read_entry_bytes(entry: &ImageEntry) -> Result<Vec<u8>, String> {
    match entry {
        ImageEntry::File(path) => std::fs::read(path).map_err(|e| e.to_string()),
        ImageEntry::Zip(zip) => {
            let file = std::fs::File::open(&zip.archive_path).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut entry = archive
                .by_index(zip.entry_index)
                .map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            Ok(bytes)
        }
        ImageEntry::S7z(s7z) => {
            let mut reader = sevenz_rust2::ArchiveReader::open(
                &s7z.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|e| e.to_string())?;
            reader.read_file(&s7z.name).map_err(|e| e.to_string())
        }
        ImageEntry::Rar(rar) => {
            let mut archive = unrar::Archive::new(&rar.archive_path)
                .open_for_processing()
                .map_err(|e| e.to_string())?;

            loop {
                let header = archive
                    .read_header()
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("File not found in archive: {}", rar.name))?;

                let filename = header.entry().filename.to_string_lossy().to_string();
                if filename == rar.name {
                    let (bytes, _) = header.read().map_err(|e| e.to_string())?;
                    return Ok(bytes);
                }

                archive = header.skip().map_err(|e| e.to_string())?;
            }
        }
    }
}

fn read_exif(bytes: &[u8]) -> (Vec<(String, String)>, Option<(f64, f64)>) {
    let mut cursor = Cursor::new(bytes);
    let exif = match ExifReader::new().read_from_container(&mut cursor) {
        Ok(exif) => exif,
        Err(_) => return (Vec::new(), None),
    };

    let wanted = [
        "Make",
        "Model",
        "LensModel",
        "DateTimeOriginal",
        "ExposureTime",
        "FNumber",
        "PhotographicSensitivity",
        "ISOSpeedRatings",
        "FocalLength",
        "ExposureBiasValue",
        "Flash",
        "Orientation",
        "Software",
        "Artist",
        "Copyright",
    ];

    let mut fields = Vec::new();
    for field in exif.fields() {
        let tag_name = format!("{:?}", field.tag);
        if wanted.iter().any(|name| *name == tag_name) {
            let value = field.display_value().with_unit(&exif).to_string();
            if !value.is_empty() {
                let display_name = match tag_name.as_str() {
                    "PhotographicSensitivity" | "ISOSpeedRatings" => "ISO",
                    "DateTimeOriginal" => "Date Taken",
                    "LensModel" => "Lens",
                    "FNumber" => "Aperture",
                    "ExposureTime" => "Shutter Speed",
                    "FocalLength" => "Focal Length",
                    "ExposureBiasValue" => "Exposure Bias",
                    other => other,
                };
                let mut fields: Vec<(String, String)> = Vec::new();
                if !fields.iter().any(|entry| entry.0 == display_name) {
                    fields.push((display_name.to_string(), value));
                }
            }
        }
    }

    if let Some(gps) = gps_coordinates(&exif) {
        fields.push(("GPS".to_string(), gps));
    }

    let x = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "XResolution");
    let y = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "YResolution");
    let dpi = match (x, y) {
        (Some(x), Some(y)) => match (rational_value(&x.value), rational_value(&y.value)) {
            (Some(x), Some(y)) if x > 0.0 && y > 0.0 => Some((x, y)),
            _ => None,
        },
        _ => None,
    };

    (fields, dpi)
}

fn gps_coordinates(exif: &exif::Exif) -> Option<String> {
    let latitude = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "GPSLatitude")
        .and_then(|f| gps_triplet(&f.value));
    let longitude = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "GPSLongitude")
        .and_then(|f| gps_triplet(&f.value));

    let lat_ref = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "GPSLatitudeRef")
        .map(|f| f.display_value().to_string());
    let lon_ref = exif
        .fields()
        .find(|f| format!("{:?}", f.tag) == "GPSLongitudeRef")
        .map(|f| f.display_value().to_string());

    match (latitude, longitude) {
        (Some(lat), Some(lon)) => {
            let lat_sign = if lat_ref.as_deref().unwrap_or("").contains('S') {
                -1.0
            } else {
                1.0
            };
            let lon_sign = if lon_ref.as_deref().unwrap_or("").contains('W') {
                -1.0
            } else {
                1.0
            };
            Some(format!("{:.6}, {:.6}", lat * lat_sign, lon * lon_sign))
        }
        _ => None,
    }
}

fn gps_triplet(value: &ExifValue) -> Option<f64> {
    match value {
        ExifValue::Rational(values) if values.len() >= 3 => {
            let d = if values[0].denom == 0 {
                return None;
            } else {
                values[0].num as f64 / values[0].denom as f64
            };
            let m = if values[1].denom == 0 {
                return None;
            } else {
                values[1].num as f64 / values[1].denom as f64
            };
            let s = if values[2].denom == 0 {
                return None;
            } else {
                values[2].num as f64 / values[2].denom as f64
            };
            Some(d + m / 60.0 + s / 3600.0)
        }
        _ => None,
    }
}

fn rational_value(value: &ExifValue) -> Option<f64> {
    match value {
        ExifValue::Rational(values) => values.first().map(|r| {
            if r.denom == 0 {
                0.0
            } else {
                r.num as f64 / r.denom as f64
            }
        }),
        ExifValue::SRational(values) => values.first().map(|r| {
            if r.denom == 0 {
                0.0
            } else {
                r.num as f64 / r.denom as f64
            }
        }),
        _ => None,
    }
}
