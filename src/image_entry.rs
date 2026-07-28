use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ArchiveImage {
    pub archive_path: PathBuf,
    pub entry_index: usize,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct S7ArchiveImage {
    pub archive_path: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct RarArchiveImage {
    pub archive_path: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug)]
pub enum ImageEntry {
    File(PathBuf),
    Zip(ArchiveImage),
    S7z(S7ArchiveImage),
    Rar(RarArchiveImage),
}

impl ImageEntry {
    pub fn get_id(&self) -> String {
        match self {
            ImageEntry::File(path) => format!("file:{}", path.display()),
            ImageEntry::Zip(zip) => format!("zip:{}:{}", zip.archive_path.display(), zip.entry_index),
            ImageEntry::S7z(s7z) => format!("7z:{}:{}", s7z.archive_path.display(), s7z.name),
            ImageEntry::Rar(rar) => format!("rar:{}:{}", rar.archive_path.display(), rar.name),
        }
    }
}