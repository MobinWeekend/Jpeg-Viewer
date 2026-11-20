use super::types::{FileTypeDetection, LoadedEntryData, LoadedImage, LoadingState, ViewerApp};
use crate::imaging::core::DecodeOptions;
use crate::imaging::entry::ImageEntry;
use crate::imaging::loader::load_directory_images;
use crate::imaging::sort::SortMethod;
use rayon::spawn;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;

impl ViewerApp {
    // ====== FILE / DIRECTORY LOADING ======

    /// Open a specific image.
    pub fn load_image(&mut self, path: PathBuf, sort_method: SortMethod) {
        let Some(parent) = path.parent() else { return };
        self.start_directory_indexing(parent.to_path_buf(), Some(path), sort_method);
    }

    /// Open a directory.
    pub fn load_directory(&mut self, path: &PathBuf, sort_method: SortMethod) {
        self.start_directory_indexing(path.clone(), None, sort_method);
    }

    /// Start asynchronous directory indexing.
    fn start_directory_indexing(
        &mut self,
        directory: PathBuf,
        selected_path: Option<PathBuf>,
        sort_method: SortMethod,
    ) {
        self.indexing_receiver = None;
        self.current_directory = Some(directory.clone());
        self.record_welcome_recent_path(directory.clone());
        self.clear_current_image_state();

        // When a specific image was opened, begin loading it immediately instead
        // of making the user wait for the entire containing folder to be indexed.
        // The one-entry list is temporary; process_indexing replaces it once the
        // background scan finishes, without reloading the displayed image.
        if let Some(path) = selected_path.as_ref() {
            self.image_entries = vec![ImageEntry::File(path.clone())];
            self.current_index = 0;
            self.preload_origin = 0;
            self.current_image_path = Some(path.clone());
            self.load_current_image();
        } else {
            // Do not leave the previous folder's entries behind while a new
            // directory is being indexed. In particular, an empty directory
            // must be able to reach the empty-folder fallback instead of
            // inheriting stale entries and remaining in the Indexing state.
            self.image_entries.clear();
            self.current_index = 0;
            self.preload_origin = 0;
            if let Some(tab) = self.workspace.active_tab_mut() {
                tab.entries.clear();
                tab.current_index = 0;
                tab.directory = Some(directory.clone());
            }
            self.set_loading_state(LoadingState::Indexing);
        }

        let (tx, rx) = channel();
        self.indexing_receiver = Some(rx);

        spawn(move || {
            let paths = load_directory_images(&directory, sort_method);
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
            // An explicitly selected image is already loading independently of
            // the containing-folder scan. Preserve it if the scan has no other
            // supported entries; a direct folder open instead becomes a clear,
            // non-loading empty state with an actionable error message.
            if selected_path.is_none() {
                self.current_index = 0;
                self.set_loading_state(LoadingState::Idle);
                self.image_error = Some(format!(
                    "This folder has no supported image formats:\n{}",
                    self.current_directory
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "Unknown folder".to_owned()),
                ));
                if let Some(tab) = self.workspace.active_tab_mut() {
                    tab.entries.clear();
                    tab.current_index = 0;
                }
            }
            return;
        }

        // Convert PathBufs to ImageEntry::File.
        let entries: Vec<ImageEntry> = paths.into_iter().map(ImageEntry::File).collect();

        let current_index = match selected_path.as_ref() {
            Some(path) => entries
                .iter()
                .position(|entry| matches!(entry, ImageEntry::File(p) if p == path))
                .unwrap_or(0),
            None => 0,
        };

        if selected_path.is_some() {
            // The requested image has already started loading (and may already
            // be visible). Update the navigation index in place so finishing the
            // directory scan doesn't clear its texture or trigger a second decode.
            self.image_entries = entries;
            self.current_index = current_index;
            self.preload_origin = current_index;
            self.delta_threshold =
                ((self.cache_radius as f32 * self.cache_delta_factor).round() as usize).max(1);

            // Any cache created while the temporary one-entry list was active
            // used temporary indices, so discard it before enabling preloading.
            self.image_cache.clear();
            self.preloading_indices.clear();
            self.cancel_all_preload_tasks();

            self.preload_generation = self.preload_generation.wrapping_add(1);
            self.preloading_indices.clear();
            self.should_stop_caching = false;
            self.detect_current_file_type();
        } else {
            self.set_image_entries(entries, current_index);
        }
    }

    /// Clear state belonging to the currently displayed image.
    fn clear_current_image_state(&mut self) {
        self.clear_current_image_state_with_cache(true);
    }

    fn clear_current_image_state_with_cache(&mut self, clear_cache: bool) {
        self.image_error = None;
        self.texture = None;
        self.texture_image_id = None;
        self.current_mipmap_state = None;
        self.gif_animation = None;
        self.full_gif_source = None;
        self.is_gif = false;
        self.is_preview = false;
        self.current_image_path = None;
        self.file_metadata = None;
        self.current_decoded_pixels = None;
        self.receiver = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;
        self.file_type_detection = None;
        self.file_metadata_open = false;
        if clear_cache {
            self.image_cache.clear();
        }
        self.preloading_indices.clear();
        self.cancel_all_preload_tasks();

        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.preloading_indices.clear();
        self.should_stop_caching = false;
    }

    // ====== IMAGE LOADING ======

    /// Load the current image without using the cache, preferring fast embedded RAW previews.
    pub fn load_current_image(&mut self) {
        self.load_current_image_mode(false);
    }

    /// Develop the actual RAW sensor data for the currently selected image.
    pub fn load_current_image_as_raw(&mut self) {
        self.load_current_image_mode(true);
    }

    fn load_current_image_mode(&mut self, develop_raw: bool) {
        self.load_current_image_mode_with_cache_policy(develop_raw, true);
    }

    /// Reload the current entry while preserving textures for other entries in
    /// the active Viewer collection. Used when returning from Masonry/Compare.
    pub(crate) fn load_current_image_preserving_cache(&mut self) {
        self.load_current_image_mode_with_cache_policy(false, false);
    }

    fn load_current_image_mode_with_cache_policy(&mut self, develop_raw: bool, clear_cache: bool) {
        self.window_fit_pending = self.settings_manager.get().fit_window_to_image;
        self.showing_raw_developed = develop_raw;
        self.clear_current_image_state_with_cache(clear_cache);

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
        let index = self.current_index;
        let generation = self.preload_generation;
        spawn(move || {
            let result =
                load_entry_content_with_metadata(entry, threshold, develop_raw, index, generation);
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
        self.file_metadata = None;
        self.current_decoded_pixels = None;
        self.gif_animation = None;
        self.full_gif_source = None;
        self.is_gif = false;
        self.is_preview = false;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.receiver = None;
        // The previous base texture remains visible as a loading placeholder,
        // but its mipmap worker belongs to the outgoing entry. Drop that state
        // so it cannot be promoted into the incoming entry's cache slot.
        self.current_mipmap_state = None;
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
        let index = self.current_index;
        let generation = self.preload_generation;

        spawn(move || {
            let result =
                load_entry_content_with_metadata(entry, threshold, false, index, generation);
            let _ = tx.send(result);
        });
        self.receiver = Some(rx);
    }

    // ====== IMAGE ENTRY SETUP ======

    pub fn set_image_entries(&mut self, entries: Vec<ImageEntry>, current_index: usize) {
        self.set_image_entries_internal(entries, current_index, false);
    }

    /// Switch the shared Viewer renderer to another workspace snapshot without
    /// discarding textures cached by stable image identity. This is used for
    /// workspace navigation; explicit collection replacement still clears the
    /// cache through `set_image_entries`.
    pub(crate) fn set_image_entries_preserving_cache(
        &mut self,
        entries: Vec<ImageEntry>,
        current_index: usize,
    ) {
        self.set_image_entries_internal(entries, current_index, true);
    }

    fn set_image_entries_internal(
        &mut self,
        entries: Vec<ImageEntry>,
        current_index: usize,
        preserve_cache: bool,
    ) {
        self.window_fit_pending = self.settings_manager.get().fit_window_to_image;
        self.image_entries = entries;
        self.current_index = current_index.min(self.image_entries.len().saturating_sub(1));
        self.preload_origin = self.current_index;
        self.delta_threshold =
            ((self.cache_radius as f32 * self.cache_delta_factor).round() as usize).max(1);
        self.b_fit_to_window = true;
        self.gif_animation = None;
        self.full_gif_source = None;
        self.is_gif = false;
        self.is_preview = false;
        self.texture = None;
        self.texture_image_id = None;
        self.current_mipmap_state = None;
        self.image_error = None;
        self.receiver = None;
        self.full_image_receiver = None;
        self.full_gif_receiver = None;
        self.virtual_texture = None;
        self.vt_progress = None;
        self.vt_total_tiles = 0;
        self.virtual_texture_thread = None;

        if !preserve_cache {
            self.image_cache.clear();
        }
        self.preloading_indices.clear();
        self.cancel_all_preload_tasks();

        self.preload_generation = self.preload_generation.wrapping_add(1);
        self.preloading_indices.clear();
        self.should_stop_caching = false;
        self.file_type_detection = None;
        self.file_metadata = None;
        self.current_decoded_pixels = None;
        self.preloaded_mipmap_pixels = None;
        self.file_metadata_open = false;
        if let Ok(mut cache) = self.gif_detection_cache.lock() {
            cache.clear();
        }

        self.update_current_image_path();
        self.detect_current_file_type();
        if preserve_cache && self.load_from_cache(self.current_index) {
            self.set_loading_state(LoadingState::Idle);
            self.image_error = None;
        } else if preserve_cache {
            self.load_current_image_preserving_cache();
        } else {
            self.load_current_image();
        }
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
            self.record_welcome_recent_path(path.clone());
        } else {
            self.current_image_path = None;
        }
    }

    pub fn get_rename_suggestion(&mut self) -> Option<(String, &'static str)> {
        self.file_type_detection
            .as_ref()
            .filter(|detection| {
                detection.mismatch
                //&& detection.index == self.current_index
                //&& detection.generation == self.preload_generation
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

/// Read one entry's source bytes. Files and archive members share this helper
/// so format detection, metadata extraction, and decoding can consume the same
/// allocation instead of independently reopening/re-extracting the source.
fn read_all_cancellable<R: Read>(
    reader: &mut R,
    cancellation: Option<&AtomicBool>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
            return Err("Preload cancelled".to_owned());
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read image data: {error}"))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}

fn read_entry_bytes_cancellable(
    entry: &ImageEntry,
    cancellation: Option<&AtomicBool>,
) -> Result<Vec<u8>, String> {
    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err("Preload cancelled".to_owned());
    }

    let bytes = match entry {
        ImageEntry::File(path) => {
            if let Some(token) = cancellation {
                let mut file = std::fs::File::open(path)
                    .map_err(|error| format!("Failed to open file: {error}"))?;
                read_all_cancellable(&mut file, Some(token))?
            } else {
                std::fs::read(path).map_err(|error| format!("Failed to read file: {error}"))?
            }
        }
        ImageEntry::Zip(zip) => {
            let file = std::fs::File::open(&zip.archive_path)
                .map_err(|error| format!("Failed to open archive: {error}"))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|error| format!("Failed to read archive: {error}"))?;
            let mut member = archive
                .by_index(zip.entry_index)
                .map_err(|error| format!("Failed to read entry: {error}"))?;
            read_all_cancellable(&mut member, cancellation)?
        }
        ImageEntry::S7z(s7z) => {
            // The 7z crate exposes whole-member extraction, so cancellation can
            // only be checked immediately before and after this operation.
            let mut reader = sevenz_rust2::ArchiveReader::open(
                &s7z.archive_path,
                sevenz_rust2::Password::empty(),
            )
            .map_err(|error| format!("Failed to open 7z archive: {error}"))?;
            let bytes = reader
                .read_file(&s7z.name)
                .map_err(|error| format!("Failed to read file from 7z: {error}"))?;
            if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                return Err("Preload cancelled".to_owned());
            }
            bytes
        }
        ImageEntry::Rar(rar) => {
            let mut archive = unrar::Archive::new(&rar.archive_path)
                .open_for_processing()
                .map_err(|error| format!("Failed to open RAR archive: {error}"))?;
            loop {
                if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                    return Err("Preload cancelled".to_owned());
                }
                let header = archive
                    .read_header()
                    .map_err(|error| format!("Failed to read RAR header: {error}"))?
                    .ok_or_else(|| format!("File not found in archive: {}", rar.name))?;
                let filename = header.entry().filename.to_string_lossy().to_string();
                if filename == rar.name {
                    let (bytes, _) = header
                        .read()
                        .map_err(|error| format!("Failed to read file from RAR: {error}"))?;
                    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                        return Err("Preload cancelled".to_owned());
                    }
                    break bytes;
                }
                archive = header
                    .skip()
                    .map_err(|error| format!("Failed to skip RAR entry: {error}"))?;
            }
        }
    };

    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err("Preload cancelled".to_owned());
    }
    match entry {
        ImageEntry::File(_) => crate::io_metrics::source_read(bytes.len()),
        ImageEntry::Zip(_) | ImageEntry::S7z(_) | ImageEntry::Rar(_) => {
            crate::io_metrics::archive_extraction(bytes.len());
        }
    }
    Ok(bytes)
}

fn entry_path_hint(entry: &ImageEntry) -> Option<&std::path::Path> {
    match entry {
        ImageEntry::File(path) => Some(path.as_path()),
        ImageEntry::Zip(item) => Some(std::path::Path::new(&item.name)),
        ImageEntry::S7z(item) => Some(std::path::Path::new(&item.name)),
        ImageEntry::Rar(item) => Some(std::path::Path::new(&item.name)),
    }
}

fn detect_entry_file_type(
    entry: &ImageEntry,
    bytes: &[u8],
    index: usize,
    generation: u64,
) -> Option<FileTypeDetection> {
    let ImageEntry::File(path) = entry else {
        return None;
    };
    let detected_format = crate::decoder::format_detection::detect_format(bytes)?;
    let current_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    let mismatch = current_extension
        .as_deref()
        .map(|extension| !detected_format.matches_extension(extension))
        .unwrap_or(true);
    Some(FileTypeDetection {
        detected_format,
        current_extension,
        mismatch,
        index,
        generation,
    })
}

/// Full current-image load. Metadata and mismatch detection are computed from
/// the exact same bytes that are handed to the decoder.
pub(crate) fn load_entry_content_with_metadata(
    entry: ImageEntry,
    threshold: u32,
    develop_raw: bool,
    index: usize,
    generation: u64,
) -> Result<LoadedEntryData, String> {
    load_entry_content_with_metadata_cancellable(
        entry,
        threshold,
        develop_raw,
        index,
        generation,
        None,
    )
}

/// Preload variant with cooperative cancellation checkpoints. File and ZIP
/// members are read in bounded chunks; archive APIs that only expose whole-file
/// extraction are checked immediately before and after extraction.
pub(crate) fn load_entry_content_with_metadata_cancellable(
    entry: ImageEntry,
    threshold: u32,
    develop_raw: bool,
    index: usize,
    generation: u64,
    cancellation: Option<&AtomicBool>,
) -> Result<LoadedEntryData, String> {
    let bytes = read_entry_bytes_cancellable(&entry, cancellation)?;
    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err("Preload cancelled".to_owned());
    }
    let mut metadata = ViewerApp::build_file_metadata_from_bytes(&entry, Some(&bytes));
    let file_type_detection = detect_entry_file_type(&entry, &bytes, index, generation);
    if let Some(detection) = &file_type_detection {
        metadata.mismatch = detection.mismatch;
        metadata.current_extension = detection.current_extension.clone();
        if metadata.format.is_none() {
            metadata.format = Some(detection.detected_format);
        }
    }
    let (image, retained_gif_source) =
        decode_entry_bytes_with_source(&entry, bytes, threshold, develop_raw)?;
    if let LoadedImage::Animated(gif, is_preview) = &image {
        // Preview GIFs are upgraded asynchronously. Their frame count is
        // populated after the full pass rather than decoding every frame here
        // solely for metadata and then decoding them again for playback.
        if !is_preview {
            metadata.animated = Some(gif.is_animated());
            metadata.frame_count = Some(gif.frame_count());
        }
    }
    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err("Preload cancelled".to_owned());
    }
    const MAX_RETAINED_GIF_SOURCE_BYTES: usize = 64 * 1024 * 1024;
    let full_gif_source = retained_gif_source
        .filter(|bytes| bytes.len() <= MAX_RETAINED_GIF_SOURCE_BYTES)
        .map(std::sync::Arc::new);
    Ok(LoadedEntryData {
        image,
        metadata,
        file_type_detection,
        full_gif_source,
    })
}

/// Load an entry through the shared format-detection and decoding pipeline.
/// Used by Compare and mipmap fallback paths that do not need file metadata.
pub(crate) fn load_entry_content(
    entry: ImageEntry,
    threshold: u32,
    develop_raw: bool,
) -> Result<LoadedImage, String> {
    let bytes = read_entry_bytes_cancellable(&entry, None)?;
    decode_entry_bytes(&entry, bytes, threshold, develop_raw)
}

fn decode_entry_bytes(
    entry: &ImageEntry,
    bytes: Vec<u8>,
    threshold: u32,
    develop_raw: bool,
) -> Result<LoadedImage, String> {
    decode_entry_bytes_with_source(entry, bytes, threshold, develop_raw)
        .map(|(image, _retained_source)| image)
}

fn decode_entry_bytes_with_source(
    entry: &ImageEntry,
    bytes: Vec<u8>,
    threshold: u32,
    develop_raw: bool,
) -> Result<(LoadedImage, Option<Vec<u8>>), String> {
    let path_hint = entry_path_hint(entry);
    let extension_is_raw = path_hint
        .and_then(std::path::Path::extension)
        .and_then(|extension| extension.to_str())
        .and_then(crate::imaging::core::ImageFormat::from_extension)
        == Some(crate::imaging::core::ImageFormat::CameraRaw);
    // Some RAW files arrive from archives or have misleading extensions. Use
    // content detection as a second signal before deciding to develop sensor data.
    let content_is_raw = zenraw::is_raw_file(&bytes);
    let is_raw = extension_is_raw || content_is_raw;

    if is_raw && !develop_raw {
        let source_name = path_hint
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "archive entry".to_owned());
        eprintln!(
            "[RAW preview] Trying embedded preview for {source_name} ({} bytes; extension-recognized={extension_is_raw}, content-recognized={content_is_raw})",
            bytes.len()
        );
        match crate::decoder::extract_raw_preview_diagnostic(&bytes, path_hint) {
            Ok(Some(preview)) => {
                eprintln!(
                    "[RAW preview] Embedded preview extracted for {source_name} ({} bytes)",
                    preview.len()
                );
                return crate::decoder::format_detection::load_bytes_with_detection_and_source(
                    preview,
                    Some(std::path::Path::new("embedded-preview.jpg")),
                    threshold,
                    &DecodeOptions,
                    None,
                );
            }
            Ok(None) => eprintln!(
                "[RAW preview] Rawler reported no preview/full image/thumbnail for {source_name}; falling back to full RAW development"
            ),
            Err(error) => eprintln!(
                "[RAW preview] Extraction failed for {source_name}: {error}; falling back to full RAW development"
            ),
        }
    }

    crate::decoder::format_detection::load_bytes_with_detection_and_source(
        bytes,
        path_hint,
        threshold,
        &DecodeOptions,
        None,
    )
}
