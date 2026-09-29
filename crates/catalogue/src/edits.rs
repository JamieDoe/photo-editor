//! Edit recipes (PRODUCT.md §4.1): the parameters that reconstruct an edited photo from
//! its untouched original. Stored per photo, so they follow moves and renames.
//!
//! The catalogue treats a recipe as opaque JSON plus its schema version. Parsing,
//! migration and rendering belong to the renderer (ADR 0019).

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::PhotoId;
use crate::catalogue::{Catalogue, Result, dir_prefix, now_ms, text};

/// A stored edit recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEdit {
    /// Schema version the recipe was written with.
    pub recipe_version: u32,
    pub json: String,
    pub updated_at_ms: i64,
}

impl Catalogue {
    pub fn edit_of(&self, photo: PhotoId) -> Result<Option<StoredEdit>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT recipe_version, recipe, updated_at_ms FROM edits WHERE photo_id = ?1",
                [photo.0],
                |r| {
                    Ok(StoredEdit {
                        recipe_version: r.get::<_, i64>(0)? as u32,
                        json: r.get(1)?,
                        updated_at_ms: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Stores `photo`'s recipe (`version` and `json`), or removes it with `None` (the
    /// photo is back to its original look).
    pub fn set_edit(&self, photo: PhotoId, recipe: Option<(u32, &str)>) -> Result<()> {
        let conn = self.conn();
        match recipe {
            Some((version, json)) => conn.execute(
                "INSERT INTO edits (photo_id, recipe_version, recipe, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(photo_id) DO UPDATE SET
                   recipe_version = excluded.recipe_version,
                   recipe = excluded.recipe,
                   updated_at_ms = excluded.updated_at_ms",
                params![photo.0, i64::from(version), json, now_ms()],
            )?,
            None => conn.execute("DELETE FROM edits WHERE photo_id = ?1", [photo.0])?,
        };
        Ok(())
    }

    /// Present files directly in `dir` (or anywhere beneath it if `recursive`) whose
    /// photo has an edit.
    pub fn edits_in(&self, dir: &Path, recursive: bool) -> Result<Vec<(PathBuf, StoredEdit)>> {
        let (exact, prefix) = dir_prefix(dir)?;
        let conn = self.conn();
        let base = "SELECT f.path, e.recipe_version, e.recipe, e.updated_at_ms
                    FROM edits e JOIN files f ON f.photo_id = e.photo_id
                    WHERE f.missing = 0 AND ";
        let map = |r: &rusqlite::Row<'_>| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                StoredEdit {
                    recipe_version: r.get::<_, i64>(1)? as u32,
                    json: r.get(2)?,
                    updated_at_ms: r.get(3)?,
                },
            ))
        };
        let rows: rusqlite::Result<Vec<_>> = if recursive {
            conn.prepare(&format!(
                "{base}(f.dir = ?1 OR substr(f.dir, 1, length(?2)) = ?2)"
            ))?
            .query_map(params![exact, prefix], map)?
            .collect()
        } else {
            conn.prepare(&format!("{base}f.dir = ?1"))?
                .query_map([exact], map)?
                .collect()
        };
        Ok(rows?)
    }

    /// The edit of the photo whose present file is at `path` (canonical), if any.
    pub fn edit_at(&self, path: &Path) -> Result<Option<StoredEdit>> {
        let p = text(path)?;
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT e.recipe_version, e.recipe, e.updated_at_ms
                 FROM edits e JOIN files f ON f.photo_id = e.photo_id
                 WHERE f.path = ?1 AND f.missing = 0",
                [p],
                |r| {
                    Ok(StoredEdit {
                        recipe_version: r.get::<_, i64>(0)? as u32,
                        json: r.get(1)?,
                        updated_at_ms: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }
}
