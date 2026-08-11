use super::types::{LoadedImage, ViewerApp};
use crate::image_entry::ImageEntry;
use rayon::spawn;
use std::path::PathBuf;
use std::sync::mpsc::channel;

impl ViewerApp {
    pub fn load_image(&mut self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            self.current_directory = Some(parent.to_path_buf());

            let files = crate::loader::load_directory_images(parent);

            if let Some(index) = files.iter().position(|p| p == &path) {
                let entries = files.into_iter().map(ImageEntry::File).collect();
                self.set_image_entries(entries, index);
            }
        }
    }

    /// Original load function - clears texture (used for initial load and directory changes)
    pub fn load_current_image(&mut self) {
        // Clear any previous state before loading
        self.image_error = None;
        self.texture = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.b_is_loading_full = false;
        self.receiver = None;

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => {
                self.b_is_loading = false;
                return;
            }
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        } else {
            self.current_image_path = None;
        }

        self.b_is_loading = true;
        self.b_fit_to_window = true;
        let (tx, rx) = channel();

        // Load using content detection - no extension-based branching
        let entry_clone = entry.clone();
        spawn(move || {
            let result = load_entry_content(entry_clone);
            let _ = tx.send(result);
        });

        self.receiver = Some(rx);
    }

    /// Load from cache and clear texture if not found (used for initial load)
    pub fn load_current_image_with_cache(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.b_is_loading = false;
            self.image_error = None;
            return;
        }
        self.load_current_image();
    }

    /// Load from cache but keep existing texture if not found (used for navigation)
    pub fn load_current_image_with_cache_keep_texture(&mut self) {
        // Check cache first - if cached, swap textures immediately
        if self.load_from_cache(self.current_index) {
            self.b_is_loading = false;
            self.image_error = None;
            return;
        }

        // Not in cache - load in background but keep current texture
        self.load_current_image_keep_texture();
    }

    /// Load image without clearing current texture (for smooth navigation)
    pub fn load_current_image_keep_texture(&mut self) {
        // Clear any previous loading state but KEEP the texture
        self.image_error = None;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.b_is_loading_full = false;
        self.receiver = None;

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => {
                self.b_is_loading = false;
                return;
            }
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        } else {
            self.current_image_path = None;
        }

        self.b_is_loading = true;
        self.b_fit_to_window = true;
        let (tx, rx) = channel();

        // Load using content detection - no extension-based branching
        let entry_clone = entry.clone();
        spawn(move || {
            let result = load_entry_content(entry_clone);
            let _ = tx.send(result);
        });

        self.receiver = Some(rx);
    }

    pub fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.image_entries = entries;
        self.current_index = current_index;
        self.preload_origin = current_index;
        self.delta_threshold =
            ((self.cache_radius as f32 * self.cache_delta_factor).round() as usize).max(1);
        self.b_fit_to_window = true;
        self.gif_animation = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.image_error = None;
        self.receiver = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;

        // Reset preload state – discard all pending tasks and invalidate results.
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
        self.file_type_detection = None; // Clear file extension detection

        // Detect file type early (so we have it even if loading fails)
        self.detect_current_file_type();

        self.load_current_image();
    }

    pub fn load_directory(&mut self, path: &PathBuf) {
        self.current_directory = Some(path.clone());
        let files = crate::loader::load_directory_images(path);
        if files.is_empty() {
            println!("No images found in directory: {:?}", path);
            return;
        }
        let entries = files.into_iter().map(ImageEntry::File).collect();

        self.set_image_entries(entries, 0);
    }
}

/// Load entry content with automatic content detection
/// This function detects the actual content type by magic bytes, not by extension
fn load_entry_content(entry: ImageEntry) -> Result<LoadedImage, String> {
    match entry {
        ImageEntry::File(path) => {
            // Read the file bytes
            let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;

            // Detect by content using magic bytes
            load_bytes_with_detection(bytes, Some(&path))
        }
        ImageEntry::Zip(zip) => {
            // Read from zip archive
            let file = std::fs::File::open(&zip.archive_path)
                .map_err(|e| format!("Failed to open archive: {}", e))?;
            let mut archive =
                zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {}", e))?;
            let mut entry = archive
                .by_index(zip.entry_index)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)
                .map_err(|e| format!("Failed to read data: {}", e))?;

            let path_hint = std::path::Path::new(&zip.name);
            load_bytes_with_detection(bytes, Some(path_hint))
        }
        ImageEntry::S7z(s7z) => {
            // Read from 7z archive
            let mut reader = sevenz_rust2::ArchiveReader::open(
                &s7z.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
            let bytes = reader
                .read_file(&s7z.name)
                .map_err(|e| format!("Failed to read file from 7z: {}", e))?;

            let path_hint = std::path::Path::new(&s7z.name);
            load_bytes_with_detection(bytes, Some(path_hint))
        }
        ImageEntry::Rar(rar) => {
            // Read from RAR archive
            let archive = unrar::Archive::new(&rar.archive_path)
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
                    let path_hint = std::path::Path::new(&rar.name);
                    return load_bytes_with_detection(bytes, Some(path_hint));
                }
                archive = header
                    .skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    }
}

/// Load bytes with automatic content detection
/// First tries GIF (by magic bytes), then falls back to static image loading
fn load_bytes_with_detection(
    bytes: Vec<u8>,
    path_hint: Option<&std::path::Path>,
) -> Result<LoadedImage, String> {
    // Check if it's a GIF by magic number (GIF87a or GIF89a)
    let is_gif = bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a");

    if is_gif {
        // Try to load as GIF preview first
        if let Ok(gif) = crate::gif_animation::GifAnimation::from_bytes_preview(&bytes) {
            return Ok(LoadedImage::Animated(gif, true));
        }
        // If GIF loading fails, try as static (maybe corrupted GIF that's actually another format)
        // Fall through to static loading
    }

    // Try to load as static image
    match crate::loader::load_image_from_bytes(&bytes, path_hint) {
        Ok(img) => Ok(LoadedImage::Static(img)),
        Err(e) => {
            // If we already tried GIF and it worked, use that
            // If we got here, either it wasn't a GIF or GIF loading failed
            if is_gif {
                // Try one more time with full GIF loading
                if let Ok(gif) = crate::gif_animation::GifAnimation::from_bytes(&bytes) {
                    return Ok(LoadedImage::Animated(gif, false));
                }
            }
            Err(e)
        }
    }
}

// Load entry content as full GIF (all frames) - used for upgrading from preview
/// Load entry content as full GIF (all frames) - used for upgrading from preview
pub fn load_entry_content_full_gif(entry: ImageEntry) -> Result<LoadedImage, String> {
    println!("[FULL GIF] Starting load_entry_content_full_gif");
    let result = match entry {
        ImageEntry::File(path) => {
            println!("[FULL GIF] File variant, path: {:?}", path);
            let bytes = std::fs::read(&path)
                .map_err(|e| {
                    eprintln!("[FULL GIF] Read error: {}", e);
                    format!("Failed to read file: {}", e)
                })?;
            println!("[FULL GIF] Read {} bytes from file", bytes.len());
            let gif = crate::gif_animation::GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames from file", gif.frame_count());
            Ok(LoadedImage::Animated(gif, false))
        }
        ImageEntry::Zip(zip) => {
            println!("[FULL GIF] ZIP variant, name: {}", zip.name);
            let file = std::fs::File::open(&zip.archive_path)
                .map_err(|e| format!("Failed to open archive: {}", e))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Failed to read archive: {}", e))?;
            let mut entry = archive.by_index(zip.entry_index)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)
                .map_err(|e| format!("Failed to read data: {}", e))?;
            println!("[FULL GIF] Read {} bytes from ZIP", bytes.len());
            let gif = crate::gif_animation::GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames from ZIP", gif.frame_count());
            Ok(LoadedImage::Animated(gif, false))
        }
        ImageEntry::S7z(s7z) => {
            println!("[FULL GIF] 7z variant, name: {}", s7z.name);
            let mut reader = sevenz_rust2::ArchiveReader::open(&s7z.archive_path, sevenz_rust2::Password::empty())
                .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
            let bytes = reader.read_file(&s7z.name)
                .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
            println!("[FULL GIF] Read {} bytes from 7z", bytes.len());
            let gif = crate::gif_animation::GifAnimation::from_bytes(&bytes)?;
            println!("[FULL GIF] Decoded {} frames from 7z", gif.frame_count());
            Ok(LoadedImage::Animated(gif, false))
        }
        ImageEntry::Rar(rar) => {
            println!("[FULL GIF] RAR variant, name: {}", rar.name);
            let archive = unrar::Archive::new(&rar.archive_path)
                .open_for_processing()
                .map_err(|e| format!("Failed to open RAR archive: {}", e))?;
            let mut archive = archive;
            loop {
                let header = archive.read_header()
                    .map_err(|e| format!("Failed to read RAR header: {}", e))?
                    .ok_or_else(|| format!("File not found in archive: {}", rar.name))?;
                let filename = header.entry().filename.to_string_lossy().to_string();
                if filename == rar.name {
                    let (bytes, _) = header.read()
                        .map_err(|e| format!("Failed to read file from RAR: {}", e))?;
                    println!("[FULL GIF] Read {} bytes from RAR", bytes.len());
                    let gif = crate::gif_animation::GifAnimation::from_bytes(&bytes)?;
                    println!("[FULL GIF] Decoded {} frames from RAR", gif.frame_count());
                    return Ok(LoadedImage::Animated(gif, false));
                }
                archive = header.skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    };
    println!("[FULL GIF] load_entry_content_full_gif returning result");
    result
}
