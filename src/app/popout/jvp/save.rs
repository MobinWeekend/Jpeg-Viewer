use super::*;

pub(in crate::app::popout) fn save_jvp(
    path: &Path,
    preset: &PopoutPreset,
    entry: Option<&ImageEntry>,
    choice: SaveImageChoice,
) -> Result<(), String> {
    let mut preset = preset.clone();
    preset.sanitize();
    let mut image = None;
    let mut bundled_entry = None;

    match choice {
        SaveImageChoice::PresetOnly => {}
        SaveImageChoice::Link => {
            let entry = entry.ok_or_else(|| "There is no current image to link.".to_owned())?;
            image = Some(super::archive::linked_image(entry)?);
        }
        SaveImageChoice::Bundle => {
            let entry = entry.ok_or_else(|| "There is no current image to bundle.".to_owned())?;
            let uncompressed_size = super::archive::validate_linked_archive_entry(entry)?;
            if uncompressed_size > MAX_BUNDLED_IMAGE_BYTES {
                return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
            }
            let filename = super::bundle::entry_file_name(entry);
            let member_path = format!("image/{filename}");
            image = Some(JvpImage::Bundled {
                member_path: member_path.clone(),
            });
            bundled_entry = Some((member_path, entry.clone()));
        }
    }

    let manifest = JvpManifest {
        format: JVP_FORMAT.to_owned(),
        version: JVP_VERSION,
        preset,
        image,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Could not encode the JVP manifest: {error}"))?;
    let (temporary_path, file) = super::file_ops::create_temporary_file(path)?;
    let mut writer = ZipWriter::new(file);
    let write_result = (|| -> Result<(), String> {
        writer
            .start_file(
                MANIFEST_PATH,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|error| format!("Could not write the JVP manifest: {error}"))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(|error| format!("Could not write the JVP manifest: {error}"))?;
        if let Some((member_path, entry)) = bundled_entry {
            writer
                .start_file(
                    member_path,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .map_err(|error| format!("Could not add the bundled image to the JVP: {error}"))?;
            super::bundle::write_bundled_entry(&mut writer, &entry)?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(writer);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    let output = match writer.finish() {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!("Could not finish the JVP archive: {error}"));
        }
    };
    if let Err(error) = output.sync_all() {
        drop(output);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Could not flush the JVP archive to disk: {error}"));
    }
    drop(output);
    super::file_ops::replace_file_atomically(&temporary_path, path)?;
    Ok(())
}

