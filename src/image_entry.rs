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
pub enum ImageEntry {
    File(PathBuf),
    Zip(ArchiveImage),
    S7z(S7ArchiveImage),
}