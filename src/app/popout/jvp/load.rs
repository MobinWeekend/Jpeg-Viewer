use super::*;

pub(in crate::app::popout) fn load_jvp(path: &Path) -> Result<LoadedJvp, String> {
    if !path.is_file() {
        return Err(format!("JVP file does not exist: {}", path.display()));
    }
    let mut archive = super::archive::open_zip(path)?;
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|error| format!("This is not a valid JPEG Viewer preset (manifest missing): {error}"))?;
    if manifest_file.size() > MAX_MANIFEST_BYTES {
        return Err("The JVP manifest is unexpectedly large.".into());
    }
    let mut manifest_bytes = Vec::new();
    (&mut manifest_file)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| format!("Could not read the JVP manifest: {error}"))?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("The JVP manifest is unexpectedly large.".into());
    }
    drop(manifest_file);
    drop(archive);

    let mut manifest: JvpManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("The JVP manifest is invalid: {error}"))?;
    if manifest.format != JVP_FORMAT {
        return Err("This archive is not a JPEG Viewer Pop-out preset.".into());
    }
    if manifest.version != JVP_VERSION {
        return Err(format!(
            "Unsupported JVP version {} (this version supports {}).",
            manifest.version, JVP_VERSION
        ));
    }
    manifest.preset.sanitize();
    if let Some(JvpImage::Bundled { member_path }) = manifest.image.as_ref() {
        super::bundle::validate_bundle_member_path(member_path)?;
        let mut archive = super::archive::open_zip(path)?;
        let member = archive
            .by_name(member_path)
            .map_err(|error| format!("The JVP references a missing bundled image: {error}"))?;
        if member.size() > MAX_BUNDLED_IMAGE_BYTES {
            return Err("The bundled image exceeds the 2 GiB safety limit.".into());
        }
    }

    Ok(LoadedJvp {
        manifest,
        archive_path: path.to_path_buf(),
    })
}

