//! Smart collections (ADR 0084): named rules that a photo must all match, kept in the
//! catalogue and looked up each time, so they fill themselves as photos are rated,
//! flagged, labelled, edited or added.
//!
//! A rule left out matches every photo. The words are those of the search box (ADR
//! 0056): folders (places), cameras, lenses and capture dates.

use rusqlite::{OptionalExtension, params};

use crate::catalogue::{Catalogue, Result, now_ms};
use crate::marks::{CollectionEntry, ColourLabel, Rating};
use crate::search;

/// A smart collection's row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SmartCollectionId(pub i64);

/// Which ratings match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingRule {
    /// No stars.
    Unrated,
    /// At least this many stars (one to five).
    AtLeast(Rating),
}

/// Which flags match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagRule {
    Picked,
    Rejected,
    Unflagged,
    /// Picked or unflagged.
    NotRejected,
}

/// What a photo must match; `None` (or no words) matches every photo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartRules {
    pub rating: Option<RatingRule>,
    pub flag: Option<FlagRule>,
    /// A colour label, or [`ColourLabel::None`] for photos without one.
    pub label: Option<ColourLabel>,
    /// Edited (true) or not (false).
    pub edited: Option<bool>,
    /// Words as the search box takes them; empty for none.
    pub words: String,
}

/// A smart collection and how many present photos match it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartCollection {
    pub id: SmartCollectionId,
    pub name: String,
    pub rules: SmartRules,
    pub count: usize,
}

impl RatingRule {
    fn to_db(self) -> i64 {
        match self {
            Self::Unrated => 0,
            Self::AtLeast(r) => i64::from(r.stars().max(1)),
        }
    }

    fn from_db(v: i64) -> Self {
        match Rating::new(v.clamp(0, 5) as u8).filter(|r| r.stars() > 0) {
            Some(r) => Self::AtLeast(r),
            None => Self::Unrated,
        }
    }
}

impl FlagRule {
    fn to_db(self) -> i64 {
        match self {
            Self::Picked => 1,
            Self::Rejected => -1,
            Self::Unflagged => 0,
            Self::NotRejected => 2,
        }
    }

    fn from_db(v: i64) -> Self {
        match v {
            1 => Self::Picked,
            -1 => Self::Rejected,
            2 => Self::NotRejected,
            _ => Self::Unflagged,
        }
    }
}

impl SmartRules {
    /// No rules: every photo matches.
    pub fn is_empty(&self) -> bool {
        self.rating.is_none()
            && self.flag.is_none()
            && self.label.is_none()
            && self.edited.is_none()
            && search::terms(&self.words).is_empty()
    }

    /// The SQL condition (over `p` and `f`) and its parameters, for the words.
    fn condition(&self) -> (String, Vec<String>) {
        let mut parts = vec!["1 = 1".to_owned()];
        match self.rating {
            Some(RatingRule::Unrated) => parts.push("p.rating = 0".into()),
            Some(RatingRule::AtLeast(r)) => parts.push(format!("p.rating >= {}", r.stars())),
            None => {}
        }
        // Flags are stored as 1 (picked), -1 (rejected) and 0.
        match self.flag {
            Some(FlagRule::Picked) => parts.push("p.flag = 1".into()),
            Some(FlagRule::Rejected) => parts.push("p.flag = -1".into()),
            Some(FlagRule::Unflagged) => parts.push("p.flag = 0".into()),
            Some(FlagRule::NotRejected) => parts.push("p.flag <> -1".into()),
            None => {}
        }
        if let Some(label) = self.label {
            parts.push(format!("p.label = {}", label.to_db()));
        }
        match self.edited {
            Some(true) => {
                parts.push("EXISTS(SELECT 1 FROM edits e WHERE e.photo_id = p.id)".into())
            }
            Some(false) => {
                parts.push("NOT EXISTS(SELECT 1 FROM edits e WHERE e.photo_id = p.id)".into())
            }
            None => {}
        }
        let terms = search::terms(&self.words);
        let mut params = Vec::new();
        if !terms.is_empty() {
            let (words, p) = search::condition(&terms);
            parts.push(words);
            params = p;
        }
        (parts.join(" AND "), params)
    }
}

const SELECT: &str = "SELECT id, name, rating, flag, label, edited, words FROM smart_collections";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(SmartCollectionId, String, SmartRules)> {
    Ok((
        SmartCollectionId(r.get(0)?),
        r.get(1)?,
        SmartRules {
            rating: r.get::<_, Option<i64>>(2)?.map(RatingRule::from_db),
            flag: r.get::<_, Option<i64>>(3)?.map(FlagRule::from_db),
            label: r.get::<_, Option<i64>>(4)?.map(ColourLabel::from_db),
            edited: r.get::<_, Option<i64>>(5)?.map(|v| v != 0),
            words: r.get(6)?,
        },
    ))
}

impl Catalogue {
    /// Every smart collection, by name (ignoring case), with its count.
    pub fn smart_collections(&self) -> Result<Vec<SmartCollection>> {
        let stored = {
            let conn = self.conn();
            let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY name COLLATE NOCASE, id"))?;
            let rows = stmt.query_map([], row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        stored
            .into_iter()
            .map(|(id, name, rules)| {
                Ok(SmartCollection {
                    count: self.count_matching(&rules)?,
                    id,
                    name,
                    rules,
                })
            })
            .collect()
    }

    pub fn smart_collection(&self, id: SmartCollectionId) -> Result<Option<SmartCollection>> {
        let stored = {
            let conn = self.conn();
            conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id.0], row)
                .optional()?
        };
        stored
            .map(|(id, name, rules)| {
                Ok(SmartCollection {
                    count: self.count_matching(&rules)?,
                    id,
                    name,
                    rules,
                })
            })
            .transpose()
    }

    pub fn add_smart_collection(
        &self,
        name: &str,
        rules: &SmartRules,
    ) -> Result<SmartCollectionId> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO smart_collections
               (name, rating, flag, label, edited, words, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                name,
                rules.rating.map(RatingRule::to_db),
                rules.flag.map(FlagRule::to_db),
                rules.label.map(ColourLabel::to_db),
                rules.edited.map(i64::from),
                rules.words.trim(),
                now_ms()
            ],
        )?;
        Ok(SmartCollectionId(conn.last_insert_rowid()))
    }

    /// Renames a smart collection and replaces its rules; false if there is none.
    pub fn update_smart_collection(
        &self,
        id: SmartCollectionId,
        name: &str,
        rules: &SmartRules,
    ) -> Result<bool> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE smart_collections
                SET name = ?2, rating = ?3, flag = ?4, label = ?5, edited = ?6, words = ?7,
                    updated_at_ms = ?8
              WHERE id = ?1",
            params![
                id.0,
                name,
                rules.rating.map(RatingRule::to_db),
                rules.flag.map(FlagRule::to_db),
                rules.label.map(ColourLabel::to_db),
                rules.edited.map(i64::from),
                rules.words.trim(),
                now_ms()
            ],
        )?;
        Ok(n > 0)
    }

    /// Deletes a smart collection (its photos are untouched); false if there was none.
    pub fn delete_smart_collection(&self, id: SmartCollectionId) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute("DELETE FROM smart_collections WHERE id = ?1", [id.0])? > 0)
    }

    /// The present photos matching `rules`, oldest capture first.
    pub fn matching(&self, rules: &SmartRules) -> Result<Vec<CollectionEntry>> {
        let (condition, params) = rules.condition();
        self.entries(&condition, rusqlite::params_from_iter(params))
    }

    /// How many present photos match `rules`.
    pub fn count_matching(&self, rules: &SmartRules) -> Result<usize> {
        let (condition, params) = rules.condition();
        let conn = self.conn();
        let n: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT p.id) FROM photos p JOIN files f ON f.photo_id = p.id
                 WHERE f.missing = 0 AND {condition}"
            ),
            rusqlite::params_from_iter(params),
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_round_trip_through_the_database_encoding() {
        for r in 1..=5 {
            let rule = RatingRule::AtLeast(Rating::new(r).unwrap());
            assert_eq!(RatingRule::from_db(rule.to_db()), rule);
        }
        assert_eq!(RatingRule::from_db(0), RatingRule::Unrated);
        for f in [
            FlagRule::Picked,
            FlagRule::Rejected,
            FlagRule::Unflagged,
            FlagRule::NotRejected,
        ] {
            assert_eq!(FlagRule::from_db(f.to_db()), f);
        }
    }

    #[test]
    fn no_rules_is_empty_and_words_of_spaces_are_none() {
        assert!(SmartRules::default().is_empty());
        assert!(
            SmartRules {
                words: "   ".into(),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !SmartRules {
                edited: Some(false),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
