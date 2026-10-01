//! Ratings and pick/reject flags: the photographer's own judgements (PRODUCT.md §17,
//! "application metadata"). Stored on the photo, so they follow it across moves and
//! renames, and never written into the original file.
//!
//! Unlike everything else in the catalogue, marks cannot be rebuilt from the files.
//! See ADR 0018.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::PhotoId;
use crate::catalogue::{Catalogue, Result};
use crate::details::PhotoDetails;

/// A star rating, 0 (unrated) to 5.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rating(u8);

impl Rating {
    pub const MAX: u8 = 5;

    /// `None` for values above 5.
    pub fn new(stars: u8) -> Option<Self> {
        (stars <= Self::MAX).then_some(Self(stars))
    }

    pub fn stars(self) -> u8 {
        self.0
    }
}

/// Pick/reject flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Flag {
    #[default]
    None,
    Pick,
    Reject,
}

impl Flag {
    fn to_db(self) -> i64 {
        match self {
            Self::Reject => -1,
            Self::None => 0,
            Self::Pick => 1,
        }
    }

    fn from_db(v: i64) -> Self {
        match v {
            1 => Self::Pick,
            -1 => Self::Reject,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub rating: Rating,
    pub flag: Flag,
}

impl Marks {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One change applied to a set of photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkChange {
    Rating(Rating),
    Flag(Flag),
}

/// Library-wide views: built from marks, or (Recently imported) from when photos
/// joined the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    Picks,
    /// One star or more.
    Rated,
    Rejected,
    /// First indexed within the last [`RECENT_DAYS`] days (ADR 0056).
    RecentlyImported,
}

/// How far back Recently imported reaches.
pub const RECENT_DAYS: i64 = 30;

/// Photos first indexed after this (ms since the epoch) are recently imported.
fn recent_cutoff_ms() -> i64 {
    crate::catalogue::now_ms() - RECENT_DAYS * 24 * 60 * 60 * 1000
}

impl Collection {
    fn condition(self) -> String {
        match self {
            Self::Picks => "p.flag = 1".into(),
            Self::Rated => "p.rating > 0".into(),
            Self::Rejected => "p.flag = -1".into(),
            Self::RecentlyImported => format!("p.created_at_ms >= {}", recent_cutoff_ms()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollectionCounts {
    pub picks: usize,
    pub rated: usize,
    pub rejected: usize,
    pub recent: usize,
}

/// A photo in a collection: its present file and what the catalogue knows about it.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionEntry {
    pub photo: PhotoId,
    pub path: PathBuf,
    pub size: u64,
    pub modified_ns: i64,
    pub marks: Marks,
    /// `None` until indexing has read the file's details.
    pub details: Option<PhotoDetails>,
    /// The photo has an edit recipe.
    pub edited: bool,
}

fn marks_from(rating: i64, flag: i64) -> Marks {
    Marks {
        rating: Rating::new(rating.clamp(0, 5) as u8).unwrap_or_default(),
        flag: Flag::from_db(flag),
    }
}

impl Catalogue {
    /// The photo whose present file is at `path` (canonical).
    pub fn photo_at(&self, path: &Path) -> Result<Option<PhotoId>> {
        let p = crate::catalogue::text(path)?;
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT photo_id FROM files WHERE path = ?1 AND missing = 0",
                [p],
                |r| r.get(0),
            )
            .optional()?
            .map(PhotoId))
    }

    /// Applies `change` to every photo in `photos`, in one transaction.
    pub fn set_marks(&self, photos: &[PhotoId], change: MarkChange) -> Result<()> {
        self.with_tx(|tx| {
            let (sql, value) = match change {
                MarkChange::Rating(r) => (
                    "UPDATE photos SET rating = ?2 WHERE id = ?1",
                    i64::from(r.stars()),
                ),
                MarkChange::Flag(f) => ("UPDATE photos SET flag = ?2 WHERE id = ?1", f.to_db()),
            };
            let mut stmt = tx.prepare_cached(sql)?;
            for photo in photos {
                stmt.execute(params![photo.0, value])?;
            }
            Ok(())
        })
    }

    pub fn marks(&self, photo: PhotoId) -> Result<Marks> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT rating, flag FROM photos WHERE id = ?1",
                [photo.0],
                |r| Ok(marks_from(r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Marks of the photos in `dir` (not recursive) that have any.
    pub fn marks_in_dir(&self, dir: &Path) -> Result<Vec<(PathBuf, Marks)>> {
        let dir = crate::catalogue::text(dir)?
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .to_owned();
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT f.path, p.rating, p.flag FROM files f JOIN photos p ON p.id = f.photo_id
             WHERE f.dir = ?1 AND f.missing = 0 AND (p.rating > 0 OR p.flag <> 0)",
        )?;
        let rows = stmt.query_map([dir], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                marks_from(r.get(1)?, r.get(2)?),
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Present photos in `collection` across the whole library, oldest capture first
    /// (then by path; photos without a capture time last).
    pub fn collection(&self, collection: Collection) -> Result<Vec<CollectionEntry>> {
        self.entries(&collection.condition(), [])
    }

    /// Present photos meeting `condition` (SQL over `p`, the photo, and `f`, its file),
    /// oldest capture first (then by path; photos without a capture time last).
    pub(crate) fn entries(
        &self,
        condition: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<CollectionEntry>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT p.id, f.path, f.size, f.modified_ns, p.rating, p.flag, p.metadata_version,
                    EXISTS(SELECT 1 FROM edits e WHERE e.photo_id = p.id), {}
             FROM photos p JOIN files f ON f.photo_id = p.id
             WHERE f.missing = 0 AND {condition}
             ORDER BY p.captured_at IS NULL, p.captured_at, f.path",
            crate::details::COLUMNS,
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params, |r| {
            let indexed: i64 = r.get(6)?;
            Ok(CollectionEntry {
                photo: PhotoId(r.get(0)?),
                path: PathBuf::from(r.get::<_, String>(1)?),
                size: r.get::<_, i64>(2)? as u64,
                modified_ns: r.get(3)?,
                marks: marks_from(r.get(4)?, r.get(5)?),
                details: if indexed > 0 {
                    Some(crate::details::from_row(r, 8)?)
                } else {
                    None
                },
                edited: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Sizes of the library-wide collections (present photos only).
    pub fn collection_counts(&self) -> Result<CollectionCounts> {
        let conn = self.conn();
        let (picks, rated, rejected) = conn.query_row(
            "SELECT
               COUNT(DISTINCT CASE WHEN p.flag = 1 THEN p.id END),
               COUNT(DISTINCT CASE WHEN p.rating > 0 THEN p.id END),
               COUNT(DISTINCT CASE WHEN p.flag = -1 THEN p.id END)
             FROM photos p JOIN files f ON f.photo_id = p.id
             WHERE f.missing = 0 AND (p.flag <> 0 OR p.rating > 0)",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )?;
        let recent: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT p.id) FROM photos p JOIN files f ON f.photo_id = p.id
             WHERE f.missing = 0 AND p.created_at_ms >= ?1",
            [recent_cutoff_ms()],
            |r| r.get(0),
        )?;
        Ok(CollectionCounts {
            picks: picks as usize,
            rated: rated as usize,
            rejected: rejected as usize,
            recent: recent as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_above_five_do_not_exist() {
        assert_eq!(Rating::new(5).map(Rating::stars), Some(5));
        assert_eq!(Rating::new(6), None);
        assert_eq!(Rating::default().stars(), 0);
    }

    #[test]
    fn flags_round_trip_through_the_database_encoding() {
        for f in [Flag::None, Flag::Pick, Flag::Reject] {
            assert_eq!(Flag::from_db(f.to_db()), f);
        }
        assert_eq!(Flag::from_db(7), Flag::None);
    }
}
