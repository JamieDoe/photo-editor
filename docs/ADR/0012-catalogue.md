# ADR 0012: SQLite catalogue with photo/file separation and content-based identity

- Status: Accepted (Phase 2, milestone 1)
- Date: 2026-09-28

## Context

Phase 2 builds the catalogue (PRODUCT.md §31). CLAUDE.md §11–12 require:
- SQLite for application state, never for photographs;
- a database that is rebuildable from the filesystem;
- file identity that does not rely on the path alone, so a moved photo can be
  recognised.

## Decision

1. **`crates/catalogue`**, using `rusqlite` with SQLite compiled in (`bundled`):
   - no system SQLite dependency on any platform;
   - SQLite is public domain and rusqlite is MIT; adds ~1 MB.
2. **Photos are separate from files.**
   - A `photos` row is the photograph. Ratings, flags, metadata and edits will attach
     to it in later migrations.
   - A `files` row is where it currently lives: canonical path, parent dir, size,
     modification time, content fingerprint.
3. **Identity** (`SourceIdentity`, moved here from `app-core`). The fingerprint is
   FNV-1a over size + first 64 KB + last 64 KB. Recording a file resolves, in order:
   - same path, same size/mtime/fingerprint → **Unchanged**;
   - same path, different content → **Changed** (same photo, e.g. re-saved);
   - same size + fingerprint at another path that no longer exists → **Moved** (the
     photo follows the file);
   - otherwise → **New**. A copy whose original still exists is a new photo; duplicate
     detection is a later feature.
4. **Missing, not deleted.** Each indexing pass has a scan id. Files in the scanned
   folder (or subtree) that weren't seen are marked `missing`, so ratings and edits
   survive a disconnected drive. They become present again when seen.
5. **Robustness:**
   - WAL journal, `synchronous=NORMAL`, foreign keys on.
   - `PRAGMA quick_check` on open. A damaged file returns `CatalogueError::Corrupt`;
     the app moves it aside and re-indexes (the catalogue is rebuildable).
   - Migrations are append-only, versioned with `PRAGMA user_version`, one
     transaction each. A newer schema is refused rather than misread.
6. **Threading:** one connection behind a mutex. Every call blocks, so it runs on
   background jobs, never the UI thread. Batches use one transaction.

## Measurements

5,000 files, release build, in-memory database: first index 57 ms, rescan 40 ms
(database work only; filesystem walking and fingerprinting are measured in
milestone 2).

## Consequences

- Moves and renames keep a photo's future ratings and edits, as long as the file is
  re-found within a library folder.
- A fingerprint collision (same size and identical first and last 64 KB) would
  wrongly link two files. That is very unlikely for camera files, whose headers
  include timestamps. A full-content hash can be added for duplicate detection.
- Non-Unicode paths cannot be stored (SQLite text is UTF-8). They are reported and
  skipped.
