# ADR 0032: Crop and straighten, applied first

- Status: Accepted (Phase 5, milestone 3)
- Date: 2026-09-30

## Context

The design adds three pieces:

- **Photo toolbar:** a "Crop" button.
- **Crop mode:**
  - the outside dimmed, a thirds grid, corner and edge handles, and the size shown
    below;
  - a floating toolbar with aspect ratios (Original, Free, 1:1, 4:5, 16:9), a
    Straighten slider (−15…15°), Auto level, Reset and Done.
- **Geometry section:** the same aspect ratios and Straighten, Auto level, Crop, and
  "Perspective & lens" behind "More controls".

This is the first edit that changes the output's size and shape.

## Decision

1. **Recipe version 11** adds an optional `geometry`, written only when used.
   - `straighten`: degrees; positive turns the picture anticlockwise.
   - `crop`: a rectangle in the straightened view, in fractions of the photo's width
     and height, so it is independent of render size.
   - `aspect`: the shape the crop keeps while edited.
2. **Geometry first.** The renderer resamples the source into the output frame (rotate
   about the centre, then crop), and every stage runs on that frame.
   - The Vignette follows the crop, and Grain and the surroundings maps see the framed
     picture.
   - Without rotation, whole pixels are copied (no softening); with rotation, samples
     are bilinear.
   - The framed image is cached (one entry, keyed by source and geometry), so
     dragging any other control reuses it, and the maps keyed on it stay cached.
3. **No empty corners.** A crop is always pulled inside the rotated photo:
   `effective_crop` scales it about the centre until it fits.
   - Straightening fits the largest crop of the chosen shape (`fit_crop`).
   - In crop mode the tool shows the largest straightened area in the photo's own
     shape, and the crop is drawn within it. So areas along the edges outside that view
     can't be chosen, a small limitation compared with Lightroom.
   - Changing the angle keeps the crop in the same place within the view.
4. **Sizes:**
   - The engine picks the preview level by what the crop keeps, so a cropped preview is
     as sharp as an uncropped one.
   - Thumbnails of cropped photos decode enough pixels to fill the thumbnail.
   - Frames carry the full-resolution output size (the binary header grew to 28 bytes),
     and the viewer keeps one box per photo and crop from it.
5. **UI:**
   - Rust owns the geometry.
   - The crop tool's interaction maths (`cropGeometry.ts`) mirrors `fit_crop`, with a
     shared test case.
   - Straighten's range comes from `EngineInfo.straighten`.
   - The editor gained a view transform, so crop mode renders the whole straightened
     view while the recipe keeps the crop.
   - Enter or Escape finishes, like Done.

Not in this milestone:

- **Auto level:** horizon detection, next.
- **Perspective & lens:** later in Phase 5.
- **90° rotation and flips:** not in the design.

## Measurements (Nikon Z 6, 1516×1010, median of 40, three runs)

| Case | Render |
|---|---|
| Default recipe | 3.2 ms |
| Straightened and cropped, another slider dragged (cached frame) | 3.2 ms |
| Straighten dragged (resampled every frame) | 8.3 ms |

Release self-test, which crops through the real app: the frame reports the cropped
full-resolution size (for example 3032×2020, half of 6064×4040) and has the right
shape.

## Consequences

- Masks (Phase 6) will be defined in the framed image's coordinates, or mapped through
  the geometry.
- A GPU backend has to resample first too; the GPU spike reports geometry as
  unsupported.
