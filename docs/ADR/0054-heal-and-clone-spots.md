# ADR 0054: Heal and clone spots

- Status: Accepted
- Date: 2026-10-01

## Context

Retouching is in V1 (PRODUCT §06: heal, clone, spot removal), and it was the biggest
editing gap. There was no way to remove dust spots, blemishes or small distractions,
and Lightroom presets listed healing as left out.

The design has a **Retouch** section:

- three tools: Remove (paint over anything to erase it), Heal and Clone, each with a
  line of explanation;
- a Brush size slider (5–200);
- a sensor-dust banner ("2 sensor dust spots found in the sky · Fix all") and dashed
  amber rings on the photo where the spots are.

## Decision

1. **A recipe field `spots`** (recipe v24, written only when there are some): spots in
   the order made. Each spot has:
   - a kind, `heal` or `clone`;
   - a centre `x`, `y` and a source `sourceX`, `sourceY`;
   - a `radius`;
   - `feather` (0–100, default 30) and `opacity` (0–100, default 100).
   - Spots belong to the photo, like masks: presets leave them out and keep the
     photo's own, and they copy as their own group, "Retouch", off by default.
2. **Spots are in the source photo's coordinates:** fractions of its width and height,
   with the radius a fraction of its long edge.
   - A spot stays on the dust it covers whatever the crop, straighten, perspective or
     quarter turns, and applies alike at every preview size and at export.
   - Masks, by contrast, are in the frame (ADR 0040).
   - The editor maps between the frame and the source with the same maths as the
     renderer: `frameToSource`, which mirrors `Mapping::source`. It inverts it by
     Newton's method rather than writing an inverse homography. Round trips are
     tested under straighten, perspective, turns and flip.
3. **Applied to the source before anything else**, in order, so each spot sees the
   ones before it. The renderer keeps the retouched source cached for the last two
   sources (the interactive and detail levels take turns). Dragging other controls
   then reuses it, along with everything keyed on it (framing, scene maps).
   - Images now carry an exact id (`LinearImage::id`), and the renderer's caches key
     on it. They used to key on the buffer's address plus a sparse pixel fingerprint.
     A retouched copy differs only inside its spots, which a sparse fingerprint can
     miss, and a new copy can reuse a freed address.
4. **Clone** copies the source disc (bilinear, so sources need not sit on whole
   pixels).
5. **Heal** copies the source's texture and adds a smooth correction that takes the
   spot's surroundings' colour and brightness.
   - The correction is the harmonic membrane (the smoothest surface) matching
     surroundings minus source around the edge, as seamless cloning does.
   - For a disc, the Poisson integral gives it directly. Normalised over the edge
     samples, its `(R² − |p|²)` factor cancels, leaving inverse squared distances to
     the edge samples.
   - The edge samples are spaced about 2 px apart (12–128 of them). Each averages a
     short radial run just outside the edge, and they are smoothed around the ring,
     so grain at the edge does not streak inward.
   - The membrane is evaluated on a grid of about 50 × 50 and interpolated.
   - Both kinds blend in with a smoothstep feather over the outer part of the radius.
   - The new disc is computed from the image as it was, then written, so a source
     that overlaps its spot reads clean pixels.
6. **The engine picks the source** (`new_spot`, on the interactive lane, using the
   preview level nearest 1500 px):
   - candidates at 2.5, 3.5 and 5 radii in 24 directions, wholly inside the photo
     and clear of existing spots;
   - each scored on how closely its surroundings match the spot's (two rings, on
     square roots of linear light) and how closely its inside matches the spot's
     surroundings, so a source with a blemish of its own scores badly;
   - nearer candidates win slightly. If nothing fits, the editor says so and adds no
     spot.
7. **UI:** the design's Retouch section, after Selective.
   - Opening it puts the photo in retouch mode, closing crop, masks and compare. Those
     tools take the photo back when opened.
   - **Placing a spot:** click the photo. The pointer shows the brush's ring, and the
     new spot uses it as its size.
   - **Editing a spot:** drag a spot, or its dashed source circle, to move it.
     Option-click takes the selected spot from there. Delete removes it, Escape
     deselects, and `[` and `]` resize.
   - **On the photo:** spots are the design's dashed amber rings; the selected one is
     white, joined to its dashed white source.
   - **Panel:** the tool switch (Heal, Clone), its hint, Brush size (the spot's
     diameter in screen pixels), then the spot count and Clear all. While a spot is
     selected, the switch and the size change that spot, as in Lightroom.
8. **Tests:**
   - **Rust:**
     - no spots, or a spot copying from itself or invisible, changes nothing;
     - heal restores a gradient under a blemish (within 1%) and keeps the source's
       texture;
     - clone copies exactly; the edge blends in;
     - spots land alike at half size;
     - the source search avoids existing spots and finds none when nothing fits;
     - rendering heals before framing (still healed after a quarter turn) and never
       shows a stale cache when a spot moves; sanitising.
   - **Frontend:** frame ↔ source mapping (turns, flip, straighten, perspective);
     finding the spot or source under the pointer; moving spots; spots left out
     when there are none.
   - **Release self-test, Nikon Z 6:**
     - the engine found a source 2.5 radii away;
     - the spot changed its disc by 17.5 levels on average, and every pixel outside
       it was unchanged;
     - the first render with the spot took 5.8 ms, against 4.3 ms without. With the
       spot cached, an exposure change rendered in 4.4 ms.
9. **Performance** (PERFORMANCE §37), for five heal spots:
   - 2.3 ms on a 1516 × 1010 preview level;
   - 12 ms on a full 24 MP export, 3.5 ms of it the copy;
   - finding a source: 0.05 ms.

## Deviations from the design

Recorded in ADR 0016:

- **Remove** (paint over anything to have it filled in) is not offered. It needs
  content-aware filling, which PRODUCT §06 lists as Later.
  (The fill was built in ADR 0066, and the Remove brush follows it.)
- The sensor-dust banner and Fix all are not built. Finding dust needs its own
  detection work.
- Clone's hint says to drag the source circle or Option-click, rather than only
  Option-click.
- The spot count and Clear all are added under the slider.

## Consequences

- **Memory:** a render with spots holds one retouched copy of its source level, the
  same size as the level. At export that is a transient full-resolution copy (about
  150 MB at 24 MP). Patching only the spots' boxes as rows are read would avoid it,
  but framing reads the source freely, so it would need a patched view for both
  paths.
- **Later:**
  - brushed (stroke) spots, and Lightroom's `RetouchAreas` on import;
  - Remove (content-aware fill);
  - dust detection;
  - feather and opacity controls (stored now, at their defaults).
