//! File identity: recognise a photograph by content, not just by path.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use cache::{Fnv64, SourceId};

/// Bytes hashed from each end of the file for the content fingerprint.
const FINGERPRINT_SPAN: u64 = 64 * 1024;

/// Identity of a source file.
///
/// The path is recorded but excluded from [`SourceIdentity::source_id`], so a moved
/// file keeps its identity while a modified file gets a new one. The fingerprint reads
/// only the head and tail of the file to stay cheap on 100 MB RAWs; a full-content
/// hash can be added later for duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentity {
    pub canonical_path: PathBuf,
    pub size: u64,
    pub modified_unix_ns: u128,
    pub fingerprint: u64,
}

impl SourceIdentity {
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let canonical_path = path.canonicalize()?;
        let (size, modified_unix_ns) = Self::stat(&canonical_path)?;
        Self::from_known(canonical_path, size, modified_unix_ns)
    }

    /// Size and modification time (ns since the Unix epoch) without reading content.
    pub fn stat(path: &Path) -> std::io::Result<(u64, u128)> {
        let meta = std::fs::metadata(path)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        Ok((meta.len(), modified))
    }

    /// Completes an identity for an already canonical, already stat'ed path by reading
    /// the fingerprint (head and tail of the file).
    pub fn from_known(
        canonical_path: PathBuf,
        size: u64,
        modified_unix_ns: u128,
    ) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(&canonical_path)?;
        let mut hasher = Fnv64::new();
        hasher.write_u64(size);
        let mut buf = vec![0u8; FINGERPRINT_SPAN.min(size) as usize];
        file.read_exact(&mut buf)?;
        hasher.write(&buf);
        if size > FINGERPRINT_SPAN {
            let tail = FINGERPRINT_SPAN.min(size - FINGERPRINT_SPAN);
            file.seek(SeekFrom::End(-(tail as i64)))?;
            buf.resize(tail as usize, 0);
            file.read_exact(&mut buf)?;
            hasher.write(&buf);
        }
        Ok(Self {
            canonical_path,
            size,
            modified_unix_ns,
            fingerprint: hasher.finish(),
        })
    }

    /// Cache identity: content version, independent of location.
    pub fn source_id(&self) -> SourceId {
        let mut h = Fnv64::new();
        h.write_u64(self.size);
        h.write(&self.modified_unix_ns.to_le_bytes());
        h.write_u64(self.fingerprint);
        SourceId(h.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_file_keeps_identity_modified_file_does_not() {
        let dir = fixtures::TempDir::new("identity");
        let a = dir.path().join("a.raw");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&a, &data).unwrap();
        let before = SourceIdentity::from_path(&a).unwrap();

        let b = dir.path().join("b.raw");
        std::fs::rename(&a, &b).unwrap();
        let moved = SourceIdentity::from_path(&b).unwrap();
        assert_eq!(before.source_id(), moved.source_id());
        assert_ne!(before.canonical_path, moved.canonical_path);

        let mut changed = data.clone();
        changed[10] ^= 0xff;
        std::fs::write(&b, &changed).unwrap();
        let modified = SourceIdentity::from_path(&b).unwrap();
        assert_ne!(before.fingerprint, modified.fingerprint);
        assert_ne!(before.source_id(), modified.source_id());
    }

    #[test]
    fn small_and_empty_files() {
        let dir = fixtures::TempDir::new("identity-small");
        let p = dir.path().join("tiny");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(SourceIdentity::from_path(&p).unwrap().size, 3);
        std::fs::write(&p, b"").unwrap();
        assert_eq!(SourceIdentity::from_path(&p).unwrap().size, 0);
    }

    #[test]
    fn missing_file_errors() {
        assert!(SourceIdentity::from_path(Path::new("/nonexistent/file.nef")).is_err());
    }
}
