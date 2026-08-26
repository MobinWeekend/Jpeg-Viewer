// src/image_core.rs
// Core types and the decoder trait – no dependency on `image`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Avif,
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
