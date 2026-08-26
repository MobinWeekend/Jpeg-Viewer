// src/image_core.rs
// Core types and the decoder trait – no dependency on `image`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Bmp,
    Dds,
    Exr,
    Farbfeld,
    Gif,
    Hdr,
    Ico,
    Jpeg,
    Png,
    Pnm,
    Qoi,
    Tga,
    Tiff,
    Webp,
    Avif,
    Heic,
    JpegXl,
    Svg,
}

impl ImageFormat {
    pub const ALL: &[Self] = &[
        Self::Bmp,
        Self::Dds,
        Self::Exr,
        Self::Farbfeld,
        Self::Gif,
        Self::Hdr,
        Self::Ico,
        Self::Jpeg,
        Self::Png,
        Self::Pnm,
        Self::Qoi,
        Self::Tga,
        Self::Tiff,
        Self::Webp,
        Self::Avif,
        Self::Heic,
        Self::JpegXl,
        Self::Svg,
    ];

    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Bmp => &["bmp"],
            Self::Dds => &["dds"],
            Self::Exr => &["exr"],
            Self::Farbfeld => &["ff"],
            Self::Gif => &["gif"],
            Self::Hdr => &["hdr"],
            Self::Ico => &["ico"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Png => &["png"],
            Self::Pnm => &["pnm"],
            Self::Qoi => &["qoi"],
            Self::Tga => &["tga"],
            Self::Tiff => &["tif", "tiff"],
            Self::Webp => &["webp"],
            Self::Avif => &["avif"],
            Self::Heic => &["heic", "heif"],
            Self::JpegXl => &["jxl"],
            Self::Svg => &["svg"],
        }
    }

    /// The canonical extension used when renaming files.
    pub const fn preferred_extension(self) -> &'static str {
        self.extensions()[0]
    }

    pub fn matches_extension(self, extension: &str) -> bool {
        let extension = extension.trim_start_matches('.');

        self.extensions()
            .iter()
            .any(|ext| ext.eq_ignore_ascii_case(extension))
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        let extension = extension.trim_start_matches('.');

        Self::ALL
            .iter()
            .copied()
            .find(|format| format.matches_extension(extension))
    }

    pub fn all_extensions() -> Vec<String> {
        Self::ALL
            .iter()
            .flat_map(|format| format.extensions())
            .map(|ext| (*ext).to_string())
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA8 pixels
}

impl DecodedImage {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// Decoder options – currently empty, but can be extended later.
#[derive(Debug, Clone)]
pub struct DecodeOptions; // unit struct

impl Default for DecodeOptions {
    fn default() -> Self {
        Self // works for a unit struct
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("unknown image format")]
    UnknownFormat,
    #[allow(dead_code)]
    #[error("unsupported image format: {0:?}")]
    UnsupportedFormat(ImageFormat),
    #[error("no decoder available for format: {0:?}")]
    NoDecoder(ImageFormat),
    #[allow(dead_code)]
    #[error("invalid image data: {0}")]
    InvalidData(String),
    #[error("decoding failed: {0}")]
    Decode(String),
}

/// The decoder contract – works on raw bytes.
pub trait ImageDecoder: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    fn supported_formats(&self) -> &'static [ImageFormat];

    fn decode(&self, bytes: &[u8], options: &DecodeOptions) -> Result<DecodedImage, ImageError>;

    /// Fast dimension inspection. Default falls back to full decode.
    fn dimensions(&self, bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        let img = self.decode(bytes, &DecodeOptions::default())?;
        Ok((img.width(), img.height()))
    }
}
