# ADR 0004: Job lanes with supersession, and a byte-budgeted preview cache

- Status: Accepted (Phase 0)
- Date: 2026-09-28

## Context

Long work must run in the background with priority, cancellation, progress and
failure reporting. A newer interactive render must supersede older ones. Export must
not starve interactive editing. Caches must be bounded and disposable.

## Decision

### Jobs (`crates/jobs`)

- **Lanes with dedicated workers:** `Interactive` (open, previews) and `Background`
  (export; later thumbnails and indexing). Background work can never occupy the worker
  interactive renders need.
- **Priority queue per lane** (`Interactive < VisiblePreview < VisibleThumbnail <
  Indexing < Export < Idle`), FIFO within a priority.
- **Supersede keys:** submitting with a key cancels the previous job with the same key;
  queued jobs are skipped, running jobs see their `CancelToken` and return early. The
  result of a job that noticed cancellation is always reported as `Cancelled`.
- **Bounded compute:** background jobs run inside a rayon pool of `cores/2` threads;
  interactive jobs use the global pool.
- Panics are caught per job and reported as `JobError::Panicked`; the worker survives.
- Progress is a callback passed by the caller (export emits Tauri events).

### Cache (`crates/cache`)

- `ByteLru<K, V>`: least-recently-used, bounded by bytes (default 256 MB for previews),
  O(n) eviction (n is tens of entries).
- `RenderKey`: source identity, recipe hash, width, height, format, renderer version.
- Hashing uses FNV-1a (stable across Rust versions/processes) so the same keys can
  back an on-disk cache later.
- `SourceId` excludes the path: moved files keep their cached renders; modified files
  do not.

## Consequences

- Measured (PERFORMANCE.md): a concurrent export leaves interactive preview latency
  essentially unchanged (~2 ms → ~2.2 ms). X-Trans (Fuji) exports first raised it to
  ~8 ms because LibRaw's OpenMP demosaic used every core. Decoders now receive the
  lane's thread budget (`DecodeOptions::max_threads`), which brings it to ~3.7 ms.
  Render cancellation latency is < 1 ms p50, ≤ 4.4 ms max.
- Decoder-internal threading must respect the lane budget; any future native
  library with its own thread pool needs the same treatment.
- Cancellation granularity is one row chunk for renders but one LibRaw stage for
  decodes, so an in-flight decode may run hundreds of milliseconds after cancellation.
- **Admission policy:** interactive frames go to a transient pool of 1/8 of the
  budget; thumbnail/detail renders go to a settled pool that drags cannot evict
  (`app-core/src/previews.rs`).
- No deduplication of identical concurrent requests yet (cache covers the common case).

## Alternatives considered

- *Single pool with priorities only*: a running export would still hold all workers.
- *Async tasks (tokio) for CPU work*: wrong tool for CPU-bound work; rayon + dedicated
  threads is simpler and predictable.
- *`lru` crate*: fine, but a 100-line byte-budgeted map avoids a dependency and adds
  byte accounting the crate doesn't provide.
