use super::types::{LoadedImage, LoadingState, ViewerApp};
use crate::decoder::decode_bytes;
use crate::decoder::default_registry;
use crate::decoder::format_detection::detect_format;
use crate::gif::detection::is_gif_bytes;
use crate::image_entry::ImageEntry;
use crate::loader::load_directory_images;
use rayon::spawn;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::channel;

impl ViewerApp {
    // ====== FILE / DIRECTORY LOADING ======

    /// Open a specific image.
    pub fn load_image(&mut self, path: PathBuf) {
        let Some(parent) = path.parent() else { return };
        self.start_directory_indexing(parent.to_path_buf(), Some(path));
    }

    /// Open a directory.
    pub fn load_directory(&mut self, path: &PathBuf) {
        self.start_directory_indexing(path.clone(), None);
    }

    /// Start asynchronous directory indexing.
    fn start_directory_indexing(&mut self, directory: PathBuf, selected_path: Option<PathBuf>) {
        self.indexing_receiver = None;
        self.current_directory = Some(directory.clone());
        self.clear_current_image_state();
        self.set_loading_state(LoadingState::Indexing);

        let (tx, rx) = channel();
        self.indexing_receiver = Some(rx);

        spawn(move || {
            let paths = load_directory_images(&directory);
            let _ = tx.send((paths, selected_path)); // sends Vec<PathBuf>
        });
    }

    /// Process a background directory indexing result.
    pub fn process_indexing(&mut self) {
        let Some(receiver) = &self.indexing_receiver else {
            return;
        };
        let Ok((paths, selected_path)) = receiver.try_recv() else {
            return;
        };
        self.indexing_receiver = None;

        if paths.is_empty() {
            println!("No images found in directory");
            self.image_entries.clear();
            self.current_index = 0;
            self.set_loading_state(LoadingState::Idle);
            return;
        }

        // Convert PathBufs to ImageEntry::File
        let entries: Vec<ImageEntry> = paths.into_iter().map(ImageEntry::File).collect();

        let current_index = match selected_path {
            Some(path) => entries
                .iter()
                .position(|entry| matches!(entry, ImageEntry::File(p) if p == &path))
                .unwrap_or(0),
            None => 0,
        };

        self.set_image_entries(entries, current_index);
    }

    /// Clear state belonging to the currently displayed image.
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

        let entry = match self.image_entries.get(self.current_index).cloned() {
            Some(e) => e,
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

        self.set_loading_state(LoadingState::Loading);
        self.b_fit_to_window = true;

        let (tx, rx) = channel();
        spawn(move || {
            let result = load_entry_content(entry, threshold);
            let _ = tx.send(result);
        });
        self.receiver = Some(rx);
    }

    pub fn load_current_image_with_cache(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }
        self.load_current_image();
    }

    pub fn load_current_image_with_cache_keep_texture(&mut self) {
        if self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
            return;
        }
        self.load_current_image_keep_texture();
    }

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

        let entry = match self.image_entries.get(self.current_index).cloned() {
            Some(e) => e,
            None => {
                self.set_loading_state(LoadingState::Idle);
                return;
            }
        };

        self.update_current_image_path();
        let settings = self.settings_manager.get();
        let threshold = settings.virtual_texture_threshold;

        self.set_loading_state(LoadingState::Loading);
        self.b_fit_to_window = true;

        let (tx, rx) = channel();

        spawn(move || {
            let result = load_entry_content(entry, threshold);
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

        self.image_cache.clear();
        self.preloading_indices.clear();
        self.preload_tasks.clear();
        self.preload_workers = 0;
        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.should_stop_caching = false;
        self.file_type_detection = None;

        self.detect_current_file_type();
        self.load_current_image();
    }

    /// Updates `current_image_path` based on the current index.
    /// - If the current entry is a standalone file, set path to that file.
    /// - Otherwise (archive entries), set to `None`.
    pub fn update_current_image_path(&mut self) {
        let entry = match self.image_entries.get(self.current_index).cloned() {
            Some(e) => e,
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
    }

    pub fn get_rename_suggestion(&mut self) -> Option<(String, &'static str)> {
        self.file_type_detection
            .as_ref()
            .filter(|detection| {
                detection.mismatch
                    && detection.index == self.current_index
                    && detection.generation == self.preload_generation
            })
            .map(|detection| {
                let current = detection
                    .current_extension
                    .as_deref()
                    .unwrap_or("(none)")
                    .to_owned();

                let suggested = detection.detected_format.preferred_extension();

                (current, suggested)
            })
    }
}

// ====== ENTRY LOADING ======

/// Load one entry using `decode_bytes` plus our own GIF/VT logic.
///
/// This function runs on a background worker and must not touch UI state.
fn load_entry_content(entry: ImageEntry, threshold: u32) -> Result<LoadedImage, String> {
    // Helper to read bytes from any entry.
    let bytes = match &entry {
        ImageEntry::File(path) => {
            std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?
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
            bytes
        }
        ImageEntry::S7z(s7z) => {
            let mut reader = sevenz_rust2::ArchiveReader::open(
                &s7z.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
            reader
                .read_file(&s7z.name)
                .map_err(|e| format!("Failed to read file from 7z: {}", e))?
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
                    break bytes;
                }
                archive = header
                    .skip()
                    .map_err(|e| format!("Failed to skip RAR entry: {}", e))?;
            }
        }
    };

    // ---- GIF detection ----
    if is_gif_bytes(&bytes) {
        // Try to load GIF animation (use your existing GIF loader)
        // For simplicity, we attempt to load from bytes directly.
        // If you have a function that returns GifAnimation from bytes, use it.
        if let Ok(gif) = crate::gif::animation::GifAnimation::from_bytes(&bytes) {
            return Ok(LoadedImage::Animated(gif, false));
        }
        // If that fails, fall through to static decode (will show first frame)
    }

    // ---- Virtual‑texture check ----
    // We need dimensions without full decode – use the registry.
    let registry = default_registry();
    let format = detect_format(&bytes).ok_or("Unknown image format")?;
    let (width, height) = registry
        .dimensions(&bytes, format)
        .map_err(|e| e.to_string())?;

    let use_virtual = threshold > 0 && (width >= threshold || height >= threshold);
    if use_virtual {
        return Ok(LoadedImage::VirtualPending(bytes, width, height));
    }

    // ---- Normal static decode ----
    let decoded = decode_bytes(&bytes).map_err(|e| e.to_string())?;
    Ok(LoadedImage::Static(decoded))
}
