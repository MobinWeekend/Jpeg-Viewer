use super::types::{LoadedImage, LoadingState, ViewerApp};
use crate::app::constants::MAX_TILE_SIZE;
use crate::image_entry::ImageEntry;
use crate::gif::detection::is_gif_bytes;
use rayon::spawn;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::io::Read;

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
        self.receiver = None;
        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => {
                self.set_loading_state(LoadingState::Idle);
                return;
            }
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        } else {
            self.current_image_path = None;
        }

        let settings = self.settings_manager.get();
        let threshold = settings.virtual_texture_threshold;
        let tile_size = settings.tile_size;

        self.set_loading_state(LoadingState::Loading);
        self.b_fit_to_window = true;
        let (tx, rx) = channel();

        let entry_clone = entry.clone();
        spawn(move || {
            let result = load_entry_content(entry_clone, threshold, tile_size);
            let _ = tx.send(result);
        });

        self.receiver = Some(rx);
    }

    /// Load from cache and clear texture if not found (used for initial load)
    pub fn load_current_image_with_cache(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }
        self.load_current_image();
    }

    /// Load from cache but keep existing texture if not found (used for navigation)
    pub fn load_current_image_with_cache_keep_texture(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }
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
        self.receiver = None;
        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => {
                self.set_loading_state(LoadingState::Idle);
                return;
            }
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        } else {
            self.current_image_path = None;
        }

        let settings = self.settings_manager.get();
        let threshold = settings.virtual_texture_threshold;
        let tile_size = settings.tile_size;

        self.set_loading_state(LoadingState::Loading);
        self.b_fit_to_window = true;
        let (tx, rx) = channel();

        let entry_clone = entry.clone();
        spawn(move || {
            let result = load_entry_content(entry_clone, threshold, tile_size);
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
        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;

        // Reset preload state – discard all pending tasks and invalidate results.
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
        self.file_type_detection = None;

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


/// Load entry content with automatic content detection, passing threshold and tile_size.
/// Load entry content with automatic content detection.
fn load_entry_content(
    entry: ImageEntry,
    threshold: u32,
    tile_size: u32,
) -> Result<LoadedImage, String> {
    match entry {
        ImageEntry::File(path) => {
            let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
            load_bytes_with_detection(bytes, Some(&path), threshold, tile_size)
        }
        ImageEntry::Zip(zip) => {
            let file = std::fs::File::open(&zip.archive_path)
                .map_err(|e| format!("Failed to open archive: {}", e))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Failed to read archive: {}", e))?;
            let mut entry = archive.by_index(zip.entry_index)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to read data: {}", e))?;
            let path_hint = std::path::Path::new(&zip.name);
            load_bytes_with_detection(bytes, Some(path_hint), threshold, tile_size)
        }
        ImageEntry::S7z(s7z) => {
            let mut reader = sevenz_rust2::ArchiveReader::open(
                &s7z.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
            let bytes = reader.read_file(&s7z.name)
                .map_err(|e| format!("Failed to read file from 7z: {}", e))?;
            let path_hint = std::path::Path::new(&s7z.name);
            load_bytes_with_detection(bytes, Some(path_hint), threshold, tile_size)
        }
        ImageEntry::Rar(rar) => {
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
                    let path_hint = std::path::Path::new(&rar.name);
                    return load_bytes_with_detection(bytes, Some(path_hint), threshold, tile_size);
                }
                archive = header.skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    }
}

/// Load bytes with automatic content detection.
/// First tries GIF, then checks dimensions to decide virtual vs normal.

fn load_bytes_with_detection(
    bytes: Vec<u8>,
    path_hint: Option<&std::path::Path>,
    threshold: u32,
    _tile_size: u32,
) -> Result<LoadedImage, String> {
    let is_gif = is_gif_bytes(&bytes);

    if is_gif {
        // Try preview first
        if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes_preview(&bytes) {
            return Ok(LoadedImage::Animated(gif, true));
        }
        // Fallback to full GIF (rare)
        if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes(&bytes) {
            return Ok(LoadedImage::Animated(gif, false));
        }
        // If both fail, continue as static (may still be a corrupt GIF)
    }

    // For non-GIF images, check dimensions
    let reader_result = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format();
    if let Ok(reader) = reader_result {
        if let Ok((width, height)) = reader.into_dimensions() {
            let use_virtual = width > threshold
                || height > threshold
                || width > MAX_TILE_SIZE
                || height > MAX_TILE_SIZE;
            if use_virtual {
                return Ok(LoadedImage::VirtualPending(bytes, width, height));
            }
        }
    }

    match crate::loader::load_image_from_bytes(&bytes, path_hint) {
        Ok(img) => Ok(LoadedImage::Static(img)),
        Err(e) => Err(e),
    }
}

