use super::*;

pub(super) fn create_temporary_file(destination: &Path) -> Result<(PathBuf, std::fs::File), String> {
    let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = destination.file_name().unwrap_or_else(|| std::ffi::OsStr::new("preset.jvp"));
    let filename = filename.to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..100_u32 {
        let path = parent.join(format!(
            ".{filename}.tmp-{}-{timestamp}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("Could not create a temporary JVP file: {error}"));
            }
        }
    }
    Err("Could not allocate a temporary filename for the JVP.".into())
}

pub(super) fn replace_file_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        if !destination.is_file() {
            let _ = fs::remove_file(temporary);
            return Err(format!("The destination is not a regular file: {}", destination.display()));
        }
        let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let filename = destination.file_name().unwrap_or_else(|| std::ffi::OsStr::new("preset.jvp"));
        let filename = filename.to_string_lossy();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup = (0..100_u32)
            .map(|attempt| parent.join(format!(
                ".{filename}.bak-{}-{timestamp}-{attempt}",
                std::process::id()
            )))
            .find(|path| !path.exists());
        let Some(backup) = backup else {
            let _ = fs::remove_file(temporary);
            return Err("Could not allocate a backup filename for the existing JVP.".into());
        };
        if let Err(error) = fs::rename(destination, &backup) {
            let _ = fs::remove_file(temporary);
            return Err(format!("Could not protect the existing JVP before saving: {error}"));
        }
        if let Err(error) = fs::rename(temporary, destination) {
            let restore_error = fs::rename(&backup, destination).err();
            let _ = fs::remove_file(temporary);
            return Err(match restore_error {
                Some(restore_error) => format!(
                    "Could not replace the JVP ({error}); restoring the previous file also failed ({restore_error}). The previous file remains at {}.",
                    backup.display()
                ),
                None => format!("Could not replace the JVP; the previous file was restored: {error}"),
            });
        }
        // The new preset is safely in place. A stale backup is harmless if the
        // OS refuses cleanup, so do not report a successful save as failed.
        let _ = fs::remove_file(backup);
        Ok(())
    } else {
        fs::rename(temporary, destination)
            .map_err(|error| {
                let _ = fs::remove_file(temporary);
                format!("Could not move the completed JVP into place: {error}")
            })
    }
}

