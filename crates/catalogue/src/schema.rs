//! Schema and migrations, tracked with `PRAGMA user_version`.
//!
//! Migrations only ever append; a released migration is never edited. Each runs in a
//! transaction, so a crash mid-migration leaves the previous version intact.

use rusqlite::Connection;

use crate::CatalogueError;

/// Schema version this build creates and understands.
pub const SCHEMA_VERSION: i64 = 1;

const MIGRATIONS: &[&str] = &[
    // 1: library folders, photos, files.
    r#"
    CREATE TABLE folders (
        id          INTEGER PRIMARY KEY,
        path        TEXT NOT NULL UNIQUE,     -- canonical path of a library root
        added_at_ms INTEGER NOT NULL
    );

    -- A photograph. Ratings, flags, metadata and edits attach here (later migrations),
    -- not to the file, so they survive moves and renames.
    CREATE TABLE photos (
        id            INTEGER PRIMARY KEY,
        created_at_ms INTEGER NOT NULL
    );

    -- The file currently holding a photo.
    CREATE TABLE files (
        id            INTEGER PRIMARY KEY,
        photo_id      INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
        folder_id     INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
        path          TEXT NOT NULL UNIQUE,   -- canonical path
        dir           TEXT NOT NULL,          -- canonical parent directory
        size          INTEGER NOT NULL,
        modified_ns   INTEGER NOT NULL,
        fingerprint   INTEGER NOT NULL,       -- FNV-1a of size + head + tail (u64 bits)
        missing       INTEGER NOT NULL DEFAULT 0,
        last_seen_scan INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX files_by_content ON files(size, fingerprint);
    CREATE INDEX files_by_folder ON files(folder_id, last_seen_scan);
    CREATE INDEX files_by_photo ON files(photo_id);
    CREATE INDEX files_by_dir ON files(dir);
    "#,
];

/// Brings the database to [`SCHEMA_VERSION`].
pub fn migrate(conn: &mut Connection) -> Result<(), CatalogueError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(CatalogueError::TooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i as i64 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        log::info!("catalogue migrated to schema {version}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_match_schema_version() {
        assert_eq!(MIGRATIONS.len() as i64, SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn refuses_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            migrate(&mut conn),
            Err(CatalogueError::TooNew { .. })
        ));
    }
}
