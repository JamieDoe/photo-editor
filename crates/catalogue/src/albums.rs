//! Albums (ADR 0055): the photographer's own groups of photos. An album holds
//! references to photos, never copies, so it follows a photo when its file is moved or
//! renamed, and a photo can be in any number of albums. Like marks, albums cannot be
//! rebuilt from the files; backups keep them.

use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};

use crate::PhotoId;
use crate::catalogue::{Catalogue, Result, now_ms};
use crate::marks::CollectionEntry;

/// An album's row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlbumId(pub i64);

/// An album and what the library shows of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Album {
    pub id: AlbumId,
    pub name: String,
    /// Its photos whose files are present.
    pub count: usize,
    /// The file of its first present photo (by capture time), for a cover.
    pub cover: Option<PathBuf>,
}

/// The album columns, with present photos counted and the cover found.
const SELECT: &str = "SELECT a.id, a.name,
        (SELECT COUNT(DISTINCT ap.photo_id) FROM album_photos ap
           JOIN files f ON f.photo_id = ap.photo_id AND f.missing = 0
          WHERE ap.album_id = a.id),
        (SELECT f.path FROM album_photos ap
           JOIN photos p ON p.id = ap.photo_id
           JOIN files f ON f.photo_id = p.id AND f.missing = 0
          WHERE ap.album_id = a.id
          ORDER BY p.captured_at IS NULL, p.captured_at, f.path LIMIT 1)
     FROM albums a";

impl Catalogue {
    /// Every album, by name (ignoring case).
    pub fn albums(&self) -> Result<Vec<Album>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY a.name COLLATE NOCASE, a.id"))?;
        let rows = stmt.query_map([], row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn album(&self, id: AlbumId) -> Result<Option<Album>> {
        let conn = self.conn();
        Ok(conn
            .query_row(&format!("{SELECT} WHERE a.id = ?1"), [id.0], row)
            .optional()?)
    }

    /// A new, empty album.
    pub fn add_album(&self, name: &str) -> Result<AlbumId> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO albums (name, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?2)",
            params![name, now_ms()],
        )?;
        Ok(AlbumId(conn.last_insert_rowid()))
    }

    /// Renames an album; false if there is no such album.
    pub fn rename_album(&self, id: AlbumId, name: &str) -> Result<bool> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE albums SET name = ?2, updated_at_ms = ?3 WHERE id = ?1",
            params![id.0, name, now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Deletes an album (its photos are untouched); false if there was no such album.
    pub fn delete_album(&self, id: AlbumId) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute("DELETE FROM albums WHERE id = ?1", [id.0])? > 0)
    }

    /// Adds `photos` to an album, in one transaction; returns how many were not in it
    /// already.
    pub fn add_to_album(&self, id: AlbumId, photos: &[PhotoId]) -> Result<usize> {
        self.with_tx(|tx| {
            let now = now_ms();
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO album_photos (album_id, photo_id, added_at_ms)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM albums WHERE id = ?1)",
            )?;
            let mut added = 0;
            for photo in photos {
                added += stmt.execute(params![id.0, photo.0, now])?;
            }
            if added > 0 {
                tx.execute(
                    "UPDATE albums SET updated_at_ms = ?2 WHERE id = ?1",
                    params![id.0, now],
                )?;
            }
            Ok(added)
        })
    }

    /// Takes `photos` out of an album (the photos themselves are untouched); returns
    /// how many were in it.
    pub fn remove_from_album(&self, id: AlbumId, photos: &[PhotoId]) -> Result<usize> {
        self.with_tx(|tx| {
            let mut stmt = tx
                .prepare_cached("DELETE FROM album_photos WHERE album_id = ?1 AND photo_id = ?2")?;
            let mut removed = 0;
            for photo in photos {
                removed += stmt.execute(params![id.0, photo.0])?;
            }
            Ok(removed)
        })
    }

    /// An album's present photos, oldest capture first.
    pub fn album_photos(&self, id: AlbumId) -> Result<Vec<CollectionEntry>> {
        self.entries(
            "p.id IN (SELECT photo_id FROM album_photos WHERE album_id = ?1)",
            [id.0],
        )
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Album> {
    Ok(Album {
        id: AlbumId(r.get(0)?),
        name: r.get(1)?,
        count: r.get::<_, i64>(2)? as usize,
        cover: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
    })
}
