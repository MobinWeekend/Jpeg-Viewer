//! Operating-system integration for opening files with a chosen application and locating files.

use std::path::Path;

/// Ask the operating system to display its native “Open With” application chooser.
pub fn open_with(path: &Path) -> Result<(), String> {
    os_open_with::open_with(path.to_path_buf()).map_err(|error| error.to_string())
}

/// Open the containing folder and select the file where supported by the platform.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    os_open_with::show_in_folder(path.to_path_buf()).map_err(|error| error.to_string())
}
