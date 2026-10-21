//! Queue persistence and path-repair helpers.
//!
//! Kept separate from the sidebar UI so the queue file format can be tested
//! independently of rendering.
use std::fs;
use std::path::{Path, PathBuf};

const QUEUE_MAGIC: &str = "JPEG_VIEWER_QUEUE\t1";

pub(super) fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

pub(super) fn paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn encode_hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(text: &str) -> Result<String, String> {
    if text.len() % 2 != 0 {
        return Err("invalid hex field length".into());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex field".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| "queue path is not valid UTF-8".into())
}

pub(super) fn write_queue(path: &Path, items: &[PathBuf], root: Option<&Path>) -> Result<(), String> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let root = root
        .filter(|candidate| items.iter().all(|item| item.starts_with(candidate)))
        .map(Path::to_path_buf)
        .or_else(|| common_parent(items));
    let mut output = format!("{QUEUE_MAGIC}\n");
    if let Some(root) = &root {
        output.push_str(&format!("ROOT\t{}\n", encode_hex(&root.to_string_lossy())));
    }
    for item in items {
        if let Ok(relative) = item.strip_prefix(base) {
            output.push_str(&format!(
                "ITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ));
        } else {
            output.push_str(&format!(
                "ITEM\tA\t{}\n",
                encode_hex(&item.to_string_lossy())
            ));
        }
    }
    let temp = path.with_extension("jvq.tmp");
    let backup = path.with_extension("jvq.bak");
    fs::write(&temp, output).map_err(|e| e.to_string())?;
    let had_existing = path.exists();
    if had_existing {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(path, &backup)
            .map_err(|e| format!("cannot protect existing queue file: {e}"))?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot commit queue file: {error}"));
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub(super) fn read_queue(path: &Path) -> Result<(Vec<PathBuf>, Option<PathBuf>), String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut lines = contents.lines();
    if lines.next() != Some(QUEUE_MAGIC) {
        // Plain-text path lists are accepted as an import format. Missing paths
        // are retained just like missing entries in native queue files.
        let mut imported = Vec::new();
        for line in contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let item = PathBuf::from(line);
            imported.push(if item.is_absolute() {
                item
            } else {
                absolute_path(&base.join(item))
            });
        }
        let imported_root = common_parent(&imported);
        return Ok((imported, imported_root));
    }
    let mut root = None;
    let mut items = Vec::new();
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["ROOT", encoded] => root = Some(PathBuf::from(decode_hex(encoded)?)),
            ["ITEM", kind @ ("A" | "R"), encoded] => {
                let value = PathBuf::from(decode_hex(encoded)?);
                let resolved = if *kind == "R" {
                    absolute_path(&base.join(value))
                } else {
                    value
                };
                items.push(resolved);
            }
            [""] => {}
            _ => return Err("malformed queue entry".into()),
        }
    }
    Ok((items, root))
}

pub(super) fn remap_paths(items: &mut [PathBuf], old_root: &Path, new_root: &Path) -> usize {
    let mut remapped = 0;
    for path in items {
        if path.exists() {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(old_root) {
            let candidate = new_root.join(relative);
            if candidate.exists() {
                *path = candidate;
                remapped += 1;
            }
        }
    }
    remapped
}

fn common_parent(items: &[PathBuf]) -> Option<PathBuf> {
    let first = items.first()?;
    let mut candidate = first.parent()?.to_path_buf();
    for path in items.iter().skip(1) {
        while !path.starts_with(&candidate) {
            if !candidate.pop() {
                return None;
            }
        }
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("jpeg-viewer-queue-{stamp}-{name}"))
    }

    #[test]
    fn queue_roundtrip_preserves_missing_absolute_paths() {
        let dir = temp_path("roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("sample.jvq");
        let missing = dir.join("not-here.jpg");
        write_queue(&queue_file, std::slice::from_ref(&missing), Some(&dir)).unwrap();
        let (loaded, root) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![missing]);
        assert_eq!(root, Some(dir.clone()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hex_roundtrip_supports_unicode_and_delimiters() {
        let original = "фото folder/一枚 image.jpg";
        assert_eq!(decode_hex(&encode_hex(original)).unwrap(), original);
    }

    #[test]
    fn relative_paths_resolve_from_queue_file_directory() {
        let dir = temp_path("relative");
        fs::create_dir_all(&dir).unwrap();
        let queue_file = dir.join("portable.jvq");
        let relative = PathBuf::from("sub/photo.jpg");
        fs::write(
            &queue_file,
            format!(
                "{QUEUE_MAGIC}\nITEM\tR\t{}\n",
                encode_hex(&relative.to_string_lossy())
            ),
        )
        .unwrap();
        let (loaded, _) = read_queue(&queue_file).unwrap();
        assert_eq!(loaded, vec![absolute_path(&dir.join(relative))]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remapping_preserves_unresolved_items_and_repairs_moved_root() {
        let base = temp_path("remap");
        let old_root = base.join("old");
        let new_root = base.join("new");
        fs::create_dir_all(new_root.join("sub")).unwrap();
        fs::write(new_root.join("sub/photo.jpg"), b"test").unwrap();
        let mut items = vec![old_root.join("sub/photo.jpg"), old_root.join("other.jpg")];
        let remapped = remap_paths(&mut items, &old_root, &new_root);
        assert_eq!(remapped, 1);
        assert_eq!(items[0], new_root.join("sub/photo.jpg"));
        assert_eq!(items[1], old_root.join("other.jpg"));
        fs::remove_dir_all(base).unwrap();
    }
}
