//! The photographer's presets (ADR 0046): named edit recipe templates. Like edits, a
//! preset's recipe is opaque JSON plus its schema version; what it holds and how it
//! applies belong to the renderer and the editor.

use rusqlite::{OptionalExtension, params};

use crate::catalogue::{Catalogue, Result, now_ms};

/// A stored preset's row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PresetId(pub i64);

/// A stored preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPreset {
    pub id: PresetId,
    pub name: String,
    /// Schema version the recipe was written with.
    pub recipe_version: u32,
    pub json: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Catalogue {
    /// Every stored preset, oldest first.
    pub fn presets(&self) -> Result<Vec<StoredPreset>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, recipe_version, recipe, created_at_ms, updated_at_ms
             FROM presets ORDER BY created_at_ms, id",
        )?;
        let rows = stmt.query_map([], row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn preset(&self, id: PresetId) -> Result<Option<StoredPreset>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT id, name, recipe_version, recipe, created_at_ms, updated_at_ms
                 FROM presets WHERE id = ?1",
                [id.0],
                row,
            )
            .optional()?)
    }

    /// Stores a new preset.
    pub fn add_preset(&self, name: &str, recipe_version: u32, json: &str) -> Result<PresetId> {
        let conn = self.conn();
        let now = now_ms();
        conn.execute(
            "INSERT INTO presets (name, recipe_version, recipe, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![name, i64::from(recipe_version), json, now],
        )?;
        Ok(PresetId(conn.last_insert_rowid()))
    }

    /// Renames a preset; false if there is no such preset.
    pub fn rename_preset(&self, id: PresetId, name: &str) -> Result<bool> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE presets SET name = ?2, updated_at_ms = ?3 WHERE id = ?1",
            params![id.0, name, now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Replaces a preset's recipe; false if there is no such preset.
    pub fn set_preset_recipe(&self, id: PresetId, recipe_version: u32, json: &str) -> Result<bool> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE presets SET recipe_version = ?2, recipe = ?3, updated_at_ms = ?4 WHERE id = ?1",
            params![id.0, i64::from(recipe_version), json, now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Deletes a preset; false if there was no such preset. Photos edited with it keep
    /// their edits: applying a preset copies its settings.
    pub fn delete_preset(&self, id: PresetId) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute("DELETE FROM presets WHERE id = ?1", [id.0])? > 0)
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredPreset> {
    Ok(StoredPreset {
        id: PresetId(r.get(0)?),
        name: r.get(1)?,
        recipe_version: r.get::<_, i64>(2)? as u32,
        json: r.get(3)?,
        created_at_ms: r.get(4)?,
        updated_at_ms: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_added_listed_changed_and_deleted() {
        let cat = Catalogue::open_in_memory().unwrap();
        assert!(cat.presets().unwrap().is_empty());
        let a = cat.add_preset("Soft", 20, r#"{"version":20}"#).unwrap();
        let b = cat
            .add_preset("Punchy", 20, r#"{"version":20,"contrast":30.0}"#)
            .unwrap();
        let names = |c: &Catalogue| {
            c.presets()
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&cat), ["Soft", "Punchy"]);

        assert!(cat.rename_preset(a, "Softer").unwrap());
        assert!(cat.set_preset_recipe(b, 21, r#"{"version":21}"#).unwrap());
        let got = cat.preset(b).unwrap().unwrap();
        assert_eq!(
            (got.recipe_version, got.json.as_str()),
            (21, r#"{"version":21}"#)
        );
        assert_eq!(names(&cat), ["Softer", "Punchy"]);

        assert!(cat.delete_preset(a).unwrap());
        assert!(!cat.delete_preset(a).unwrap());
        assert!(!cat.rename_preset(a, "Gone").unwrap());
        assert!(cat.preset(a).unwrap().is_none());
        assert_eq!(names(&cat), ["Punchy"]);
    }
}
