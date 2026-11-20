use crate::decoder::gif::animation::GifAnimation;
use crate::imaging::entry::ImageEntry;
use sevenz_rust2::{ArchiveReader, Password};
use std::fs::File;
use std::io::Read;
//use std::path::PathBuf;
use unrar::Archive as RarArchive;
use zip::ZipArchive;

// ---------- Preview loaders ----------
/*
pub fn load_gif_preview_from_path(path: PathBuf) -> Option<GifAnimation> {
    std::fs::read(&path)
        .ok()
        .and_then(|data| GifAnimation::from_bytes_preview(&data).ok())
}
 */

// ---------- Full GIF loader (from ImageEntry) ----------
// This was load_entry_content_full_gif in loading.rs

pub fn load_full_gif_from_entry(
    entry: ImageEntry,
) -> Result<super::animation::GifAnimation, String> {
    println!("[FULL GIF] Starting load_full_gif_from_entry");
    let result = match entry {
        ImageEntry::File(path) => {
            println!("[FULL GIF] File variant, path: {:?}", path);
            let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
            crate::io_metrics::source_read(bytes.len());
            println!("[FULL GIF] Read {} bytes from file", bytes.len());
            let gif = GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames", gif.frame_count());
            Ok(gif)
        }
        ImageEntry::Zip(zip) => {
            println!("[FULL GIF] ZIP variant, name: {}", zip.name);
            let file = File::open(&zip.archive_path)
                .map_err(|e| format!("Failed to open archive: {}", e))?;
            let mut archive =
                ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {}", e))?;
            let mut entry = archive
                .by_index(zip.entry_index)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to read data: {}", e))?;
            crate::io_metrics::archive_extraction(bytes.len());
            println!("[FULL GIF] Read {} bytes from ZIP", bytes.len());
            let gif = GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames from ZIP", gif.frame_count());
            Ok(gif)
        }
        ImageEntry::S7z(s7z) => {
            println!("[FULL GIF] 7z variant, name: {}", s7z.name);
            let mut reader = ArchiveReader::open(&s7z.archive_path, Password::empty())
                .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
            let bytes = reader
                .read_file(&s7z.name)
                .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
            crate::io_metrics::archive_extraction(bytes.len());
            println!("[FULL GIF] Read {} bytes from 7z", bytes.len());
            let gif = GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames from 7z", gif.frame_count());
            Ok(gif)
        }
        ImageEntry::Rar(rar) => {
            println!("[FULL GIF] RAR variant, name: {}", rar.name);
            let archive = RarArchive::new(&rar.archive_path)
                .open_for_processing()
                .map_err(|e| format!("Failed to open RAR archive: {}", e))?;
            let mut archive = archive;
            loop {
                let header = archive
                    .read_header()
                    .map_err(|e| format!("Failed to read RAR header: {}", e))?
                    .ok_or_else(|| format!("File not found in archive: {}", rar.name))?;
                let filename = header.entry().filename.to_string_lossy().to_string();
                if filename == rar.name {
                    let (bytes, _) = header
                        .read()
                        .map_err(|e| format!("Failed to read file from RAR: {}", e))?;
                    crate::io_metrics::archive_extraction(bytes.len());
                    println!("[FULL GIF] Read {} bytes from RAR", bytes.len());
                    let gif = GifAnimation::from_bytes(&bytes)?;
                    println!("[FULL GIF] Decoded {} frames from RAR", gif.frame_count());
                    return Ok(gif);
                }
                archive = header
                    .skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    };
    println!("[FULL GIF] load_full_gif_from_entry returning result");
    result
}
