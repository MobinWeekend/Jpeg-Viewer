use super::types::{LoadedImage, LoadingState, ViewerApp};
use crate::app::constants::MAX_TILE_SIZE;
use crate::gif::detection::is_gif_bytes;
use crate::image_entry::ImageEntry;

use rayon::spawn;

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::channel;

impl ViewerApp {
    // ====== FILE / DIRECTORY LOADING ======

    /// Open a specific image.
    ///
    /// Directory indexing happens in the background so the UI can appear
    /// immediately instead of blocking while thousands of files are scanned.
    pub fn load_image(&mut self, path: PathBuf) {
        let Some(parent) = path.parent() else {
            return;
        };

        let parent = parent.to_path_buf();

        self.start_directory_indexing(parent, Some(path));
    }

    /// Open a directory.
    ///
    /// The directory is indexed on a background Rayon worker.
    pub fn load_directory(&mut self, path: &PathBuf) {
        self.start_directory_indexing(path.clone(), None);
    }

    /// Start asynchronous directory indexing.
    ///
    /// `selected_path` is Some when the user opened a specific file.
    /// After indexing finishes, that file becomes the current image.
    fn start_directory_indexing(&mut self, directory: PathBuf, selected_path: Option<PathBuf>) {
        // Cancel/ignore any previous indexing result.
        self.indexing_receiver = None;

        self.current_directory = Some(directory.clone());

        // Clear the old image while indexing.
        self.clear_current_image_state();

        self.set_loading_state(LoadingState::Indexing);

        let (tx, rx) = channel();

        self.indexing_receiver = Some(rx);

        spawn(move || {
            let files = crate::loader::load_directory_images(&directory);

            // Ignore the result if the receiver was dropped.
            let _ = tx.send((files, selected_path));
        });
    }

    /// Process a background directory indexing result.
    ///
    /// This should be called every frame from your update loop.
    pub fn process_indexing(&mut self) {
        let Some(receiver) = &self.indexing_receiver else {
            return;
        };

        let Ok((files, selected_path)) = receiver.try_recv() else {
            return;
        };

        self.indexing_receiver = None;

        if files.is_empty() {
            println!("No images found in directory: {:?}", self.current_directory);

            self.image_entries.clear();
            self.current_index = 0;
            self.set_loading_state(LoadingState::Idle);
            return;
        }

        let current_index = match selected_path {
            Some(path) => files.iter().position(|p| p == &path).unwrap_or(0),
            None => 0,
        };

        let entries = files.into_iter().map(ImageEntry::File).collect();

        self.set_image_entries(entries, current_index);
    }

    /// Clear state belonging to the currently displayed image.
    ///
    /// This is used while directory indexing is happening so the old image
    /// does not remain active while the new directory is being scanned.
    fn clear_current_image_state(&mut self) {
        self.image_error = None;

        self.texture = None;
        self.gif_animation = None;

        self.is_gif = false;
        self.is_preview = false;

        self.current_image_path = None;

        self.receiver = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;

        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;

        self.file_type_detection = None;

        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;

        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
    }

    // ====== IMAGE LOADING ======

    /// Load the current image without using the cache.
    pub fn load_current_image(&mut self) {
        self.clear_current_image_state();

        let entry = match self.image_entries.get(self.current_index) {
            Some(entry) => entry.clone(),
            None => {
                self.set_loading_state(LoadingState::Idle);
                return;
            }
        };

        if let ImageEntry::File(path) = &entry {
            self.current_image_path = Some(path.clone());
        }

        let settings = self.settings_manager.get();

        let threshold = settings.virtual_texture_threshold;
        let tile_size = settings.tile_size;

        self.set_loading_state(LoadingState::Loading);
        self.b_fit_to_window = true;

        let (tx, rx) = channel();

        spawn(move || {
            let result = load_entry_content(entry, threshold, tile_size);
            let _ = tx.send(result);
        });

        self.receiver = Some(rx);
    }

    /// Load the current image from cache.
    ///
    /// If it isn't cached, the current texture is cleared and normal loading
    /// begins.
    pub fn load_current_image_with_cache(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }

        self.load_current_image();
    }

    /// Load the current image from cache while keeping the existing texture
    /// if it isn't cached.
    ///
    /// This is useful during navigation because it prevents the old image
    /// from disappearing while the next image loads.
    pub fn load_current_image_with_cache_keep_texture(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }

        self.load_current_image_keep_texture();
    }

    /// Load the current image without clearing the existing texture.
    pub fn load_current_image_keep_texture(&mut self) {
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

        spawn(move || {
            let result = load_entry_content(entry, threshold, tile_size);
            let _ = tx.send(result);
        });

        self.receiver = Some(rx);
    }

    // ====== IMAGE ENTRY SETUP ======

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

        // Reset preload state.
        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;

        self.preload_generation = self.preload_generation.wrapping_add(1);

        self.should_stop_caching = false;
        self.file_type_detection = None;

        // Detect file type for the newly selected image.
        self.detect_current_file_type();

        self.load_current_image();
    }
}

// ====== ENTRY LOADING ======

/// Load one image entry.
///
/// This function runs on a background worker and therefore must not access
/// ViewerApp or any UI state.
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

            let mut archive =
                zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {}", e))?;

            let mut entry = archive
                .by_index(zip.entry_index)
                .map_err(|e| format!("Failed to read entry: {}", e))?;

            let mut bytes = Vec::new();

            entry
                .read_to_end(&mut bytes)
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

            let bytes = reader
                .read_file(&s7z.name)
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

                    return load_bytes_with_detection(bytes, Some(path_hint), threshold, tile_size);
                }

                archive = header
                    .skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    }
}

// ====== BYTE DETECTION ======

/// Load bytes with automatic content detection.
///
/// Detection order:
///
/// 1. GIF preview
/// 2. GIF full animation fallback
/// 3. Virtual texture detection
/// 4. Normal static image
pub fn load_bytes_with_detection(
    bytes: Vec<u8>,
    path_hint: Option<&std::path::Path>,
    threshold: u32,
    _tile_size: u32,
) -> Result<LoadedImage, String> {
    let is_gif = is_gif_bytes(&bytes);

    if is_gif {
        if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes_preview(&bytes) {
            return Ok(LoadedImage::Animated(gif, true));
        }

        if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes(&bytes) {
            return Ok(LoadedImage::Animated(gif, false));
        }
    }

    let threshold = threshold.min(MAX_TILE_SIZE);

    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format();

    if let Ok(reader) = reader {
        if let Ok((width, height)) = reader.into_dimensions() {
            if threshold > 0 && (width >= threshold || height >= threshold) {
                return Ok(LoadedImage::VirtualPending(bytes, width, height));
            }
        }
    }

    crate::loader::load_image_from_bytes(&bytes, path_hint)
        .map(LoadedImage::Static)
        .map_err(|e| e.to_string())
}
