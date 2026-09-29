# ADR 0021: Automatic, verified, bounded library backups

- Status: Accepted (Phase 2, milestone 8)
- Date: 2026-09-29

## Context

The catalogue started as a rebuildable index (ADR 0012). Ratings, flags (ADR 0018) and
edits (ADR 0019) changed that: they exist only in the catalogue. Before this change,
a damaged catalogue was moved aside and rebuilt empty, which would have lost all of
them. Everything must stay local (PRODUCT.md §2.6).

## Decision

1. **Snapshots** are made with SQLite's `VACUUM INTO` on a separate *read-only*
   connection. Through WAL that gives a consistent, compacted copy that includes
   uncheckpointed writes, without holding the app's connection.
   - Each copy is written to a temporary name, checked with `quick_check`, then
     renamed. A copy that fails the check is discarded.
   - Files are named `catalogue-<unix ms>-<kind>.sqlite`
     (`auto` / `manual` / `pre-upgrade-v<N>`), so listing needs no database access.
2. **When:**
   - About 30 s after start-up, if the newest backup is older than 12 h (or there is
     none).
   - Then at most hourly while the library changes (`Catalogue::change_count`).
   - Before any schema upgrade: `open_catalogue` reads the file's version first, and
     backs up before migrating an existing catalogue.
   - On demand ("Back up now" in Settings).
   - Backups never overlap. Self-test runs (in-memory library) take none.
3. **Retention** (`to_prune`, pure and tested):
   - the newest 3;
   - the newest of each of the last 7 calendar days (UTC);
   - the newest of each of the 4 weeks before that;
   - the newest 2 pre-upgrade backups.
   - That's at most 16 files, so the folder cannot grow without bound. At ~11 MB per
     10,000 photos, that is under 200 MB for a large library.
4. **Recovery.** If the catalogue fails its integrity check at start-up:
   - It is moved aside (kept), and the newest backup that passes `quick_check` is
     restored.
   - The Library shows a notice saying when that backup was taken and what was lost.
   - The catalogue is rebuilt empty only when no good backup exists.
5. **Location.** Backups live in the app-data folder (`…/backups`), and Settings can
   open it. They are on the same disk, which protects against damage but not against
   losing the disk. Copying backups to a folder of the user's choosing (for example an
   external drive) is deferred until wanted.

## Consequences

- Ratings, flags and edits survive catalogue damage and bad upgrades, losing at most
  about an hour of changes.
- Restoring is automatic, which is simpler and safer than asking at start-up, before
  the UI exists. The damaged file is always kept.
- XMP sidecars (industry-standard, readable by other apps) remain an option for
  portability, separate from backups.
