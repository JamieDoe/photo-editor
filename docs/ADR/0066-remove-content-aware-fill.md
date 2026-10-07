# ADR 0066: Remove: content-aware fill (part 1, the fill)

- Status: Accepted (part 1 of 2: the fill and the recipe; part 2 is the Remove brush
  in the Retouch panel)
- Date: 2026-10-07

## Context

The design's Retouch panel has three tools: Remove (paint over anything to erase it),
Heal and Clone. ADR 0054 built Heal and Clone and left Remove for later, because it
needs content-aware filling. PRODUCT lists content-aware remove under Later. The
photographer chose to build it now: it is the most-used retouching tool, for power
lines, people, signs and litter.

Requirements, from CLAUDE.md:

- non-destructive: the fill is data in the recipe, recomputed when rendered;
- deterministic: the same recipe renders the same pixels;
- local: no service and no model download;
- responsive: never block the UI, and support cancellation;
- one renderer for previews and export.

## Decision

1. **Example-based inpainting, no machine learning:**
   - Wexler, Shechtman and Irani's completion, with PatchMatch (Barnes et al.) to find
     examples.
   - **Matching:** every 7 × 7 patch touching the hole is matched to the most similar
     whole patch outside it.
   - **Voting:** each hole pixel becomes the weighted vote of the matched patches
     covering it. Better matches count for more: weights fall off with distance
     relative to the 75th percentile.
   - **Coarse to fine:** matching and voting alternate over a pyramid of the region.
     The coarsest level makes the hole about a patch across; it starts from an "onion
     peel" fill, from the edge inward. Each finer level starts from the coarser
     level's matches.
   - **Rounds:** 10 at the coarsest level, 5 in between, 3 at full size.
   - **Strengths:** sky, grass, water, walls and repeating texture, which make up most
     removals. Large objects in front of detailed, non-repeating backgrounds are
     harder. A learned model could later replace the fill behind the same recipe and
     tool.
2. **Bounded work:** only a window around the hole is filled.
   - **The hole:** the painted area, wherever coverage is above 1/255. The brush's soft
     edge blends the fill in.
   - **Context:** a band four times the hole's half-thickness (at least 28 px) around
     its bounds. The thickness is measured with a chamfer distance inside the hole, so
     a thin wire across the frame costs a thin strip.
   - **Rasterising:** the strokes are rasterised over just that window. The brush
     rasteriser gained a windowed form for this (`rasterize_window`, `reach`), sharing
     its painting code with the masks. Rasterising a whole 24 MP frame would allocate
     about 100 MB.
3. **Matching is perceptual:** it compares the square root of linear light, so shadows
   count as much as highlights. The fill goes back to linear light.
4. **Deterministic:**
   - every random choice comes from a fixed seed;
   - matching runs in parallel over fixed bands of 16 rows, each reading neighbours
     outside itself from before the sweep;
   - so the result doesn't depend on the number of cores or on scheduling, which is
     tested on 1 and 4 threads.
5. **In the recipe (version 25):** `removals`, each a list of brush strokes (the masks'
   `Stroke`):
   - in the source photo's coordinates, so a removal stays on what it covers whatever
     the crop, straighten or turn;
   - empty or erase-only removals are dropped;
   - like spots, removals belong to the photo: presets leave them out, and applying a
     look keeps them;
   - copy and paste carries them in the Retouch group;
   - older recipes have none, and existing edits render exactly as before (no renderer
     version change).
6. **In the render:**
   - **Order:** removals are filled on the source first, then spots, then everything
     else. A heal or clone can therefore sample the filled area.
   - **Resolution:** the fill is computed at the resolution rendered: the interactive
     or detail preview, or the full image for export.
   - **Cache:** the result joins the existing retouch cache (one per source, last two
     sources, keyed exactly on every removal and spot), so dragging other controls
     never fills again.
   - **Cancellation:** a fill can be cancelled between rounds, and a cancelled fill is
     never cached.
7. **Tests:**
   - **The fill:**
     - painting nothing changes nothing;
     - a dark object on a flat wall fills to the wall, and nothing outside the hole
       changes;
     - stripes continue through a hole exactly (error 0, where a smooth fill leaves
       0.21);
     - a wire across a sky gradient disappears (within 0.03 in square roots);
     - a hole at the image's corner fills;
     - the same result on 1 and 4 threads, and run twice;
     - cancellation stops it;
     - the thickness measure.
   - **The rasteriser:** a window paints what the whole map paints there, and nothing
     lies outside the reach.
   - **The render:**
     - a post painted over is gone, and still gone after a quarter turn;
     - the same again from the cache;
     - a clone spot after a removal applies on top of the fill;
     - a cancelled render fails rather than caching half a fill.
   - **Release self-test** (`remove`), on the Nikon Z 6 through the real IPC, engine
     and cache:
     - a short stroke's area changes by an average of 22.6 levels;
     - nothing changes more than 8 px outside it;
     - the first render with the fill takes 72 ms;
     - with it cached, an exposure change renders in 4.4 ms, against 3.4 ms without a
       removal.
   - **Recipe, presets and copy/paste:**
     - removals round-trip, and empty ones are dropped;
     - a version 24 recipe has none;
     - presets leave them out, and applying a look keeps them;
     - every recipe field is in exactly one copy/paste group.
8. **Starting from the edge inward:** the coarsest level is first filled ring by ring
   from the hole's edge (as Newson et al. initialise). Each pixel takes the centre of
   the whole patch best matching the pixels already known around it, out of 96 random
   patches, its neighbours' matches shifted, and a refinement around the best. A
   smooth average first guess (tried first) is a bland grey that flat patches
   elsewhere then match best.
9. **Checked by eye** on real photos, at preview size:
   - **Clean results:**
     - a clump of grass on a sand path (Ricoh GR III): sand texture, with no visible
       patch;
     - a building and posts on a distant horizon: the horizon and sky continue.
   - **Artefacts:** any structure the stroke leaves at the hole's edge (a post half
     covered) is continued into it, and a stray lamp post was copied in.
   - **A hard case:** a toy train across a table (Nikon Z 6) hides the table's front
     edge. The fill continues it with pieces of a metal rail lower down. Sharper vote
     weights, a tighter context band and more rounds were each tried and did not help.
     A learned fill could; this one needs the hidden structure to be visible somewhere
     else.
10. **Performance:** see PERFORMANCE §48 (`bench --remove`).

## Consequences

- **Part 2:** the Remove brush in the Retouch panel, built from the masks' brush
  painting. It covers painting, adding to, removing and listing removals, with the
  fill shown as each removal is painted.
- **Previews:** the fill differs in detail between preview sizes and export, though not
  in structure (the same is true of noise reduction and sharpening).
- **A learned fill** (a local ONNX model, PRODUCT's AI subsystem) could replace the
  algorithm for hard cases, behind the same `Removal`.
