# ADR 0013: Background library indexing

- Status: Accepted (Phase 2, milestone 2)
- Date: 2026-09-28

## Context

PRODUCT.md workflow D: choose a folder, and the application indexes it while the UI
stays responsive. Rescans must be cheap, because they happen every time a folder is
opened or refreshed.

## Decision

1. **Walk** (`folders::walk_photos`): depth-first over supported extensions.
   - Hidden entries are skipped.
   - Symlinks are never followed, so there are no cycles and nothing outside the
     granted folder is visited.
   - Unreadable subfolders are counted and skipped.
2. **Fast path for rescans.** Each file is stat'ed. If the catalogue already has that
   path with the same size and modification time, it is only marked as seen
   (`Catalogue::touch_unchanged`, one transaction per batch). Only new or changed
   files are fingerprinted (128 KB read).
3. **Batches of 256 files:**
   - stat and fingerprinting run in parallel on the background lane's bounded pool;
   - each batch is recorded in one transaction;
   - progress is reported after every batch.
4. **Job semantics:**
   - background lane, `Priority::Indexing`;
   - re-indexing a folder supersedes a pass still running on it;
   - a cancelled pass returns early and marks nothing missing, so missing flags only
     come from complete passes.
5. **Only granted roots are indexed.** `index_library_folder(path)` indexes the
   granted root containing `path`, so each file belongs to one library folder and
   "missing" is well defined.
   - Indexing starts when a folder is chosen and when the Library is refreshed.
   - Progress and results arrive as `library://index` events.
6. **Catalogue lifecycle in the app:**
   - `catalogue.sqlite` lives in the OS app-data directory.
   - A corrupt file is moved aside and rebuilt.
   - A catalogue from a newer app version is left untouched; an in-memory catalogue
     is used for the session, with a notice.
   - Quitting cancels indexing without waiting, which is safe because every batch is
     a transaction.
7. **Self-test isolation:** `PE_SELF_TEST` runs use an in-memory catalogue, so tests
   never modify the user's library. (An earlier build did write to it; that file was
   removed.)

## Measurements (M1 Max, warm OS cache, `bench --index-scale 10000`)

| Pass | Time | Throughput |
|---|---|---|
| First index, 10,000 new files (70 KB each, 100 folders) | 491 ms | ~20,000 files/s |
| Rescan, nothing changed | 47 ms | ~210,000 files/s |
| Rescan, 1% changed | 59 ms | ~170,000 files/s |

These figures predate milestone 3, which added the details stage (ADR 0014) and
raised the synthetic figures (first 817 ms, of which details 218 ms; rescan 74 ms,
~25 ms of that from migration 3's index). PERFORMANCE.md §10 has the current numbers.

The directory walk takes ~9 ms, and the catalogue including WAL is ~11 MB. Cold-cache
and spinning-disk runs are still to be measured, and the first index will be slower
there because fingerprinting reads 128 KB per file.

## Consequences

- Reopening or refreshing a large folder is cheap enough to do routinely.
- File moves within a library folder are detected (the photo keeps its identity).
  Moves *between* two different library folders are too, since matching is by
  content, provided the destination is indexed after the move.
- The Library still lists folder contents from the filesystem. The catalogue-backed
  grid, metadata and thumbnails are later Phase 2 milestones.
