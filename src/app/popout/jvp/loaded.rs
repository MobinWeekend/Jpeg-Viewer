use super::*;

impl LoadedJvp {
    pub(in crate::app::popout) fn preset(&self) -> PopoutPreset {
        self.manifest.preset.clone()
    }

    pub(in crate::app::popout) fn has_image(&self) -> bool {
        self.manifest.image.is_some()
    }

    pub(in crate::app::popout) fn resolve_image(&self) -> Result<Option<ImageEntry>, String> {
        let Some(image) = self.manifest.image.as_ref() else {
            return Ok(None);
        };
        let entry = match image {
            JvpImage::File { path } => {
                let path = PathBuf::from(path);
                if !path.is_file() {
                    return Err(format!("Linked image does not exist: {}", path.display()));
                }
                ImageEntry::File(path)
            }
            JvpImage::Zip {
                archive_path,
                entry_index,
                name,
            } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!(
                        "Linked ZIP archive does not exist: {}",
                        archive_path.display()
                    ));
                }
                let mut archive = super::archive::open_zip(&archive_path)?;
                let member = archive.by_index(*entry_index).map_err(|error| {
                    format!("The linked ZIP entry at index {entry_index} is unavailable: {error}")
                })?;
                let member_name = member
                    .name()
                    .map_err(|e| format!("Failed to read ZIP entry name: {e}"))?;

                if member_name.as_ref() != name.as_str() {
                    return Err(format!(
                        "The linked ZIP entry changed: expected '{name}', found '{}'",
                        member_name
                    ));
                }
                ImageEntry::Zip(ArchiveImage {
                    archive_path,
                    entry_index: *entry_index,
                    name: name.clone(),
                })
            }
            JvpImage::SevenZip { archive_path, name } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!(
                        "Linked 7z archive does not exist: {}",
                        archive_path.display()
                    ));
                }
                ImageEntry::S7z(S7ArchiveImage {
                    archive_path,
                    name: name.clone(),
                })
            }
            JvpImage::Rar { archive_path, name } => {
                let archive_path = PathBuf::from(archive_path);
                if !archive_path.is_file() {
                    return Err(format!(
                        "Linked RAR archive does not exist: {}",
                        archive_path.display()
                    ));
                }
                ImageEntry::Rar(RarArchiveImage {
                    archive_path,
                    name: name.clone(),
                })
            }
            JvpImage::Bundled { member_path } => {
                super::bundle::validate_bundle_member_path(member_path)?;
                let mut archive = super::archive::open_zip(&self.archive_path)?;
                let member = archive.by_name(member_path).map_err(|error| {
                    format!("The bundled image is missing from this JVP: {error}")
                })?;
                if member.size() > MAX_BUNDLED_IMAGE_BYTES {
                    return Err("The bundled image exceeds the 2 GiB safety limit.".into());
                }
                let mut bytes = Vec::new();
                member
                    .take(MAX_BUNDLED_IMAGE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("Could not extract the bundled image: {error}"))?;
                if bytes.len() as u64 > MAX_BUNDLED_IMAGE_BYTES {
                    return Err("The bundled image exceeds the 2 GiB safety limit.".into());
                }
                if crate::decoder::format_detection::detect_format(&bytes).is_none() {
                    return Err("The bundled data is not a supported image format.".into());
                }
                ImageEntry::File(super::bundle::write_bundled_image(member_path, &bytes)?)
            }
        };

        // Validate archive members from their directory metadata without
        // decompressing the image into memory or changing the active tab.
        let _ = super::archive::validate_linked_archive_entry(&entry)?;
        Ok(Some(entry))
    }
}
