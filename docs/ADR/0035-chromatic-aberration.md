# ADR 0035: Remove chromatic aberration

- Status: Accepted (Phase 5, milestone 6)
- Date: 2026-09-30

## Context

The design's Geometry › "Perspective & lens" ends with two switches: Lens correction
and Remove chromatic aberration ("Cleans colour fringing on edges").

- **Lens correction is deferred.** Profile-based correction needs a lens database. The
  open one, Lensfun, is an LGPL-3 native library with a CC BY-SA database. The user
  decided not to build lens correction for now.
- **Chromatic aberration needs no profiles.** *Lateral* chromatic aberration makes the
  red and blue images slightly larger or smaller than the green one. Edges then get
  red/cyan or blue/yellow fringes that grow towards the corners. That can be measured
  from the photo itself.

## Decision

1. **Model.** Each of red and blue is scaled about the photo's centre to line up with
   green, by `1 + a + b ρ²`. Here ρ is the distance from the centre as a share of the
   half-diagonal, so the numbers do not depend on resolution. `b` allows CA that grows
   faster than linearly towards the corners.
2. **Measure once, store the numbers.**
   - Turning the switch on runs `measure_chromatic_aberration` on the open photo's
     preview level nearest 2000 px or above, as an interactive job.
   - The recipe keeps the result as `chromaticAberration: {red: [a, b], blue: [a, b]}`.
   - Previews, thumbnails and the export all use the same numbers, and the edit stays
     pure data.
   - Turning the switch off removes the field. It is kept while on, even if nothing was
     found, so the switch stays on.
3. **The measurement** (`renderer::chromatic`):
   - It works on log brightness per channel, band-passed so each region's own colour
     drops out.
   - Gauss–Newton fits four numbers per channel: the two scale terms and a constant
     shift. The shift comes from half-size RAW decoding, which takes red and blue from
     different photosites. It is measured so it does not bias the scale, but it is not
     applied.
   - Every edge pixel constrains the fit along its own gradient, so edges in any
     direction help.
   - Only 32 px tiles with clear edges count:
     - no clipped pixels;
     - the channel follows green (correlation ≥ 0.9);
     - each tile gets its own brightness ratio, so an edge between two colours, where
       the channels step by different amounts, does not read as a shifted edge.
   - Tiles that still disagree are down-weighted (Huber).
4. **Declining.**
   - Two halves of the photo (alternate tiles, each covering the whole frame) are
     fitted separately. They must agree within 0.2 px plus a third of the shift.
   - The photo also needs at least 16 usable tiles per half.
   - Otherwise the answer is "not enough clear edges", and the switch stays off.
   - A scene made only of colour edges declines rather than inventing fringes (tested).
5. **Correction:** in the framing resample (ADRs 0032, 0034), with red and blue
   sampled at their scaled positions (bilinear) and green unchanged.
   - It is about the photo's centre (the lens's), before crop, straighten and
     perspective.
   - The framed image stays cached, so dragging other controls costs nothing extra.
   - With only this correction on, the resample replaces the plain copy.
6. **Recipe version 13.** Older recipes have no correction, and render as before.
7. **UI, as designed:** a switch row in "Perspective & lens", with its label and "Cleans
   colour fringing on edges". While measuring it reads "Measuring…". It then says
   "Colour fringing removed", "Hardly any fringing found" (under ¼ px) or "Not enough
   clear edges to measure", for three seconds. The Lens correction switch is left out.

Also fixed: the header's "Edited" marker and Reset now count crop, straighten,
perspective and this correction as edits. Before, a crop-only edit read as unedited.

## Measurements

See docs/PERFORMANCE.md §24.

## Consequences

- Only lateral CA is removed. The purple fringes of *axial* CA (out-of-focus
  highlights and backlit branches) remain. A defringe control would handle those.
- The numbers are measured per photo. Presets (not built yet) must not copy them; a
  preset could carry "on" and measure again.
- X-Trans (Fuji) previews come from LibRaw's half-size mode like Bayer files. The
  measurement worked well on the X-T3 sample (about 4 and 8 px of CA at full size,
  visibly removed), but it is one sample.
