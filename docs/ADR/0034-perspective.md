# ADR 0034: Vertical and Horizontal perspective

- Status: Accepted (Phase 5, milestone 5)
- Date: 2026-09-30

## Context

The design's Geometry section has "More controls" headed "Perspective & lens". They
are Vertical and Horizontal sliders (−100..100), then two toggles: Lens correction
and Remove chromatic aberration.

Vertical fixes buildings that lean back when the camera was tilted up. Horizontal
fixes a wall shot at an angle. Both must keep what crop and straighten
(ADR 0032) guarantee: no empty corners, one cached resample, and a crop tool that
shows the whole corrected view.

## Decision

1. **Model: a homography in the existing geometry step.**
   - The corrected picture is the photo seen by a camera turned about its horizontal
     axis (Vertical) and its vertical axis (Horizontal). ±100 is 20°.
   - The focal length is the long edge (about a 50° field of view). The effect then
     depends only on the photo, not the render size, so previews and exports match.
   - The mapping is shifted so the view's centre is the photo's centre. Without the
     shift, 20° would move the picture a third of a frame.
   - Straighten applies after perspective, to the corrected picture. Mapping a view
     point undoes straighten first, then the perspective (`geometry::Mapping`).
2. **Signs:**
   - positive Vertical widens the top, fixing buildings that lean back;
   - positive Horizontal widens (and heightens) the right side.
3. **Fitting the crop.** A homography keeps straight lines straight, and the photo is
   convex, so a crop is inside when its four corners are.
   - `fit_crop_for` finds the largest centred rectangle of the aspect by binary search
     on its size (about 2 µs).
   - Without perspective it is the closed form of ADR 0032.
   - `effective_crop` pulls in crops that fall outside, as before.
4. **Resample:** bilinear through the mapping, as straighten already does. With
   perspective there is no whole-pixel copy path.
5. **Caching.** The framed-image cache key now includes Vertical and Horizontal. The
   maps built from the frame stay cached while other controls are dragged.
6. **Recipe version 12.**
   - `geometry` gains `vertical` and `horizontal`.
   - Version 11 geometry reads them as 0, so it renders exactly as before.
   - An older app sees version 12 as newer and leaves the edit alone.
7. **The crop tool follows** (`cropGeometry.ts`):
   - `viewToSource` and `fitCropFor` mirror the renderer. A shared test case pins
     them together: 6000×4000, straighten 3°, Vertical 40, Horizontal −20 → 0.84362.
   - Changing either slider keeps the crop in the same place relative to the view, as
     Straighten does.
   - The self-test checks that the renderer frames exactly the crop the UI computes.
8. **UI, as designed:** the Geometry section's "More controls", headed
   "Perspective & lens", has the Vertical and Horizontal sliders. It uses the same
   slider group as the other sections, so it opens by itself while one is edited.

## Not built here

- **Lens correction (profile-based)** needs lens profiles. The open source ones are
  Lensfun, an LGPL-3 native library with a CC BY-SA database. CLAUDE.md requires an
  explicit licensing decision before copyleft native code goes in, so the toggle is
  left out until that decision.
- **Remove chromatic aberration** can be algorithmic, with no profiles. It is the next
  milestone.
- Both toggles are listed as not built in ADR 0016.

## Measurements

- Resample, 1516×1010 frame, median of 40 over three rounds (machine load 20–30):
  - straighten only: 5.5–6.2 ms;
  - perspective plus straighten: 5.7–6.9 ms.
- Release self-test, Nikon Z 6: a perspective render at the fitted crop's pyramid
  level (5542×3692 kept) took 12.4 ms.

## Consequences

- Auto level (ADR 0033) measures the photo as shot, not the corrected picture.
  Strong perspective can still mislead it. Levelling after correcting would mean
  measuring the resampled frame, which is left for later.
- Guided "Upright"-style automatic perspective is not built. The line detector behind
  Auto level is a possible starting point.
