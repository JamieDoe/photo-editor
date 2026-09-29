//! Library backups (ADR 0021). Ratings, flags and edits cannot be rebuilt from the
//! files, so the catalogue is copied regularly into a backups folder.
//!
//! A backup is a consistent, compacted snapshot made with SQLite's `VACUUM INTO` on a
//! separate read-only connection (WAL lets it run alongside the app's writes). Every
//! copy is checked with `quick_check` before it counts. Files are named
//! `catalogue-<unix ms>-<kind>.sqlite`, so listing needs no database access.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::CatalogueError;
use crate::catalogue::{Catalogue, Result};

const PREFIX: &str = "catalogue-";
const SUFFIX: &str = ".sqlite";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Why a backup was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupKind {
    /// Scheduled (at start-up or after changes).
    Auto,
    /// "Back up now".
    Manual,
    /// Just before upgrading the catalogue from `version`.
    PreUpgrade(u32),
}

impl BackupKind {
    fn tag(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Manual => "manual".into(),
            Self::PreUpgrade(v) => format!("pre-upgrade-v{v}"),
        }
    }

    fn parse(tag: &str) -> Option<Self> {
        match tag {
            "auto" => Some(Self::Auto),
            "manual" => Some(Self::Manual),
            _ => tag
                .strip_prefix("pre-upgrade-v")
                .and_then(|v| v.parse().ok())
                .map(Self::PreUpgrade),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub taken_at_ms: i64,
    pub kind: BackupKind,
    pub bytes: u64,
}

/// The backups folder.
#[derive(Debug, Clone)]
pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Backups, newest first. A missing folder is simply empty.
    pub fn list(&self) -> io::Result<Vec<BackupInfo>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out: Vec<BackupInfo> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let (taken_at_ms, kind) = parse_name(&name)?;
                let bytes = entry.metadata().ok()?.len();
                Some(BackupInfo {
                    path: entry.path(),
                    taken_at_ms,
                    kind,
                    bytes,
                })
            })
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.taken_at_ms));
        Ok(out)
    }

    /// Snapshots `catalogue` (which must be file-backed).
    pub fn back_up(&self, catalogue: &Catalogue, kind: BackupKind) -> Result<BackupInfo> {
        let source = catalogue.path().ok_or_else(|| {
            CatalogueError::Io(io::Error::new(
                io::ErrorKind::Unsupported,
                "an in-memory catalogue has no file to back up",
            ))
        })?;
        self.back_up_file(source, kind)
    }

    /// Snapshots the catalogue file at `source` (also before it is opened, for
    /// pre-upgrade backups).
    pub fn back_up_file(&self, source: &Path, kind: BackupKind) -> Result<BackupInfo> {
        std::fs::create_dir_all(&self.dir)?;
        let taken_at_ms = now_ms();
        let name = format!("{PREFIX}{taken_at_ms:013}-{}{SUFFIX}", kind.tag());
        let dest = self.dir.join(&name);
        let temp = self.dir.join(format!(".{name}.tmp"));
        let _ = std::fs::remove_file(&temp);
        let result = snapshot(source, &temp).and_then(|()| verify(&temp));
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(e);
        }
        std::fs::rename(&temp, &dest)?;
        let bytes = std::fs::metadata(&dest)?.len();
        Ok(BackupInfo {
            path: dest,
            taken_at_ms,
            kind,
            bytes,
        })
    }

    /// Copies `backup` (from another store) into this one under the same name, so both
    /// folders apply the same retention. Verified before it counts; a copy already here
    /// is left as is.
    pub fn import(&self, backup: &BackupInfo) -> Result<BackupInfo> {
        let name = backup
            .path
            .file_name()
            .ok_or_else(|| CatalogueError::Io(io::Error::other("backup has no file name")))?;
        let dest = self.dir.join(name);
        if !dest.exists() {
            std::fs::create_dir_all(&self.dir)?;
            let temp = self.dir.join(format!(".{}.tmp", name.to_string_lossy()));
            let copied = std::fs::copy(&backup.path, &temp)
                .map_err(CatalogueError::from)
                .and_then(|_| verify(&temp));
            if let Err(e) = copied {
                let _ = std::fs::remove_file(&temp);
                return Err(e);
            }
            std::fs::rename(&temp, &dest)?;
        }
        Ok(BackupInfo {
            bytes: std::fs::metadata(&dest)?.len(),
            path: dest,
            ..backup.clone()
        })
    }

    /// Deletes backups the retention policy no longer keeps; returns how many.
    pub fn prune(&self, now_ms: i64) -> io::Result<usize> {
        let all = self.list()?;
        let mut removed = 0;
        for b in to_prune(&all, now_ms) {
            if std::fs::remove_file(&b.path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// The newest backup that passes an integrity check.
    pub fn latest_valid(&self) -> Option<BackupInfo> {
        self.list()
            .ok()?
            .into_iter()
            .find(|b| verify(&b.path).is_ok())
    }

    /// Replaces the catalogue file at `dest` with `backup`. The catalogue must not be
    /// open. Any `-wal`/`-shm` files of the old catalogue are removed.
    pub fn restore(&self, backup: &BackupInfo, dest: &Path) -> Result<()> {
        verify(&backup.path)?;
        let temp = dest.with_extension("restoring");
        std::fs::copy(&backup.path, &temp)?;
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", dest.display()));
        }
        std::fs::rename(&temp, dest)?;
        Ok(())
    }
}

/// The newest backup that passes an integrity check, across several stores (the local
/// folder and a copy on another drive).
pub fn newest_valid(stores: &[&BackupStore]) -> Option<BackupInfo> {
    let mut all: Vec<BackupInfo> = stores
        .iter()
        .filter_map(|s| s.list().ok())
        .flatten()
        .collect();
    all.sort_by_key(|b| std::cmp::Reverse(b.taken_at_ms));
    all.into_iter().find(|b| verify(&b.path).is_ok())
}

/// The schema version of the catalogue file at `path` without opening it for writing
/// (0 for a new, empty file; `None` if it does not exist).
pub fn schema_version_of(path: &Path) -> Result<Option<i64>> {
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(Some(
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))?,
    ))
}

/// Which backups to delete. Kept: the newest 3; the newest of each of the last 7 days;
/// the newest of each of the 4 weeks before that; the newest 2 pre-upgrade backups.
/// At most 16 files, however long the app runs.
pub fn to_prune(backups: &[BackupInfo], now_ms: i64) -> Vec<BackupInfo> {
    let mut sorted: Vec<&BackupInfo> = backups.iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.taken_at_ms));
    let mut keep = std::collections::HashSet::new();
    let (upgrades, regular): (Vec<&BackupInfo>, Vec<&BackupInfo>) = sorted
        .into_iter()
        .partition(|b| matches!(b.kind, BackupKind::PreUpgrade(_)));
    for b in upgrades.iter().take(2) {
        keep.insert(b.path.clone());
    }
    for b in regular.iter().take(3) {
        keep.insert(b.path.clone());
    }
    let mut buckets = std::collections::HashSet::new();
    for b in &regular {
        let age = now_ms - b.taken_at_ms;
        let bucket = if age < 7 * DAY_MS {
            Some(("day", b.taken_at_ms.div_euclid(DAY_MS)))
        } else if age < 35 * DAY_MS {
            Some(("week", b.taken_at_ms.div_euclid(7 * DAY_MS)))
        } else {
            None
        };
        // Newest first, so the first backup seen in a bucket is its newest.
        if let Some(bucket) = bucket
            && buckets.insert(bucket)
        {
            keep.insert(b.path.clone());
        }
    }
    backups
        .iter()
        .filter(|b| !keep.contains(&b.path))
        .cloned()
        .collect()
}

fn snapshot(source: &Path, dest: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let dest = dest
        .to_str()
        .ok_or_else(|| CatalogueError::NonUnicodePath(dest.to_path_buf()))?;
    conn.execute("VACUUM INTO ?1", [dest])?;
    Ok(())
}

fn verify(path: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check == "ok" {
        Ok(())
    } else {
        Err(CatalogueError::Corrupt(check))
    }
}

fn parse_name(name: &str) -> Option<(i64, BackupKind)> {
    let stem = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let (ms, tag) = stem.split_once('-')?;
    Some((ms.parse().ok()?, BackupKind::parse(tag)?))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests;
