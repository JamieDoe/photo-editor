# ADR 0049: Batch edits and the filmstrip

- Status: Accepted (Phase 7, milestone 6)
- Date: 2026-09-30

## Context

Phase 7 requires batch adjustments (`docs/PRODUCT.md` §36). The design puts a
filmstrip under the photo in Edit:
- 96×64 thumbnails of the Library's photos, with stars, an edited dot and a pick flag;
- the open photo ringed in white, rejected photos dimmed;
- an **All / Picks / Rated** switch in its header;
- a checkbox on each thumbnail to tick photos for batch work ("Tick photos to edit or
  export several at once"), ticked photos ringed in the accent colour;
- with photos ticked, the header reads "3 selected · Sync edits · Export… · ×". Sync
  edits gives them the open photo's edit, keeping each photo's geometry.

The Library had no multi-select.

## Decision

1. **Filmstrip** (`Filmstrip.tsx`) in Edit.
   - It shows the Library view's photos as filtered there, with the filter shared with
     the Library grid. The strip, the grid and ← → always agree.
   - Clicking a thumbnail opens the photo. The strip scrolls to keep the open photo in
     view.
   - Only thumbnails near the view are mounted, so a folder of thousands requests a few
     dozen.
2. **Selection** lives in the Library state, so it survives switching modes and
   photos, and is shared by the strip and the grid.
   - A thumbnail's box or ⌘-click ticks one photo; ⇧-click ticks the range from the
     last one ticked (`rangeToTick`, tested). × clears.
   - Only photos in the current view count as selected.
3. **Sync edits** gives the ticked photos (other than the open one) the open photo's
   settings. It takes the groups Copy takes (ADR 0048), so by default each photo keeps
   its own crop, geometry and masks, as the design's sync keeps geometry.
4. **Paste** with photos ticked sets the copied groups on the open photo (one undo
   step) and on every ticked photo.
5. **One command writes to the library:** `paste_edits_to(paths, source, groups)`.
   - For each photo, it merges the groups into the photo's saved edit
     (`renderer::settings::paste_groups`, the Rust twin of the editor's paste, tested
     alike) and saves it.
   - Photos outside the library are refused, and so are photos edited in a newer
     version; each is reported on its own.
   - The toast gives the count and any left out: "Synced edits to 12 photos · 1 photo
     couldn't be changed".
6. **Undo covers the open photo only.** Changes to the other photos are saved edits;
   each can be changed back like any edit (or reset), not undone as a batch.
7. **Thumbnails refresh.** A photo whose edit changes gets a new revision, and its
   thumbnail is fetched again. The old picture stays until the new one arrives, so
   there is no blank flash.
8. **Not yet:** the batch bar's **Export…**, which belongs to the export queue (the
   next milestone). It is left out rather than shown doing nothing. Ticking photos in
   the Library grid can follow the same state.
9. **Cost:** merging and saving takes 18 ms per 1,000 photos in memory and 32 ms on
   disk (release, load 6–8), so a batch needs no progress bar.
10. **Self-test** (release, real command and catalogue):
    - One photo is given its own crop, then a look is synced onto the folder's other
      seven photos plus one outside the library.
    - All seven are saved as edited, with the look, and the cropped one keeps its crop.
    - The outside one is refused ("It isn't in a library folder").

## Deviations from the design

Recorded in ADR 0016:
- the ★ 3+ filter label (the Library's own; the design says "Rated");
- ⌘- and ⇧-click selection;
- Export… left out until the export queue;
- the count excludes the open photo unless it is ticked.
