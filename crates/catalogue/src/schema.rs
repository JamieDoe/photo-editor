//! Schema and migrations, tracked with `PRAGMA user_version`.
//!
//! Migrations only ever append; a released migration is never edited. Each runs in a
//! transaction, so a crash mid-migration leaves the previous version intact.

use rusqlite::Connection;

use crate::CatalogueError;

/// Schema version this build creates and understands.
pub const SCHEMA_VERSION: i64 = 3;

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
    // 2: photo details read from file headers (camera, lens, capture time, exposure).
    r#"
    ALTER TABLE photos ADD COLUMN metadata_version INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE photos ADD COLUMN captured_at TEXT;       -- camera wall-clock, ISO 8601, no zone
    ALTER TABLE photos ADD COLUMN camera_make TEXT;
    ALTER TABLE photos ADD COLUMN camera_model TEXT;
    ALTER TABLE photos ADD COLUMN lens TEXT;
    ALTER TABLE photos ADD COLUMN iso INTEGER;
    ALTER TABLE photos ADD COLUMN aperture REAL;
    ALTER TABLE photos ADD COLUMN shutter REAL;           -- seconds
    ALTER TABLE photos ADD COLUMN focal_length REAL;      -- mm
    ALTER TABLE photos ADD COLUMN width INTEGER;          -- as displayed
    ALTER TABLE photos ADD COLUMN height INTEGER;
    ALTER TABLE photos ADD COLUMN rotation INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE photos ADD COLUMN latitude REAL;
    ALTER TABLE photos ADD COLUMN longitude REAL;
    CREATE INDEX photos_by_capture ON photos(captured_at);
    CREATE INDEX photos_by_camera ON photos(camera_model);
    "#,
    // 3: move detection looks up same-content files not yet seen in the current scan
    // (`last_seen_scan < current`); indexing that column makes it a range query, so
    // libraries with many identical copies stay linear.
    r#"
    DROP INDEX files_by_content;
    CREATE INDEX files_by_content ON files(size, fingerprint, last_seen_scan);
    "#,
];

/// Brings the database to [`SCHEMA_VERSION`].
pub fn migrate(conn: &mut Connection) -> Result<(), CatalogueError> {
    migrate_to(conn, SCHEMA_VERSION)
}

/// Applies migrations up to `target` (tests use this to build older schemas).
pub(crate) fn migrate_to(conn: &mut Connection, target: i64) -> Result<(), CatalogueError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(CatalogueError::TooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (i, sql) in MIGRATIONS
        .iter()
        .enumerate()
        .take(target as usize)
        .skip(current as usize)
    {
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
    fn upgrades_a_version_1_database_keeping_its_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_to(&mut conn, 1).unwrap();
        conn.execute("INSERT INTO photos (created_at_ms) VALUES (1)", [])
            .unwrap();
        migrate(&mut conn).unwrap();
        let (count, version): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), MAX(metadata_version) FROM photos",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (count, version),
            (1, 0),
            "existing photos kept, marked as needing details"
        );
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
