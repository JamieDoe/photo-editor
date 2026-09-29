# ADR 0027: Capture sharpening, on by default

- Status: Accepted (Phase 4, milestone 5)
- Date: 2026-09-29

## Context

The design's Detail section has Sharpening from 0 to 150, defaulting to **40**, like
Lightroom's capture sharpening for RAW files. Cameras sharpen their own JPEGs, so an
unsharpened RAW render looks soft next to them.

A non-zero default changes how every unedited photo looks in Edit. Edits saved before
this must keep their look (CLAUDE.md §13).

## Decision

1. **Sharpening is part of the Detail stage (ADR 0026).**
   - It is an unsharp mask on log2 luminance: a gain in stops, equal on R, G and B.
   - The blur is a 3×3 binomial ([1 2 1]², about a 0.7-px Gaussian) at the rendered
     size. Sharpness belongs to output pixels, so unlike Texture it does not scale
     with the image.
   - 100 multiplies one-pixel detail by 2.5.
   - Each pixel's change is limited to ±0.5 stop, which keeps halos along edges
     faint (tested on a hard 4-stop edge).
   - It fades out in deep shadows, where detail is mostly noise.
2. **Default 40, as in the design.**
   - Recipe version 7 adds `sharpening` with default 40, so an unedited photo, and
     any new edit, gets 40.
   - Recipes from versions 1–6 had no sharpening and migrate to 0. They render exactly
     as before, and since they now differ from the default, they are edits (the same
     approach as the Flat look in ADR 0022).
   - Thumbnails of unedited photos are still the camera's embedded previews (ADR
     0015), so they are unaffected.
3. **Identity means "at the defaults", not "all zero",** both in Rust and in the UI's
   `isIdentity`.
4. **Cost.** In the default recipe the sharpening blur is computed row by row from
   the luminance, with no blurred plane. Texture and Clarity work is skipped when
   they are zero.

## Measurements (Nikon Z 6, 1516×1010 interactive, three runs)

| Recipe | Render |
|---|---|
| Default, no sharpening | 1.6 ms |
| Default (sharpening 40), first version | 4.7–5.5 ms (two box blurs, plane copies) |
| Default (sharpening 40), final | 2.9–3.0 ms |

Bench recipe (Texture 20, Clarity 25, Sharpening 40):

- Detail stage: +3.4 to +3.9 ms; whole render 10.4 ms.
- Full-resolution render: 163–174 ms; export render: about 295 ms.

Release self-test, dragging every control:

- median render 10.3–12.3 ms, 95th percentile 12.5–14.4 ms;
- 30 fps on Nikon, Canon and Fuji.

## Consequences

- Unedited photos look crisper in Edit than before; saved edits don't change.
- If Lightroom-style Radius, Detail or Masking are ever needed, they belong behind
  "More controls". The design has none.
