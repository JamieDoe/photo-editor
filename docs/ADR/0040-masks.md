# ADR 0040: Masks, starting with the linear gradient

- Status: Accepted (Phase 6, milestone 1)
- Date: 2026-09-30

## Context

Phase 6 brings local editing: brush, linear and radial gradients, combining masks,
feathering and inversion (`docs/PRODUCT.md` §5).

The design has a Masks tool:
- a Masks button in the photo toolbar opens a mask toolbar (the masks, Add, a
  show-overlay switch, Done);
- the photo shows the active mask tinted in the accent colour, and a gradient's lines;
- a Selective panel section lists the masks and holds the active one's Exposure,
  Warmth and Clarity.

The product requires one mask system: the renderer must not care how a mask was made
(manual or AI).

## Decision

1. **Data:** the recipe gains `masks: [Mask]` (recipe v17), in the order made.
   - A `Mask` is an `id` (to tell masks apart while editing; not rendered), a `shape`
     and its `adjustments`.
   - The shape is tagged by kind. The first kind is `linear`, with `start` and `end`
     points: fully on at the start line, fading (smoothstep) to nothing at the end
     line, both lines perpendicular to start → end.
   - The recipe no longer derives `Copy`: masks are a list, and brush strokes will be
     longer lists.
2. **Coordinates:** shapes are in the frame the crop is in (the photo turned,
   straightened and perspective corrected, before cropping), as fractions of its width
   and height.
   - Cropping does not move a mask over the picture (tested: a cropped render equals
     the middle of the uncropped one).
   - Rotate and flip turn masks with the crop (ADR 0039).
   - Straighten and perspective do not move masks, so a gradient stays where it was on
     screen. Gradients are soft, so this rarely matters; a brush may need it later.
   - Distances are measured in the frame's pixels, so a gradient's lines are
     perpendicular on screen whatever the photo's shape.
3. **Rendering:** the renderer asks a shape only how much it covers each pixel (0..1).
   A gradient's coverage is computed analytically per pixel, with no mask images.
   - **Exposure and Warmth** are per-pixel gains right after the global white balance
     and exposure (scene-linear, so the base look still rolls off highlights they push
     up). Warmth uses the Temperature scale's gains.
   - **Highlights and Shadows** see the local exposure. The tone stage reads its
     surroundings map at the pixel's brightness without the mask, then adds the
     mask's stops back. A darkened sky is treated as dark, not as it was.
   - **Clarity** adds to the global amount, per pixel, in the detail stage, which now
     runs when only a mask has Clarity.
   - Masks without adjustments render nothing, and the plan is the same as without
     them.
   - Cost at 1516×1010: one mask adds about 1.3–2.1 ms, three masks about 2.1–2.7 ms,
     a Clarity mask about 2 ms (docs/PERFORMANCE.md §28).
4. **UI, as designed:**
   - **Photo toolbar:** a Masks button after Crop. Only one tool is open at a time.
   - **Mask toolbar:** the masks as chips (kind colour dot and name), "Add" and the kinds
     to add, the overlay switch (eye), and Done. Enter or Escape finishes; Delete
     removes the active mask.
   - **On the photo:** the coverage tinted with the accent at 42 %, drawn as an SVG
     gradient with the renderer's smoothstep stops, plus the dashed start and end lines
     and the solid centre line.
   - **Selective section:** after Geometry, with the mask count and edited dot in the
     header. It has rows (dot, name, kind, delete), the active mask's Exposure (±2 EV),
     Warmth (on the blue–amber track) and Clarity in a card, and the Add tiles.
   - Mask sliders come from the engine (`EngineInfo.mask`), like the others.
   - New linear gradients start over the shown picture: full at the top, fading out
     just past the middle, as for a sky.
   - A mask counts as an edit (header marker, saved), even before its sliders move.
   - The self-test renders −1 EV over the top of the Nikon sample: the top rows get 43
     levels darker, and the bottom rows are unchanged to the level.

## Deviations from the design

Recorded in ADR 0016:
- only Linear is offered (Radial and Brush follow; Subject and Sky need on-device AI);
- start and end handles on the dashed lines, besides the centre one;
- the empty-state text: the design's mentions detected skies and subjects.

## Consequences

- Radial gradients and brushes add shape kinds. Everything downstream (stages,
  panel, overlay switch) is shared.
- Brushes rasterise their strokes into a cached coverage map (ADR 0042).
- Masks are in a frame that straighten and perspective change. If that shows with
  brushes, masks can move to source coordinates through the geometry's mapping.
