//! Keywords (ADR 0088): words the photographer attaches to photos to find them again
//! ("heron", "Iceland 2026", "client: Smith"). Stored once each and linked to photos,
//! so they follow a photo across moves and renames, like marks and albums; like them,
//! they can't be rebuilt from the files, and backups keep them.
//!
//! A keyword's name is unique ignoring ASCII case: adding "heron" to a photo when
//! "Heron" exists links the existing one, so the library never holds both.

use std::collections::HashMap;

use rusqlite::{Transaction, params, params_from_iter};

use crate::PhotoId;
use crate::catalogue::{Catalogue, Result};

/// The longest keyword kept, in characters.
pub const MAX_KEYWORD_CHARS: usize = 100;

/// Photos looked up per statement, well under SQLite's limit on parameters.
const CHUNK: usize = 500;

/// A keyword and how many photos have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordUse {
    pub name: String,
    pub photos: usize,
}

/// `text` as a keyword: trimmed, with runs of whitespace (and control characters) made
/// one space; `None` when that leaves nothing or more than [`MAX_KEYWORD_CHARS`].
pub fn keyword_name(text: &str) -> Option<String> {
    let name = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_KEYWORD_CHARS).then_some(name)
}

/// The valid names among `names`, each once (ignoring ASCII case), in order.
fn valid_names(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names.iter().filter_map(|n| keyword_name(n)) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out
}

/// The id of keyword `name`, added if it is new.
fn keyword_id(tx: &Transaction<'_>, name: &str) -> rusqlite::Result<i64> {
    tx.prepare_cached("INSERT INTO keywords (name) VALUES (?1) ON CONFLICT(name) DO NOTHING")?
        .execute([name])?;
    tx.prepare_cached("SELECT id FROM keywords WHERE name = ?1")?
        .query_row([name], |r| r.get(0))
}

/// Links `photo` to each of `names` (valid, distinct); how many links were new.
fn link(tx: &Transaction<'_>, photo: PhotoId, names: &[String]) -> rusqlite::Result<usize> {
    let mut added = 0;
    for name in names {
        let id = keyword_id(tx, name)?;
        added += tx
            .prepare_cached(
                "INSERT OR IGNORE INTO photo_keywords (photo_id, keyword_id) VALUES (?1, ?2)",
            )?
            .execute(params![photo.0, id])?;
    }
    Ok(added)
}

impl Catalogue {
    /// Every keyword some photo has, by name (ignoring case), with how many photos.
    pub fn keywords(&self) -> Result<Vec<KeywordUse>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT k.name, COUNT(*) FROM keywords k JOIN photo_keywords pk ON pk.keyword_id = k.id
             GROUP BY k.id ORDER BY k.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(KeywordUse {
                name: r.get(0)?,
                photos: r.get::<_, i64>(1)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The keywords of `photo`, by name (ignoring case).
    pub fn photo_keywords(&self, photo: PhotoId) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT k.name FROM keywords k JOIN photo_keywords pk ON pk.keyword_id = k.id
             WHERE pk.photo_id = ?1 ORDER BY k.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([photo.0], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The keywords any of `photos` has, by name (ignoring case), each with how many of
    /// them have it.
    pub fn keywords_on(&self, photos: &[PhotoId]) -> Result<Vec<KeywordUse>> {
        let conn = self.conn();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for chunk in photos.chunks(CHUNK) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let mut stmt = conn.prepare(&format!(
                "SELECT k.name, COUNT(DISTINCT pk.photo_id) FROM keywords k
                 JOIN photo_keywords pk ON pk.keyword_id = k.id
                 WHERE pk.photo_id IN ({marks}) GROUP BY k.id"
            ))?;
            let rows = stmt.query_map(params_from_iter(chunk.iter().map(|p| p.0)), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
            })?;
            for row in rows {
                let (name, n) = row?;
                *counts.entry(name).or_default() += n;
            }
        }
        let mut uses: Vec<KeywordUse> = counts
            .into_iter()
            .map(|(name, photos)| KeywordUse { name, photos })
            .collect();
        uses.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(uses)
    }

    /// Adds each of `names` (made keywords with [`keyword_name`]; invalid ones are
    /// skipped) to every photo in `photos`, in one transaction. An existing keyword
    /// keeps its spelling.
    pub fn add_keywords(&self, photos: &[PhotoId], names: &[String]) -> Result<()> {
        let names = valid_names(names);
        if names.is_empty() {
            return Ok(());
        }
        self.with_tx(|tx| {
            for photo in photos {
                link(tx, *photo, &names)?;
            }
            Ok(())
        })
    }

    /// Takes each of `names` (ignoring case) off every photo in `photos`, in one
    /// transaction; keywords no photo has any more are forgotten.
    pub fn remove_keywords(&self, photos: &[PhotoId], names: &[String]) -> Result<()> {
        let names = valid_names(names);
        if names.is_empty() {
            return Ok(());
        }
        self.with_tx(|tx| {
            let mut stmt = tx.prepare_cached(
                "DELETE FROM photo_keywords WHERE photo_id = ?1
                   AND keyword_id = (SELECT id FROM keywords WHERE name = ?2)",
            )?;
            for photo in photos {
                for name in &names {
                    stmt.execute(params![photo.0, name])?;
                }
            }
            tx.execute(
                "DELETE FROM keywords WHERE id NOT IN (SELECT keyword_id FROM photo_keywords)",
                [],
            )?;
            Ok(())
        })
    }

    /// Gives photos that have no keywords yet the keywords found for them (in other
    /// apps' sidecars, ADR 0088); photos with their own keep them. How many photos took
    /// keywords.
    pub fn import_keywords(&self, items: &[(PhotoId, Vec<String>)]) -> Result<usize> {
        self.with_tx(|tx| {
            let mut taken = 0;
            for (photo, names) in items {
                let has: bool = tx
                    .prepare_cached(
                        "SELECT EXISTS(SELECT 1 FROM photo_keywords WHERE photo_id = ?1)",
                    )?
                    .query_row([photo.0], |r| r.get(0))?;
                if !has && link(tx, *photo, &valid_names(names))? > 0 {
                    taken += 1;
                }
            }
            Ok(taken)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_names_are_tidied_and_bounded() {
        assert_eq!(
            keyword_name("  grey   heron\t").as_deref(),
            Some("grey heron")
        );
        assert_eq!(keyword_name("line\nbreak").as_deref(), Some("line break"));
        assert_eq!(keyword_name("Café").as_deref(), Some("Café"));
        assert_eq!(keyword_name("   "), None);
        assert_eq!(
            keyword_name(&"x".repeat(MAX_KEYWORD_CHARS)).map(|k| k.len()),
            Some(100)
        );
        assert_eq!(keyword_name(&"x".repeat(MAX_KEYWORD_CHARS + 1)), None);
    }

    #[test]
    fn names_are_kept_once_ignoring_case() {
        let names = ["Heron", " heron ", "Lake", "", "HERON"].map(String::from);
        assert_eq!(valid_names(&names), vec!["Heron", "Lake"]);
    }
}
