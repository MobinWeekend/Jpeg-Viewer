use crate::image_entry::ImageEntry;
use zip::ZipArchive;
use std::fs::File;
use std::io::Read;

/// Check if the first 6 bytes match GIF magic numbers.
pub fn is_gif_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a")
}

/// Determine if an entry is a GIF by inspecting the content or extension.
pub fn is_gif_entry(entry: &ImageEntry) -> bool {
    match entry {
        ImageEntry::File(path) => {
            // Read first few bytes to check magic number
            if let Ok(mut file) = File::open(path) {
                let mut header = [0u8; 6];
                if file.read_exact(&mut header).is_ok() {
                    return is_gif_bytes(&header);
                }
            }
            false
        }
        ImageEntry::Zip(zip) => {
            // For zip, try to read the header from the archive
            if let Ok(file) = File::open(&zip.archive_path) {
                if let Ok(mut archive) = ZipArchive::new(file) {
                    if let Ok(mut entry) = archive.by_index(zip.entry_index) {
                        let mut header = [0u8; 6];
                        if entry.read_exact(&mut header).is_ok() {
                            return is_gif_bytes(&header);
                        }
                    }
                }
            }
            false
        }
        ImageEntry::S7z(s7z) => {
            // For 7z, checking content is expensive; fall back to extension hint
            s7z.name.to_lowercase().ends_with(".gif")
        }
        ImageEntry::Rar(rar) => {
            rar.name.to_lowercase().ends_with(".gif")
        }
    }
}