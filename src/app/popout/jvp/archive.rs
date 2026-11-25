use super::*;

pub(super) fn open_zip(path: &Path) -> Result<ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    ZipArchive::new(file).map_err(|error| format!("Could not read JVP archive: {error}"))
}

pub(super) fn linked_image(entry: &ImageEntry) -> Result<JvpImage, String> {
    let _ = validate_linked_archive_entry(entry)?;
    let canonical_file = |path: &Path, description: &str| -> Result<String, String> {
        if !path.is_file() {
            return Err(format!(
                "The {description} is unavailable: {}",
                path.display()
            ));
        }
        path.canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("Could not resolve the {description}: {error}"))
    };
    Ok(match entry {
        ImageEntry::File(path) => JvpImage::File {
            path: canonical_file(path, "image file")?,
        },
        ImageEntry::Zip(item) => JvpImage::Zip {
            archive_path: canonical_file(&item.archive_path, "ZIP archive")?,
            entry_index: item.entry_index,
            name: item.name.clone(),
        },
        ImageEntry::S7z(item) => JvpImage::SevenZip {
            archive_path: canonical_file(&item.archive_path, "7z archive")?,
            name: item.name.clone(),
        },
        ImageEntry::Rar(item) => JvpImage::Rar {
            archive_path: canonical_file(&item.archive_path, "RAR archive")?,
            name: item.name.clone(),
        },
    })
}

pub(super) fn validate_linked_archive_entry(entry: &ImageEntry) -> Result<u64, String> {
    match entry {
        ImageEntry::File(path) => {
            if !path.is_file() {
                return Err(format!(
                    "The linked image file is unavailable: {}",
                    path.display()
                ));
            }
            fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|error| format!("Could not inspect the linked image file: {error}"))
        }
        ImageEntry::Zip(item) => {
            let file = std::fs::File::open(&item.archive_path)
                .map_err(|error| format!("Could not open the linked ZIP archive: {error}"))?;
            let mut archive = ZipArchive::new(file)
                .map_err(|error| format!("Could not read the linked ZIP archive: {error}"))?;
            let member = archive
                .by_index(item.entry_index)
                .map_err(|error| format!("The linked ZIP entry is unavailable: {error}"))?;
            let member_name = member
                .name()
                .map_err(|e| format!("Failed to read ZIP entry name: {e}"))?;

            if member_name != item.name.as_str() {
                return Err(format!(
                    "The linked ZIP entry changed: expected '{}', found '{}'",
                    item.name, member_name
                ));
            }
            Ok(member.size())
        }
        ImageEntry::S7z(item) => {
            let archive = sevenz_rust2::Archive::open(&item.archive_path)
                .map_err(|error| format!("Could not read the linked 7z archive: {error}"))?;
            archive
                .files
                .iter()
                .find(|file| file.name() == item.name.as_str())
                .map(|file| file.size)
                .ok_or_else(|| {
                    format!(
                        "The linked image '{}' is missing from the 7z archive.",
                        item.name
                    )
                })
        }
        ImageEntry::Rar(item) => {
            let archive = unrar::Archive::new(&item.archive_path)
                .open_for_listing()
                .map_err(|error| format!("Could not read the linked RAR archive: {error}"))?;
            let mut archive = archive;
            loop {
                let header = archive.read_header().map_err(|error| {
                    format!("Could not inspect the linked RAR archive: {error}")
                })?;
                let Some(header) = header else {
                    return Err(format!(
                        "The linked image '{}' is missing from the RAR archive.",
                        item.name
                    ));
                };
                let name = header.entry().filename.to_string_lossy().to_string();
                if name.as_str() == item.name.as_str() {
                    return Ok(header.entry().unpacked_size);
                }
                archive = header.skip().map_err(|error| {
                    format!("Could not inspect the linked RAR archive: {error}")
                })?;
            }
        }
    }
}
