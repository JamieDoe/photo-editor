# ADR 0045: Before / after

- Status: Accepted (Phase 7, milestone 2)
- Date: 2026-09-30

## Context

Phase 7 requires before/after (`docs/PRODUCT.md` §36). The design has it:
- a Compare button in the photo toolbar, after Crop and Masks, tinted in the accent
  colour when on;
- over the photo, a white divider with a round ↔ handle;
- "Before" and "After" pills in the top corners;
- the whole photo a range input to drag the divider (2–98 %, starting at 50 %).

Previews for the viewer all share one supersede key: a new request cancels whatever
the viewer is rendering. A second image rendered alongside the edit would cancel it,
and be cancelled by it.

## Decision

1. **Before is the photo unedited, framed as the edit.** Every adjustment is at its
   default, with the edit's geometry (crop, straighten, perspective, turn and flip), so
   the two sides line up (`beforeRecipe`). Masks, curves, the colour mixer and lens
   corrections are left out.
2. **Preview slots.** `render_preview` takes an optional `slot` (`viewer` by default,
   or `compare`).
   - Each slot has its own supersede key, so a new request cancels only the same
     slot's earlier render.
   - Compare renders run on the interactive lane at `VisiblePreview` priority, behind
     the edit's renders.
   - Both slots go through the same preview cache.
3. **Rendered once.** The before image is a detail-quality render at the photo box's
   long edge in device pixels, rounded up to 64 px.
   - It is requested again only when the photo, the before recipe (so the geometry)
     or that size changes. Moving the divider or editing renders nothing new for it.
   - It is drawn on its own canvas over the viewer's, clipped at the divider with CSS
     (`clip-path`). The edit shows through on the right, live.
   - A render for another photo or framing is never shown. Until the new one arrives,
     the Before pill says "Before…".
4. **One mode at a time.** Opening Crop or Masks closes the comparison, and Compare
   closes them. `\` toggles it, as in other editors.
5. **Self-test** (Nikon Z 6, release):
   - After clicking Compare, the before image was drawn within 38–57 ms, at 3032×2020.
   - Its mean was 107.3, against 107.4 for a plain render of the unedited photo. The
     edit, 1 EV brighter, measured 208.3.
   - Editing while comparing rendered the edit and did not render the before image
     again.
   - Clicking Compare again closed it.

## Deviations from the design

Recorded in ADR 0016:
- the `\` shortcut;
- the "Before…" pill while the before image renders;
- Compare sits in the toolbar only; the design also has an icon button in its focus
  header, which this app does not have.
