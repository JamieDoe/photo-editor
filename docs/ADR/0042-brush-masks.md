# ADR 0042: Brush masks

- Status: Accepted (Phase 6, milestone 3)
- Date: 2026-09-30

## Context

The product's V1 masks are brush, linear and radial (`docs/PRODUCT.md` §5.1).

The design's mask overlay shows the brush as two rings: the brush's size and, dashed,
where its soft edge starts. Its mask list gives the Brush a pink dot.

Brushes are the first masks whose data is more than a few numbers, and whose
coverage can't be computed per pixel in closed form.

## Decision

1. **Data:** shape `brush { strokes }`, in the order painted (recipe v19). Each
   stroke is stored as data, never as pixels:
   - `points`: its path, in frame fractions, rounded to 1/10000;
   - `size`: the brush's radius, as a fraction of the frame's diagonal (a turn leaves
     it alone, as with radial radii);
   - `feather` (0..100): the share of the radius the soft edge takes;
   - `flow` (0..100);
   - `erase`.
2. **Meaning:**
   - A stroke's own coverage is the brush's profile (full within the unfeathered
     radius, smoothstep to nothing at the radius) at its distance from the path. It
     is even along the path, whatever the speed or point spacing.
   - Its flow is its strength. Paint strokes build up over what is there
     (`c + (1 − c)·a`, so two 50 % strokes make 75 %).
   - Erase strokes take away (`c·(1 − a)`), only from what was painted before them.
3. **Rendering:** strokes are rasterised into a 16-bit coverage map, 2048 px on the
   frame's long side, and renders read it with bilinear sampling.
   - Exact distance to each segment, no dabs.
   - Paths are simplified (Douglas–Peucker, within ¼ map pixel) first.
   - Rows are rasterised in parallel, each testing only the segments that reach it.
   - Maps are cached by the strokes' content and the map's size (a few maps kept), so
     other controls never repaint them.
   - While a stroke is painted, only it is rasterised, over the cached map of the
     strokes before it. Coverage is rounded to the map's precision after every stroke,
     so painting in steps and rendering all at once (as the export does) give
     identical maps (tested).
   - The 2048 px map is about a third of a 24 MP frame's width. Soft brushes don't
     show this; a hard-edged brush's edge is soft by a pixel or two in a
     full-resolution export.
   - An empty brush mask covers nothing and is left out of the plan, unless it is
     inverted (then it covers everything).
4. **Cost** (a 3:2 frame, docs/PERFORMANCE.md §30):
   - rasterising a 200-point stroke takes 1–8 ms depending on its size;
   - ten medium strokes take about 22 ms;
   - each update while painting a stroke over ten others takes about 2.4 ms.
5. **UI:**
   - **Adding:** Brush in Add (toolbar and tiles), first as in the design's order. A
     new brush mask is empty and ready to paint.
   - **Painting:** drag on the photo, at most one edit per display frame; the preview
     scheduler handles the rest. Option erases while held, and `[` and `]` resize.
   - **Cursor:** the design's two rings follow the pointer, the outer one the size and
     the dashed inner one where the soft edge starts, with a minus sign when erasing.
     The system cursor is hidden over the photo.
   - **Tint:** drawn in SVG from the strokes, in order.
     - Paint strokes are round-capped paths at their flow, with a blur for their
       feather.
     - Each erase stroke masks out the layers before it, so the tint follows the
       renderer's composition.
     - An inverted brush's tint is the complement, drawn through a mask.
   - **Mask card:** Paint/Erase, Clear, and Size (1–100, 100 being a quarter of the
     diagonal), Feather and Flow, plus a one-line hint. These are brush settings for
     the next stroke, not stored; each stroke keeps its own.
6. **Self-test:** a −1 EV stroke across the middle of the Nikon sample darkens that
   band by 38.5 levels and leaves the top unchanged.

## Deviations from the design

Recorded in ADR 0016: the brush settings in the mask card (the design shows none), and
the erase minus in the cursor.

## Consequences

- Brushes stay put on the picture when cropped, and turn with it. Straighten and
  perspective don't move them (as with gradients, ADR 0040). A large straighten after
  painting shifts them slightly against the picture.
- Undo (Phase 7) will treat each stroke as one step. Strokes are already separate
  entries.
