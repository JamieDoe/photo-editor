use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

/// Counts from a [`walk_photos`] pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub folders: usize,
    /// Folders or entries that could not be read.
    pub skipped: usize,
    /// True if the visitor stopped the walk early.
    pub stopped: bool,
}

/// Visits every supported photo beneath `root` (depth first). Hidden entries are
/// skipped. Symlinks are never followed, so there are no cycles and nothing outside
/// `root` is visited. Unreadable folders are counted and skipped. Only an unreadable
/// `root` is an error. `visit` can stop the walk (e.g. on cancellation).
pub fn walk_photos(
    root: &Path,
    extensions: &[&str],
    mut visit: impl FnMut(PathBuf) -> ControlFlow<()>,
) -> std::io::Result<WalkStats> {
    let mut stats = WalkStats::default();
    let mut stack = vec![root.to_path_buf()];
    let mut first = true;
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if first => return Err(e),
            Err(_) => {
                stats.skipped += 1;
                continue;
            }
        };
        first = false;
        stats.folders += 1;
        let mut subdirs = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else {
                stats.skipped += 1;
                continue;
            };
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            // `file_type` does not follow symlinks.
            let Ok(kind) = entry.file_type() else {
                stats.skipped += 1;
                continue;
            };
            if kind.is_dir() {
                subdirs.push(entry.path());
            } else if kind.is_file()
                && has_extension(Path::new(&name), extensions)
                && visit(entry.path()).is_break()
            {
                stats.stopped = true;
                return Ok(stats);
            }
        }
        // Reverse so folders are visited in directory order (stack is LIFO).
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(stats)
}

fn has_extension(name: &Path, extensions: &[&str]) -> bool {
    name.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn finds_photos_recursively_and_skips_hidden() {
        let dir = fixtures::TempDir::new("walk");
        let root = dir.path();
        for f in [
            "a.nef",
            "sub/b.JPG",
            "sub/deeper/c.dng",
            "sub/notes.txt",
            ".cache/d.nef",
            "sub/._b.JPG",
        ] {
            touch(&root.join(f));
        }
        let mut found = Vec::new();
        let stats = walk_photos(root, &["nef", "jpg", "dng"], |p| {
            found.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            ControlFlow::Continue(())
        })
        .unwrap();
        found.sort();
        assert_eq!(found, ["a.nef", "sub/b.JPG", "sub/deeper/c.dng"]);
        assert_eq!(stats.folders, 3);
        assert!(!stats.stopped);
    }

    #[test]
    fn visitor_can_stop_the_walk() {
        let dir = fixtures::TempDir::new("walk-stop");
        for i in 0..10 {
            touch(&dir.path().join(format!("{i}.nef")));
        }
        let mut n = 0;
        let stats = walk_photos(dir.path(), &["nef"], |_| {
            n += 1;
            if n == 3 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
        assert_eq!(n, 3);
        assert!(stats.stopped);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let dir = fixtures::TempDir::new("walk-links");
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        touch(&outside.join("secret.nef"));
        touch(&root.join("real.nef"));
        std::os::unix::fs::symlink(&outside, root.join("linkdir")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.nef"), root.join("link.nef")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap(); // would cycle
        let mut found = Vec::new();
        walk_photos(&root, &["nef"], |p| {
            found.push(p.file_name().unwrap().to_string_lossy().into_owned());
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(found, ["real.nef"]);
    }

    #[test]
    fn unreadable_root_is_an_error() {
        assert!(
            walk_photos(Path::new("/definitely/missing"), &["nef"], |_| {
                ControlFlow::Continue(())
            })
            .is_err()
        );
    }
}
