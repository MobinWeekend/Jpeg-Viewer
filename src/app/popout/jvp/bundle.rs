use super::*;

pub(super) fn write_bundled_entry<W: Write>(writer: &mut W, entry: &ImageEntry) -> Result<(), String> {
    match entry {
        ImageEntry::File(path) => {
            let mut source = std::fs::File::open(path)
                .map_err(|error| format!("Could not open the image for bundling: {error}"))?;
            copy_limited_image(&mut source, writer)
        }
        ImageEntry::Zip(item) => {
            let file = std::fs::File::open(&item.archive_path)
                .map_err(|error| format!("Could not open the ZIP image for bundling: {error}"))?;
            let mut archive = ZipArchive::new(file)
                .map_err(|error| format!("Could not read the ZIP image for bundling: {error}"))?;
            let mut source = archive
                .by_index(item.entry_index)
                .map_err(|error| format!("Could not open the ZIP image entry: {error}"))?;
            copy_limited_image(&mut source, writer)
        }
        ImageEntry::S7z(_) | ImageEntry::Rar(_) => {
            // The 7z and RAR readers in the existing archive abstraction expose
            // decoded entries as byte vectors. Keep that path, but enforce the
            // same output limit before writing anything into the JVP.
            let bytes = crate::app::workspace::read_archive_entry_for_share(entry)
                .map_err(|error| format!("Could not extract the archive image: {error}"))?;
            if bytes.is_empty() {
                return Err("The current image is empty.".into());
            }
            if bytes.len() as u64 > MAX_BUNDLED_IMAGE_BYTES {
                return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
            }
            writer
                .write_all(&bytes)
                .map_err(|error| format!("Could not write the bundled image: {error}"))
        }
    }
}

pub(super) fn copy_limited_image<R: Read, W: Write>(source: &mut R, destination: &mut W) -> Result<(), String> {
    let mut limited = source.take(MAX_BUNDLED_IMAGE_BYTES + 1);
    let copied = std::io::copy(&mut limited, destination)
        .map_err(|error| format!("Could not copy the bundled image: {error}"))?;
    if copied == 0 {
        return Err("The current image is empty.".into());
    }
    if copied > MAX_BUNDLED_IMAGE_BYTES {
        return Err("The current image exceeds the 2 GiB bundle safety limit.".into());
    }
    Ok(())
}


pub(super) fn entry_file_name(entry: &ImageEntry) -> String {
    let raw = match entry {
        ImageEntry::File(path) => path.file_name().and_then(|name| name.to_str()),
        ImageEntry::Zip(item) => Some(item.name.as_str()),
        ImageEntry::S7z(item) => Some(item.name.as_str()),
        ImageEntry::Rar(item) => Some(item.name.as_str()),
    }
    .unwrap_or("image.bin");
    let filename = raw.rsplit(['/', '\\']).next().unwrap_or("image.bin");
    let cleaned: String = filename
        .chars()
        .filter(|character| !character.is_control() && !matches!(*character, '/' | '\\' | ':'))
        .take(180)
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "image.bin".to_owned()
    } else {
        cleaned
    }
}

pub(super) fn validate_bundle_member_path(path: &str) -> Result<(), String> {
    let normalized = path.replace('\\', "/");
    if !normalized.starts_with("image/")
        || normalized.contains("../")
        || normalized.starts_with('/')
        || normalized.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err("The JVP contains an unsafe bundled image path.".into());
    }
    Ok(())
}

pub(super) fn write_bundled_image(member_path: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let filename = Path::new(member_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.bin");
    let extension: String = Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("img")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(16)
        .collect();
    let extension = if extension.is_empty() { "img".to_owned() } else { extension };
    let root = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Jpeg Viewer")
        .join("jvp-images");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create the JVP image cache: {error}"))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..100_u32 {
        let path = root.join(format!(
            "image-{}-{timestamp}-{attempt}.{extension}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = fs::remove_file(&path);
                    return Err(format!("Could not materialize the bundled image: {error}"));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("Could not create the extracted image file: {error}"));
            }
        }
    }
    Err("Could not allocate a unique filename for the bundled image.".into())
}
