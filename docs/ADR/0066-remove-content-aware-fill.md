# ADR 0066: Remove: content-aware fill

- Status: Accepted (part 1: the fill and the recipe; part 2: the Remove brush in the
  Retouch panel)
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

## Part 2: the Remove brush

1. **Remove joins the Retouch panel** as the design has it: first of Remove, Heal and
   Clone, and the tool chosen when the section opens. The design's own wording is used:
   "Paint over anything to erase it", and "Paint over anything distracting — people,
   litter, power lines — and it's filled in from its surroundings."
2. **Painting:**
   - **Dragging** over the photo paints, shown in the design's amber at half
     strength. A still click paints a dab.
   - **On release** the stroke becomes one removal in the recipe and is filled. The
     fill never runs while dragging, only once per stroke, so painting stays smooth.
     The first render with it takes 72 ms (Nikon Z 6, self-test), a person-sized area
     about 0.3 s (PERFORMANCE §48).
   - **Coordinates:** each point is recorded in the source photo's coordinates through
     the same mapping as spots, so a removal painted on a cropped, straightened or
     turned photo lands where it was painted.
   - **Size:** the brush's size (5–200 px on screen, `[` and `]` as for spots) becomes
     the stroke's size as a fraction of the photo's diagonal. The edge is softened
     slightly (feather 10) to blend the fill in.
3. **Removals on the photo:**
   - **Pins:** a small pin marks where each removal was started, sized in screen
     pixels like the rings.
   - **Selecting:** a still click on a painted area selects it. Its area shows in
     translucent white and its pin turns white.
   - **Deleting:** Delete removes the selected removal, and Escape deselects it.
   - **Spots:** spots can still be clicked and dragged while Remove is the tool.
4. **In the panel:**
   - **Count:** the section counts removals and spots together, and the line under the
     slider names both ("2 removals · 1 spot").
   - **Clear all** clears both in one edit, so one undo restores both.
   - **Reset:** a photo with removals counts as edited.
5. **Tests:**
   - **Helpers (vitest):**
     - removals are left out of the recipe when there are none;
     - the stroke size follows the brush on screen, by the photo's diagonal, in
       landscape and portrait;
     - points are rounded as the renderer keeps them;
     - the removal under a point is found along its whole stroke, the last made
       winning.
   - **The dev mock in the browser:**
     - dragging makes "1 removal";
     - a still click on it selects it (white area, white pin, the Delete hint);
     - Delete removes it;
     - Heal still places a spot with its source ring.

## Matching the fill's tone to its edges

After trying the brush, the photographer found the fills "a little jarring". The
commonest cause is tone: patches copied from elsewhere bring their own brightness and
colour, and the fill sits lighter, darker or tinted against its surroundings, which
reads as a seam.

1. **The correction:** after the last round at full size, the fill is matched to its
   edges:
   - in a 2 px ring of known pixels around the hole, the photo is compared with what
     the matched patches predict there;
   - the difference is spread smoothly over the hole by pull-push interpolation
     (Gortler et al.) and added.
   - The texture stays; only its tone follows the edge, as Heal does for spots (ADR
     0054).
2. **Bounded:** the interpolation covers only the hole and its ring, not the work
   region. Over the whole region (about 10 MP for a person-sized hole at 24 MP) it
   cost 0.5 s; cropped, the fill costs 5–10 % more than without it (PERFORMANCE §48).
3. **Tests:**
   - with every patch matched from a brighter half of the image, the fill comes out
     at the darker half's level around the hole (within 0.01), and nothing outside the
     hole changes;
   - pull-push across a square hole whose ring runs from 0 on one side to 1 on the
     other gives a smooth, monotonic rise, about 0.5 at the centre, for odd and even
     sizes.
   - Pull-push is an approximation: with known values only at two far ends (no
     ring), it leans towards one side. The edge correction always has a ring.
4. **Tried and left out:** a sharper final vote (weights relative to the 25th
   percentile at full size) made no visible difference on the test photos.
5. **Not addressed:** copied structure in large holes (the train over the table's
   edge) is unchanged. That needs the hidden structure to exist elsewhere, or a learned
   fill.
6. **Smooth areas (ADR 0070):** viewed at 100 %, fills in smooth areas (a defocused
   background) still showed the hole's outline as a crisp ring.
   - **Why the ring correction missed it:** the patches matched at the hole's edge
     predicted the photo there well, so the difference came out near zero. A few
     pixels in, patches from a darker place took over, leaving a step just inside the
     edge.
   - **The fix:** where the photo around the edge is smooth, the correction also
     compares averages across the edge: the photo's within 6 px outside against the
     fill's within 6 px inside, measured from the hole pixels near the edge. Smooth
     means a local variance below 0.0004 (in square roots of linear light, with a
     steep Gaussian fall-off). The fill side must be smooth too.
   - **Detailed areas** keep the prediction-based comparison, weighted the other way.
     In a detailed area, averages across an edge mean little: the fill may rightly
     carry fur on one side and wall on the other. Measured there, they smeared dark
     fur into a plain wall.
   - **Where smoothness is measured:** on the ring outside, where every window is at
     least half photo. A window inside may hold only a corner of photo, too little to
     judge by: there, stripes passed for smooth.
   - **Cost:** the fill takes 10–20 % longer (PERFORMANCE §48).
   - **Tests:** the existing tests still pass (stripes still continue across the
     hole, and the tone still follows its edge). On the Nikon Z 6, a removal over a
     defocused map no longer shows its outline at either size, and one at the edge of
     the fur is as clean as before.

## Consequences

- **Adding to a removal** (painting more onto a selected one) is not offered: each
  stroke is its own removal. Strokes that overlap still fill as one area, since a
  later removal sees the earlier fill.
- **Previews:** fills made at different sizes differed, in structure as well as
  detail, which zooming made plain. Since ADR 0070, an open photo's removals are filled
  once at full resolution and that fill is shown, scaled, at every size.
- **A learned fill** (a local ONNX model, PRODUCT's AI subsystem) could replace the
  algorithm for hard cases, behind the same `Removal`.
