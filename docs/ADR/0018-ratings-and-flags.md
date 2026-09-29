# ADR 0018: Ratings and flags live in the catalogue, on the photo

- Status: Accepted (Phase 2, milestone 6)
- Date: 2026-09-29

## Context

Library V1 needs ratings (0–5), pick/reject flags and filtering (PRODUCT.md §3.1).
- §17 keeps "application metadata" (rating, flag, colour, keywords) separate from the
  file's own metadata, and ordinary editing must never modify the original.
- The design adds a filter ("All / Picks / ★ 3+"), library-wide Picks / Rated /
  Rejected collections, stars and a pick badge on cards, and star and flag controls
  in Edit.

## Decision

1. **Storage.** Migration 4 adds `photos.rating` (0–5) and `photos.flag` (−1 reject,
   0 none, 1 pick), with CHECK constraints and partial indexes.
   - Marks attach to the *photo*, not the file, so they follow moves and renames
     (ADR 0012), and survive an edit of the file's content.
   - Nothing is written to the original.
   - `Rating` cannot hold more than 5, so invalid values are unrepresentable.
2. **Marking an unindexed photo.** If a folder's index hasn't reached a photo yet,
   `set_photo_marks` records that one file on the spot. The next full pass sees it as
   unchanged. Only photos inside granted folders can be marked.
3. **Listing.**
   - Folder listings still come from the filesystem, now joined with details and marks
     from the catalogue in one query each.
   - Library-wide collections come from the catalogue (present files only,
     capture-time order), filtered to granted folders.
   - Counts are returned by every mark change, so the sidebar stays current.
4. **UI rules** (pure functions in `features/library/marks.ts`, unit-tested):
   - **Keyboard:** 0–5 rate, P pick, X reject, U unflag. Clicking the current star, or
     the active flag, clears it (as in the design).
   - **Filters** apply within the current view.
   - **Collections:** a photo that no longer belongs to the collection being viewed
     disappears at once.
   - **Instant feedback:** changes appear immediately and are then stored. A failure
     reloads the view and shows the error.
5. **Selection.** One click selects, a double-click or Enter opens, and arrow keys move
   the selection. This departs from the design, where one click opens, because culling
   needs a selection to rate with the keyboard. In Edit, ← → step through the Library's
   current (filtered) photos, so culling works at full size.

## Consequences

- **Marks are the first catalogue data that cannot be rebuilt from the files.** A
  corrupt catalogue is moved aside (ADR 0012), and its marks go with it.
  - Before V1 we need a way to keep them safe. Options: regular catalogue backups, or
    optional XMP sidecars (industry-standard, readable by other tools; needs a
    licensing-clean XMP writer).
  - Until then the old file is kept, so marks can be recovered by hand.
- Colour labels (PRODUCT.md §3.1) follow the same pattern: one more column, key and
  filter. They wait until the design shows them.
- Multi-select and batch marking reuse `set_marks(&[PhotoId], …)`, which already
  takes many photos.
