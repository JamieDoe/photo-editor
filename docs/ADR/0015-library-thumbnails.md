# ADR 0015: Library thumbnails from embedded previews, on a browse lane, cached on disk

- Status: Accepted (Phase 2, milestone 4)
- Date: 2026-09-28

## Context

PRODUCT.md workflows A and D: thumbnails are generated, and scanning a folder must
stay responsive. §8.2 says the thumbnail render must be "very cheap", and the
performance requirements say visible thumbnails appear progressively and indexing
never makes the interface unusable. CLAUDE.md §9 requires bounded, disposable caches
whose keys cover source identity, recipe, resolution and renderer version. §10 ranks
visible thumbnails above background indexing.

Two constraints came from the existing design:

- The job system had two lanes. The background lane's single worker is held for
  seconds by an indexing pass, so thumbnails queued behind it would wait regardless
  of priority.
- Decoding and rendering every RAW file for its thumbnail costs 150–1,000 ms each.
  Cameras already embed a full-quality JPEG preview.

## Decision

1. **Source:** `Decoder::display_preview(path, 512)`.
   - Camera RAW files use the embedded preview, DCT-scaled with libjpeg-turbo (ADR 0007).
   - JPEGs use a reduced-scale decode of the file itself.
   - When there is none, or it is smaller than 512 px, the fallback is a reduced
     decode (`DecodeScale::AtLeast(512)`) rendered with the default recipe.
2. **Size and format:**
   - Long edge **512 px** (area-averaged by `image_core::resize::fit_long_edge`, never
     upscaled), which is sharp on high-DPI screens for tiles up to ~256 CSS px.
   - JPEG quality 80, about 30–55 KB each.
3. **Disk cache** (`cache::DiskCache`) in the OS cache directory (`…/thumbnails`):
   - Files are named by a 64-bit key in 256 shards.
   - The key hashes the canonical path, size, modification time, thumbnail size,
     `THUMBNAIL_VERSION` and `RENDERER_VERSION`.
   - Recency is the file time, refreshed on a hit at most hourly.
   - It's bounded at 1 GiB (~20,000 thumbnails). Eviction removes the least recently
     used entries down to 90%.
   - Writes are atomic but not synced. A truncated entry (no JPEG end marker) is
     removed and regenerated.
   - Self-test runs use a fresh temporary directory.
4. **A third job lane, `Browse`:**
   - `(cores / 3).clamp(1, 4)` workers, with a compute pool of the same size.
   - Thumbnails run at `Priority::VisibleThumbnail` and never wait for indexing or
     export on the background lane.
5. **Per-photo cancellation:**
   - Each request supersedes an earlier one for the same path (`library-thumbnail:<path>`).
   - `cancel_thumbnail` cancels it. The Library calls it when a row scrolls out of
     view before its thumbnail arrives, so fast scrolling doesn't build a backlog.
   - Finished jobs now remove their supersede key, so per-item keys don't accumulate.
6. **IPC:**
   - `library_thumbnail(path)` returns the JPEG bytes as a binary response, only for
     paths inside granted folders.
   - A cache hit is a file read on the blocking pool; a miss is an engine job.
   - The UI turns the bytes into an object URL. A shared `IntersectionObserver`
     requests thumbnails only for visible rows and frees them when rows leave.
   - We rejected a custom URI scheme (`<img src="thumb://…">`) because it gives no
     cancellation signal to Rust.

7. **Pre-generation (added in milestone 5).**
   - After a successful index, the app queues the library folder's present photos for
     `Engine::pregenerate_thumbnails`.
   - Each photo is its own job on the background lane at `Priority::Idle`, so exports
     and indexing (same lane) and on-screen thumbnails (browse lane) always go first,
     and a queued batch costs one thumbnail of latency at most.
   - Photos already cached are skipped with a metadata check.
   - Re-indexing the folder cancels the previous batch.
   - A batch is capped at what fits the cache budget (1 GiB / 64 KB, ~16,000 photos),
     so it never churns the cache.
   - It follows the Background-processing setting, because it runs inside that lane's
     compute pool.

## Measurements (M1 Max, warm OS cache; `bench --thumbnails`)

| Case | Time |
|---|---|
| Nikon Z 6 NEF (embedded) | 5 ms |
| Canon R6 CR3 / Sony A7R IV ARW (embedded) | 12–13 ms |
| Sony A7 III ARW / Fuji X-T3 RAF / Ricoh GR III DNG (embedded) | 17 / 34 / 44 ms |
| 24 MP JPEG (DCT-scaled decode) | 21 ms |
| 24 MP DNG without a preview (decode + render fallback) | 182 ms |
| Cache hit (in-process) | 0.03 ms; ~1 ms through IPC |
| 48 thumbnails at once, 3 workers / 1 worker | 340 ms / 995 ms |
| 48 thumbnails while an index pass runs | 344 ms (index unaffected: 538 ms) |
| Pre-generating 300 (background lane, idle) | 6.2 s (48/s); an on-screen request meanwhile: 11.5 ms |

## Consequences

- Library rows show thumbnails progressively. Revisiting a folder is served from
  disk in about a millisecond per photo.
- Thumbnails show the original. Once edit recipes are saved (a later milestone), the
  recipe hash joins the key and edited photos are re-rendered.
- **Known limitations:**
  - JPEG EXIF orientation is still ignored (as in the viewer).
  - A moved or renamed file gets a new thumbnail. Keying by the catalogue's content
    fingerprint would avoid that at the cost of a lookup per request.
  - Pre-generation shows no progress in the UI yet. It is background work, and its
    thumbnails appear as the photos are browsed.
