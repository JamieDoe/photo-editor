use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::natural_cmp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoEntry {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    /// Modification time, milliseconds since the Unix epoch (0 if unavailable).
    pub modified_ms: u64,
    /// Lowercase extension, e.g. "nef".
    pub extension: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    /// Canonical path of the listed folder.
    pub path: PathBuf,
    pub folders: Vec<FolderEntry>,
    pub photos: Vec<PhotoEntry>,
    /// Entries that could not be read (permissions, broken links).
    pub skipped: usize,
}

/// Lists subfolders and photos (files whose extension is in `extensions`, compared
/// case-insensitively) directly inside `dir`. Hidden entries (dot-prefixed, including
/// macOS `._` metadata files) are skipped. Both lists are in natural order.
///
/// This reads directory metadata only; no photo is opened.
pub fn list_folder(dir: &Path, extensions: &[&str]) -> std::io::Result<FolderListing> {
    let path = dir.canonicalize()?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "not a folder",
        ));
    }
    let mut folders = Vec::new();
    let mut photos = Vec::new();
    let mut skipped = 0;
    for entry in std::fs::read_dir(&path)? {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // `metadata` follows symlinks, so linked folders and photos behave like the real
        // thing; a broken link is counted as skipped.
        let Ok(meta) = std::fs::metadata(entry.path()) else {
            skipped += 1;
            continue;
        };
        if meta.is_dir() {
            folders.push(FolderEntry {
                name,
                path: entry.path(),
            });
            continue;
        }
        let extension = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let Some(extension) =
            extension.filter(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        else {
            continue;
        };
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        photos.push(PhotoEntry {
            name,
            path: entry.path(),
            size_bytes: meta.len(),
            modified_ms,
            extension,
        });
    }
    folders.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    photos.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(FolderListing {
        path,
        folders,
        photos,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTS: &[&str] = &["nef", "jpg", "dng"];

    fn touch(p: &Path) {
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn lists_folders_and_supported_photos_in_natural_order() {
        let dir = fixtures::TempDir::new("folders-list");
        let root = dir.path();
        for f in [
            "DSC_10.NEF",
            "DSC_2.nef",
            "notes.txt",
            ".hidden.jpg",
            "._DSC_2.nef",
            "a.JPG",
            "b.dng",
        ] {
            touch(&root.join(f));
        }
        for d in ["2024", "2023-10", ".git"] {
            std::fs::create_dir(root.join(d)).unwrap();
        }
        let l = list_folder(root, EXTS).unwrap();
        let names = |v: Vec<String>| v;
        assert_eq!(
            names(l.folders.iter().map(|f| f.name.clone()).collect()),
            vec!["2023-10", "2024"]
        );
        assert_eq!(
            names(l.photos.iter().map(|p| p.name.clone()).collect()),
            vec!["a.JPG", "b.dng", "DSC_2.nef", "DSC_10.NEF"]
        );
        assert_eq!(l.photos[0].extension, "jpg");
        assert_eq!(l.photos[0].size_bytes, 1);
        assert!(l.photos[0].modified_ms > 0);
        assert_eq!(l.path, root.canonicalize().unwrap());
    }

    #[test]
    fn missing_or_file_paths_are_errors() {
        let dir = fixtures::TempDir::new("folders-errors");
        assert_eq!(
            list_folder(&dir.path().join("nope"), EXTS)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        let file = dir.path().join("x.jpg");
        touch(&file);
        assert!(list_folder(&file, EXTS).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn broken_links_are_skipped_not_fatal() {
        let dir = fixtures::TempDir::new("folders-links");
        std::os::unix::fs::symlink(
            dir.path().join("missing.nef"),
            dir.path().join("broken.nef"),
        )
        .unwrap();
        touch(&dir.path().join("ok.nef"));
        let l = list_folder(dir.path(), EXTS).unwrap();
        assert_eq!(l.photos.len(), 1);
        assert_eq!(l.skipped, 1);
    }
}
