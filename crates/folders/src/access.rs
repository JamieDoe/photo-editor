use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Folders the user has explicitly granted (via the native folder dialog, or folders
/// granted that way in earlier sessions and remembered in settings).
///
/// The webview can only list folders and open photos inside granted folders. A path
/// is inside a root only after canonicalisation, so `..` and symlinks cannot escape.
#[derive(Debug, Default)]
pub struct FolderAccess {
    roots: RwLock<Vec<PathBuf>>,
}

impl FolderAccess {
    pub fn new() -> Self {
        Self::default()
    }

    /// Grants `folder` (and everything beneath it). Returns its canonical path.
    pub fn grant(&self, folder: &Path) -> std::io::Result<PathBuf> {
        let canonical = folder.canonicalize()?;
        let mut roots = self.roots.write().expect("access lock");
        if !roots.contains(&canonical) {
            roots.push(canonical.clone());
        }
        Ok(canonical)
    }

    /// Grants folders remembered from earlier sessions; ones that no longer exist are
    /// ignored.
    pub fn grant_remembered<'a>(&self, folders: impl IntoIterator<Item = &'a str>) {
        for f in folders {
            let _ = self.grant(Path::new(f));
        }
    }

    /// The canonical form of `path` if it lies within a granted folder.
    pub fn check(&self, path: &Path) -> Option<PathBuf> {
        let canonical = path.canonicalize().ok()?;
        let roots = self.roots.read().expect("access lock");
        roots
            .iter()
            .any(|r| canonical.starts_with(r))
            .then_some(canonical)
    }

    /// The granted root containing `path`, if any (used to stop "up" navigation there).
    pub fn root_of(&self, path: &Path) -> Option<PathBuf> {
        let canonical = path.canonicalize().ok()?;
        let roots = self.roots.read().expect("access lock");
        roots
            .iter()
            .filter(|r| canonical.starts_with(r))
            .max_by_key(|r| r.components().count())
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_granted_trees_are_accessible() {
        let dir = fixtures::TempDir::new("folders-access");
        let photos = dir.path().join("photos");
        let other = dir.path().join("other");
        std::fs::create_dir_all(photos.join("2024")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let access = FolderAccess::new();
        assert!(access.check(&photos).is_none(), "nothing granted yet");
        access.grant(&photos).unwrap();
        assert!(access.check(&photos.join("2024")).is_some());
        assert!(access.check(&other).is_none());
        // `..` cannot escape the granted tree.
        assert!(access.check(&photos.join("2024/../../other")).is_none());
        assert_eq!(
            access.root_of(&photos.join("2024")),
            Some(photos.canonicalize().unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape() {
        let dir = fixtures::TempDir::new("folders-access-link");
        let granted = dir.path().join("granted");
        let secret = dir.path().join("secret");
        std::fs::create_dir_all(&granted).unwrap();
        std::fs::create_dir_all(&secret).unwrap();
        std::os::unix::fs::symlink(&secret, granted.join("link")).unwrap();
        let access = FolderAccess::new();
        access.grant(&granted).unwrap();
        assert!(
            access.check(&granted.join("link")).is_none(),
            "link resolves outside the grant"
        );
    }

    #[test]
    fn remembered_folders_that_vanished_are_ignored() {
        let dir = fixtures::TempDir::new("folders-access-remembered");
        let access = FolderAccess::new();
        let existing = dir.path().to_string_lossy().into_owned();
        access.grant_remembered(["/definitely/not/here", existing.as_str()]);
        assert!(access.check(dir.path()).is_some());
    }
}
