# ADR 0014: Photo details (metadata) from file headers, stored in the catalogue

- Status: Accepted (Phase 2, milestone 3)
- Date: 2026-09-28

## Context

The Library, search ("places, cameras, dates" in the design) and the Edit panel's EXIF
strip need camera, lens, capture time and exposure settings. PRODUCT.md §17:
- keep metadata behind an abstraction;
- never modify originals;
- avoid Exiv2 (GPL-2.0) without a licensing review.

## Decision

1. **`raw::Decoder::read_metadata`** returns `PhotoMetadata` (all fields optional)
   from headers only, with no image decoding:
   - **Camera RAW:** a LibRaw shim (`pe_raw_metadata`: `libraw_open_file` only):
     - make, model and lens (EXIF lens, else maker-notes lens);
     - ISO, aperture, shutter, focal length;
     - displayed dimensions, orientation, GPS.
   - **JPEG:** `kamadak-exif` (BSD-2-Clause, pure Rust) over the first 256 KB of the
     file, plus the JPEG frame header for dimensions.
2. **Capture time is the camera's wall-clock time** ("2026-09-24T06:41:12", no zone),
   because cameras don't record a time zone.
   - LibRaw converts the recorded fields to a timestamp using this machine's zone; the
     shim converts straight back, so the digits match what the camera wrote.
   - The UI formats it as UTC, so they never shift.
   - As text it sorts chronologically.
3. **Stored on `photos`** (migration 2), so details follow a photo across moves. The
   catalogue has its own `PhotoDetails` type and does not depend on decoders;
   `app-core` maps between the two.
4. **Read once:**
   - `photos.metadata_version` below `METADATA_VERSION` means "needs reading".
   - That covers new photos, photos whose content changed, libraries indexed before
     this milestone, and future extraction improvements (bump the version).
   - Failures are stored as empty details and not retried until the file changes.
5. **Indexing has two stages:**
   - recording files (ADR 0013);
   - then reading details for the queue, in parallel batches of 256 per transaction,
     with progress reported as `ReadingDetails`.
   - Cancelling leaves the rest queued.
6. **Shown now:**
   - Library "Taken" and "Camera" columns, from the catalogue in one query per folder.
     Unindexed photos fall back to the file date, shown dimmed.
   - The camera and exposure line in Edit ("Nikon Z 6 · ISO 100 · 52 mm · f/6.7 · 1 s").

## Found while benchmarking: duplicate files were quadratic

`bench --index-links 10000` uses 10,000 hard links to the six real camera samples,
so it parses real RAW headers.

- **The problem:** the first index took **36.8 s**. Move detection fetched every
  earlier file with the same content and checked each on disk.
- **The fix:**
  - skip files already seen in the current scan (they exist, so they can't be a
    move's source), as a range query on a new (size, fingerprint, last_seen_scan)
    index (migration 3);
  - cap on-disk checks at 32, missing files first.
- **Result:** **1.86 s**, 0.98 s of it reading real RAW headers for all 10,000 files.
  A 2,000-copy test guards against regressions.
- **The cost:** rescans now update that index for every file they touch.
  - Measured with and without `last_seen_scan` in the index: an unchanged
    10,000-file rescan takes ~76 ms instead of ~50 ms; first-index time is unchanged.
  - Without the column, the duplicate case takes 4.6 s at 10,000 copies and keeps
    growing quadratically.
  - We accept the linear cost to bound the worst case; both paths are background work.

## Consequences

- Header reads take ~0.25–0.4 ms per file warm (LibRaw) and ~0.13 ms (JPEG), so
  details add about 1 s per 10,000 photos on this machine.
- Timezone-aware times, GPS place names and XMP sidecars are future work, as is
  showing the photo's own capture timezone when cameras start recording one.
- GPS coordinates are stored locally and never leave the machine. Stripping location
  on export is an export option in the design, for Phase 7.
