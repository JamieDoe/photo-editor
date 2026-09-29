# ADR 0026: Texture and Clarity

- Status: Accepted (Phase 4, milestone 4)
- Date: 2026-09-29

## Context

The design's Detail section is Texture, Clarity, Sharpening and Noise reduction, with
Vignette and Grain behind "More controls". This milestone adds Texture and Clarity.

Both change local contrast, at different scales. Two properties are hard to get:

- **No halos:** a local-contrast boost normally glows along strong edges.
- **Preview matches export:** the effect must look the same at preview size and at
  full resolution.

They are also the renderer's first stage that needs full-resolution neighbourhoods.
Highlights and Shadows only needed a 256-px map.

## Decision

1. **One stage, `Stage::Detail`, after tone and before contrast.** It measures log2
   luminance of the source after white balance and exposure, like the tone map. It
   then applies a gain in stops, equal on R, G and B, so hues are kept.
2. **Texture = the pixel − a small blur.**
   - The blur is two box passes with a radius of 0.15 % of the long edge: 2 px at the
     1516-px preview, 9 px at 24 MP. The same detail is picked out relative to the
     picture at every size.
   - ±100 doubles or removes fine detail.
   - It fades out 6–10 stops below white, so deep-shadow noise isn't boosted.
3. **Clarity = the small blur − the edge-aware surroundings map (ADR 0023).**
   - The map's model is evaluated with the blurred value as its guide.
   - Where the map follows an edge, the band is zero, so there is no halo. This is
     tested with a hard 5-stop step, where each side stays within 0.25 stop.
   - In flat regions the band is the structure around the local mean.
   - Strength is 2× at +100, chosen on real photos: 1× was barely visible because
     the map keeps most edges. It fades off near white and in deep shadows.
   - Each gain is limited to ±1.5 stops.
4. **CPU backend:**
   - Chunks read the source rows around them (`RowSpan` now carries the source).
     Each chunk builds the luminance of its rows plus the blur's reach (2 × radius)
     either side, so it gives exactly what a whole-image blur gives. Tests show the
     chunked render matches a whole-image reference within one code.
   - Chunks are at least 8 × radius rows tall when this stage runs, so at full
     resolution the extra rows stay small.
   - Supporting pieces: per-thread scratch buffers, a range-reduced `fast_log2`
     (accurate to about 1e-6), and a gain lookup table.
5. **Recipe version 6** adds `texture` and `clarity`; older recipes read them as 0.
   The renderer version is unchanged, because existing recipes render the same.

## Measurements

Nikon Z 6, bench recipe (texture 20, clarity 25), three runs:

| Stage or render | First version | Final |
|---|---|---|
| Detail stage, 1516×1010 interactive | +3.2 ms | +3.1 ms |
| Whole bench recipe, interactive | — | 9.5 ms |
| Full-resolution render (24 MP) | 202 ms | 150–158 ms (was ~70 ms without the stage) |
| Export render | 357 ms | 275 ms |

Final means after taller chunks and the gain lookup table.

Release self-test, which changes exposure, every Light and Colour control, Texture
and Clarity on every frame (the worst case, since the map is rebuilt each frame):

- median render 9.7–12.3 ms, 95th percentile 13–19 ms;
- 30 fps preview on Nikon, Canon and Fuji.

## Consequences

- Sharpening and Noise reduction will reuse the row-apron machinery.
- If interactive cost matters on low-end machines, the next step is caching the
  blurred plane between frames while exposure and white balance are unchanged, as
  the map already is.
- The GPU spike does not implement the stage; it reports it as unsupported.
