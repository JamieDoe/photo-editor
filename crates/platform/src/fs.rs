//! Filesystem helpers.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `bytes` to `dest` atomically: a temporary file in the same directory, then a
/// rename over the destination (`std::fs::rename` replaces existing files on Unix and
/// Windows). Readers see the old or the new content, never a partial file, and a failed
/// write leaves no temporary file behind.
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = dest.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no file name",
        )
    })?;
    let tmp: PathBuf = dir.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_content_and_leaves_no_temp_files() {
        let dir = fixtures::TempDir::new("platform-atomic");
        let dest = dir.path().join("out.bin");
        write_atomic(&dest, b"one").unwrap();
        write_atomic(&dest, b"two").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failure_leaves_nothing_behind() {
        let err = write_atomic(Path::new("/nonexistent-dir/out.bin"), b"x").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(write_atomic(Path::new("/"), b"x").is_err(), "no file name");
    }
}
