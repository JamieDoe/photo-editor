use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{CatalogueError, FolderId, PhotoId, SourceIdentity, schema};

pub(crate) type Result<T> = std::result::Result<T, CatalogueError>;

/// Most same-content files checked on disk when looking for a move source.
const MAX_MOVE_CANDIDATES: i64 = 32;

/// Identifies one indexing pass; files not seen during a pass are marked missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFolder {
    pub id: FolderId,
    pub path: PathBuf,
    pub added_at_ms: i64,
}

/// What [`Catalogue::record_file`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// Same path, same size, modification time and fingerprint.
    Unchanged,
    /// Same path, different content (e.g. re-saved). Still the same photo.
    Changed,
    /// Same content found at a new path, and the old path no longer exists: the file
    /// was moved or renamed. The photo (and everything attached to it) follows.
    Moved { from: PathBuf },
    /// A photo the catalogue has not seen before (including copies of known photos).
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Present,
    /// Not seen during the last scan of its folder (deleted, moved outside the
    /// library, or on a disconnected drive). Kept so ratings and edits survive.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub photo: PhotoId,
    pub path: PathBuf,
    pub size: u64,
    pub modified_ns: i64,
    pub status: FileStatus,
}

/// The catalogue database. All methods block on SQLite; call them from background
/// jobs, never from the UI thread.
pub struct Catalogue {
    conn: Mutex<Connection>,
}

impl Catalogue {
    /// Opens (creating if needed) the catalogue at `path`, verifies its integrity and
    /// migrates it to the current schema.
    ///
    /// Returns [`CatalogueError::Corrupt`] if the file is damaged; the caller should
    /// move it aside and start a fresh catalogue (it is rebuilt by re-indexing).
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    /// An in-memory catalogue (tests, benchmarks).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // WAL: readers don't block the writer and a crash cannot corrupt committed data.
        // NORMAL sync is durable across app crashes (fsync at checkpoints).
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(CatalogueError::Corrupt(check));
        }
        schema::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("catalogue lock")
    }

    pub(crate) fn with_tx<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().expect("catalogue lock");
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Adds a library folder (a canonical path), or returns the existing one.
    pub fn add_folder(&self, path: &Path) -> Result<FolderId> {
        let p = text(path)?;
        self.with_tx(|tx| {
            tx.execute(
                "INSERT INTO folders (path, added_at_ms) VALUES (?1, ?2) ON CONFLICT(path) DO NOTHING",
                params![p, now_ms()],
            )?;
            Ok(FolderId(tx.query_row("SELECT id FROM folders WHERE path = ?1", [p], |r| r.get(0))?))
        })
    }

    pub fn folders(&self) -> Result<Vec<LibraryFolder>> {
        let conn = self.conn.lock().expect("catalogue lock");
        let mut stmt = conn.prepare("SELECT id, path, added_at_ms FROM folders ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok(LibraryFolder {
                id: FolderId(r.get(0)?),
                path: PathBuf::from(r.get::<_, String>(1)?),
                added_at_ms: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Removes a library folder, its files, and photos left without any file. The
    /// photographs on disk are not touched.
    pub fn remove_folder(&self, id: FolderId) -> Result<()> {
        self.with_tx(|tx| {
            tx.execute("DELETE FROM folders WHERE id = ?1", [id.0])?;
            tx.execute(
                "DELETE FROM photos WHERE id NOT IN (SELECT photo_id FROM files)",
                [],
            )?;
            Ok(())
        })
    }

    /// Starts an indexing pass.
    pub fn begin_scan(&self) -> Result<ScanId> {
        let conn = self.conn.lock().expect("catalogue lock");
        Ok(ScanId(conn.query_row(
            "SELECT COALESCE(MAX(last_seen_scan), 0) + 1 FROM files",
            [],
            |r| r.get(0),
        )?))
    }

    /// Records one file found in `folder` during `scan`. See [`RecordOutcome`].
    pub fn record_file(
        &self,
        folder: FolderId,
        identity: &SourceIdentity,
        scan: ScanId,
    ) -> Result<(PhotoId, RecordOutcome)> {
        self.with_tx(|tx| record(tx, folder, identity, scan))
    }

    /// Records many files in one transaction (much faster than one at a time).
    pub fn record_files(
        &self,
        folder: FolderId,
        identities: &[SourceIdentity],
        scan: ScanId,
    ) -> Result<Vec<(PhotoId, RecordOutcome)>> {
        self.with_tx(|tx| {
            identities
                .iter()
                .map(|id| record(tx, folder, id, scan))
                .collect()
        })
    }

    /// Fast path for rescans: marks each `(path, size, modified_ns)` as seen in `scan` if
    /// the catalogue already has that path with the same size and modification time.
    /// Returns, per entry, whether it was known and unchanged. Only the others need a
    /// fingerprint and [`Catalogue::record_files`].
    pub fn touch_unchanged(
        &self,
        entries: &[(PathBuf, u64, u128)],
        scan: ScanId,
    ) -> Result<Vec<bool>> {
        self.with_tx(|tx| {
            let mut stmt = tx.prepare_cached(
                "UPDATE files SET missing = 0, last_seen_scan = ?4 WHERE path = ?1 AND size = ?2 AND modified_ns = ?3",
            )?;
            entries
                .iter()
                .map(|(path, size, modified)| {
                    let Some(p) = path.to_str() else { return Ok(false) };
                    let modified = i64::try_from(*modified).unwrap_or(i64::MAX);
                    Ok(stmt.execute(params![p, *size as i64, modified, scan.0])? == 1)
                })
                .collect()
        })
    }

    /// Ends a pass: files in `folder` (optionally only beneath `within`) that were not
    /// recorded during `scan` are marked missing. Returns how many were.
    pub fn finish_scan(
        &self,
        folder: FolderId,
        scan: ScanId,
        within: Option<&Path>,
    ) -> Result<usize> {
        let prefix = within.map(dir_prefix).transpose()?;
        self.with_tx(|tx| {
            let n = match &prefix {
                None => tx.execute(
                    "UPDATE files SET missing = 1 WHERE folder_id = ?1 AND last_seen_scan <> ?2 AND missing = 0",
                    params![folder.0, scan.0],
                )?,
                Some((exact, prefix)) => tx.execute(
                    "UPDATE files SET missing = 1 WHERE folder_id = ?1 AND last_seen_scan <> ?2 AND missing = 0
                     AND (dir = ?3 OR substr(dir, 1, length(?4)) = ?4)",
                    params![folder.0, scan.0, exact, prefix],
                )?,
            };
            Ok(n)
        })
    }

    /// Files directly in `dir`, or anywhere beneath it if `recursive`, by path.
    pub fn files_in(&self, dir: &Path, recursive: bool) -> Result<Vec<FileRecord>> {
        let (exact, prefix) = dir_prefix(dir)?;
        let conn = self.conn.lock().expect("catalogue lock");
        let columns = "SELECT photo_id, path, size, modified_ns, missing FROM files";
        let map = |r: &rusqlite::Row<'_>| {
            Ok(FileRecord {
                photo: PhotoId(r.get(0)?),
                path: PathBuf::from(r.get::<_, String>(1)?),
                size: r.get::<_, i64>(2)? as u64,
                modified_ns: r.get(3)?,
                status: if r.get::<_, bool>(4)? {
                    FileStatus::Missing
                } else {
                    FileStatus::Present
                },
            })
        };
        let rows: rusqlite::Result<Vec<FileRecord>> = if recursive {
            let sql = format!(
                "{columns} WHERE dir = ?1 OR substr(dir, 1, length(?2)) = ?2 ORDER BY path"
            );
            conn.prepare(&sql)?
                .query_map(params![exact, prefix], map)?
                .collect()
        } else {
            conn.prepare(&format!("{columns} WHERE dir = ?1 ORDER BY path"))?
                .query_map([exact], map)?
                .collect()
        };
        Ok(rows?)
    }

    pub fn photo_count(&self) -> Result<i64> {
        let conn = self.conn.lock().expect("catalogue lock");
        Ok(conn.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))?)
    }
}

fn record(
    tx: &Transaction<'_>,
    folder: FolderId,
    id: &SourceIdentity,
    scan: ScanId,
) -> Result<(PhotoId, RecordOutcome)> {
    let path = text(&id.canonical_path)?;
    let dir = text(id.canonical_path.parent().unwrap_or(Path::new("")))?;
    let size = id.size as i64;
    let modified = i64::try_from(id.modified_unix_ns).unwrap_or(i64::MAX);
    let fingerprint = id.fingerprint as i64; // stored as bits

    // 1. Known path.
    let existing: Option<(i64, i64, i64, i64, i64)> = tx
        .query_row(
            "SELECT id, photo_id, size, modified_ns, fingerprint FROM files WHERE path = ?1",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    if let Some((file_id, photo, old_size, old_modified, old_fp)) = existing {
        let unchanged = (old_size, old_modified, old_fp) == (size, modified, fingerprint);
        tx.execute(
            "UPDATE files SET folder_id = ?2, size = ?3, modified_ns = ?4, fingerprint = ?5, missing = 0, last_seen_scan = ?6
             WHERE id = ?1",
            params![file_id, folder.0, size, modified, fingerprint, scan.0],
        )?;
        let outcome = if unchanged {
            RecordOutcome::Unchanged
        } else {
            // New content: its details must be read again.
            tx.execute(
                "UPDATE photos SET metadata_version = 0 WHERE id = ?1",
                [photo],
            )?;
            RecordOutcome::Changed
        };
        return Ok((PhotoId(photo), outcome));
    }

    // 2. Same content at a path that no longer exists: moved or renamed.
    //
    // A file already recorded in this scan exists, so it cannot be where this one moved
    // from. Scan ids only increase, so "not seen in this scan" is `last_seen_scan <
    // current`: a range on the (size, fingerprint, last_seen_scan) index. That keeps
    // libraries with many identical copies linear. Missing files are the likeliest
    // sources, so they come first, and the number of on-disk checks is bounded.
    let mut stmt = tx.prepare_cached(
        "SELECT id, photo_id, path FROM files
         WHERE size = ?1 AND fingerprint = ?2 AND last_seen_scan < ?3
         ORDER BY missing DESC LIMIT ?4",
    )?;
    let candidates: Vec<(i64, i64, String)> = stmt
        .query_map(
            params![size, fingerprint, scan.0, MAX_MOVE_CANDIDATES],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
        .collect::<rusqlite::Result<_>>()?;
    if let Some((file_id, photo, old_path)) = candidates
        .into_iter()
        .find(|(_, _, p)| !Path::new(p).exists())
    {
        tx.execute(
            "UPDATE files SET folder_id = ?2, path = ?3, dir = ?4, modified_ns = ?5, missing = 0, last_seen_scan = ?6
             WHERE id = ?1",
            params![file_id, folder.0, path, dir, modified, scan.0],
        )?;
        return Ok((
            PhotoId(photo),
            RecordOutcome::Moved {
                from: PathBuf::from(old_path),
            },
        ));
    }

    // 3. New photo.
    tx.execute("INSERT INTO photos (created_at_ms) VALUES (?1)", [now_ms()])?;
    let photo = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO files (photo_id, folder_id, path, dir, size, modified_ns, fingerprint, last_seen_scan)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![photo, folder.0, path, dir, size, modified, fingerprint, scan.0],
    )?;
    Ok((PhotoId(photo), RecordOutcome::New))
}

pub(crate) fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| CatalogueError::NonUnicodePath(path.to_path_buf()))
}

/// `dir` itself and the prefix every descendant directory starts with.
pub(crate) fn dir_prefix(dir: &Path) -> Result<(String, String)> {
    let exact = text(dir)?
        .trim_end_matches(std::path::MAIN_SEPARATOR)
        .to_owned();
    let prefix = format!("{exact}{}", std::path::MAIN_SEPARATOR);
    Ok((exact, prefix))
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests;
