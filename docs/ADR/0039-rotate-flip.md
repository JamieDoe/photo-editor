# ADR 0039: Rotate 90° and flip

- Status: Accepted (Phase 5, final milestone)
- Date: 2026-09-30

## Context

`docs/PRODUCT.md` lists Rotate in Phase 5. Photos whose camera recorded no orientation
(or the wrong one) need quarter turns, and some pictures read better mirrored. The
decoder already applies the EXIF orientation; this is the photographer's own turn on
top.

The design has no rotate buttons.

## Decision

1. **The geometry gains `rotation` (quarter turns clockwise, 0–3) and `flip` (mirrored
   left to right, before the turns).**
   - They apply to the photo first. Straighten, perspective and the crop are all in
     the turned photo's frame, so the crop tool, the "no empty corners" fitting and the
     output size work unchanged in that frame.
   - `Mapping::source` undoes straighten, then perspective, then the turns and the
     flip, down to source pixels.
   - Chromatic aberration's red and blue sample points (ADR 0035) stay in source
     coordinates, about the lens's centre.
2. **Turning keeps the edit.** The UI (`turnGeometry`, `flipGeometry`) rewrites the
   geometry so the picture on screen just turns or mirrors.
   - **A quarter turn:**
     - the crop rotates with the picture;
     - Vertical and Horizontal perspective trade places (clockwise: Horizontal takes
       Vertical's value and Vertical takes minus Horizontal's);
     - Straighten stays, since turns about the centre commute;
     - a 4:5 or 16:9 crop keeps its shape but is marked Free, as those shapes are not
       offered turned.
   - **A flip:**
     - the crop, Straighten and Horizontal mirror;
     - the stored turns reverse, as the flip applies before them.
   - This is exact for straighten and the crop. For perspective it is exact when only
     one of Vertical and Horizontal is set; with both, the two camera turns apply in
     the other order, a small difference.
   - Auto level measures the photo as shot. Its angle is negated when the photo is
     flipped.
3. **Rendering:** a turned or flipped photo goes through the bilinear resample (ADR
   0032). At a quarter turn every sample falls on a pixel centre, so pixels are copied
   exactly (tested). It is cached like any framing.
   - Release self-test, Nikon Z 6: 11.7 ms for the turned 1010×1516 frame.
4. **Recipe v16:** `geometry.rotation` and `geometry.flip`. Older geometry is upright.
   The header's Edited marker counts a turn or flip.
5. **UI:**
   - Rotate left, Rotate right and Flip horizontally sit as icon buttons in the crop
     toolbar, after the aspect ratios. There is no vertical flip button: it is a flip
     and a half turn.
   - The toolbar now adapts to narrow stages (container queries): below 760 px the
     "Straighten" label goes and spacing tightens. Below 600 px the aspect ratios
     leave the toolbar, as the Geometry panel has them too. It fits the 800 px minimum
     window, which it did not before.

## Deviations from the design

Recorded in ADR 0016: the three buttons, and the narrow-stage toolbar.
