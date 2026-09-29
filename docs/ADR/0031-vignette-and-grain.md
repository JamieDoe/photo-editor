# ADR 0031: Vignette and Grain

- Status: Accepted (Phase 5, milestone 2)
- Date: 2026-09-30

## Context

The design's Detail section hides two finishing controls behind "More controls",
under a "Finishing" heading:

- **Vignette:** −100…100.
- **Grain:** 0…100.

Both depend on a pixel's place in the frame, so they must look the same in the
preview and the export.

## Decision

1. **Vignette** is a gain in stops, equal on R, G and B.
   - It follows an ellipse fitted to the frame. The effect starts a third of the way
     from the centre to the corners and is full at the corners (smoothstep).
   - −100 darkens the corners by 1.5 stops; +100 lightens them by 1 stop.
   - It runs on scene-referred values after the detail stage and before contrast and
     the base look, as lens falloff would. Lightened corners therefore still roll off
     through the look's shoulder.
   - Positions are fractions of the frame, so the result does not depend on render
     size. Once crop exists, it will follow the cropped frame.
2. **Grain** is a fixed film-grain pattern.
   - It is two layers of smooth value noise; the second is finer and rotated 30°, so
     the square lattice doesn't show (a single layer looked blocky at 1:1).
   - It is placed at 1500 cells across the long edge, about 4 px grains at 24 MP, in
     frame coordinates. A preview and an export sample the same grains (tested).
   - It changes brightness only (hue-stable gain) and is strongest in the midtones:
     up to 0.35 stop at 100, fading towards black and white.
   - It is the last stage, on the finished image.
3. **Recipe version 10** adds `vignette` and `grain`; older recipes read them as 0.

A tried alternative for Grain: a precomputed 1024² tile of blurred white noise,
looked up per pixel. It was slower (grain +6 ms instead of +3.4 ms) because of cache
traffic from a 4 MB tile across all threads, so the procedural noise stayed.

## Measurements

Nikon Z 6, 1516×1010, default recipe, median of 60 renders, three runs (load average
about 10):

| Recipe | Render |
|---|---|
| Default | 3.2 ms |
| + Vignette −40 | 4.2 ms (+1.0) |
| + Grain 40 | 6.6–6.8 ms (+3.4) |

- Both cost nothing when 0.
- Release self-test (every control, including both, dragged together, load average
  11): 21–28.5 fps, all checks passing.

## Consequences

- The Detail section is complete as designed.
- Vignette will need the crop rectangle when crop arrives (Phase 5).
- If Grain's cost matters on low-end hardware, per-row reuse of the lattice hashes, or
  a per-chunk blurred-noise plane, are the next steps.
