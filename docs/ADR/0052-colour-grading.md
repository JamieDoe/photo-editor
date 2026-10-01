# ADR 0052: Colour grading

- Status: Accepted (Lightroom parity, milestone 2)
- Date: 2026-10-01

## Context

Color Grading, and the older Split Toning it replaced, is the most visible look
Lightroom presets lost on import (ADR 0051). It is the basis of most film, moody and
toned black and white presets.

Lightroom's version has four wheels (Shadows, Midtones, Highlights, Global). Each sets
a hue, a saturation and a luminance. Blending sets how much the ranges overlap, and
Balance moves the point between shadows and highlights. The design has no colour
grading.

## Decision

1. **A recipe field `colourGrading`** (recipe v22, written only while a wheel is set),
   with four wheels and the ranges' Blending (0..100, default 50) and Balance
   (−100..100).
   - A wheel's hue is in degrees on the colour wheel (0 red, 120 green, 240 blue). Its
     strength runs 0..100 and its luminance −100..100.
   - It is in the Colour copy group, now "Colour and grading".
2. **Where it runs:** after Saturation, before Grain, so black and white photos can be
   toned.
3. **The tints are worked out in Oklab,** so a tint's strength looks alike at any hue.
   - A hue's direction is that of the fully saturated colour at it.
   - A pixel's lightness gives it weights for the ranges: shadows `(1 − x)^k`,
     highlights `x^k`, midtones the rest.
   - `x` is lightness bent so that the point where shadows give way to highlights
     lands in the middle. That point is middle grey (18% luminance) at Balance 0, so
     a photo's typical tones count as midtones; Balance moves it.
   - Blending sets `k`: 6 at 0, 3 at 50, 1.5 at 100. At the default, a tone halfway
     from middle grey to white takes about a fifth of the highlights' tint. (The first
     version put the point at Oklab L 0.5 with `k` 2, and a highlight tint spread
     over most of a photo as a cast.)
   - At full strength a wheel moves chroma by 0.10, and luminance at ±100 moves
     lightness by 0.12.
4. **Applied through a table over lightness.** What grading does to a grey of each
   lightness (1,025 steps) is tabulated as a brightness gain for all three channels,
   capped at 4×, plus the tint, added. A pixel is graded by its lightness (the cube
   root of its luminance).
   - This is exact for greys: toned black and white is graded as Oklab would.
   - Colours get the same tint while keeping their own colour.
   - Each pixel needs no Oklab round trip.
   - The approaches were measured on a 1516×1010 render, against about 3.3 ms
     ungraded:
     - Oklab per pixel: +11 ms.
     - A shift table: +5.9 ms.
     - A fast cube root as well: +5.3 ms.
     - Gains plus tint: +1.4–1.7 ms.
   - The first gains-only table divided by a near-black grey, which would have
     flared dark saturated colours. Capping the gain and adding the tint avoids it
     (tested on a deep red).
5. **Lightroom presets import it:**
   - `ColorGrade{Shadow,Midtone,Highlight,Global}{Hue,Sat,Lum}` fill the four wheels;
   - `ColorGradeBlending` gives Blending, and `SplitToningBalance` gives Balance;
   - the older `SplitToning{Shadow,Highlight}{Hue,Saturation}` fill the shadows' and
     highlights' wheels when Color Grading is absent.
6. **UI:** a "Colour grading" section after Detail.
   - A switch picks the range: Shadows, Midtones, Highlights or Global. A dot marks
     ranges that are set.
   - One 128 px wheel: drag or click the point. Its direction is the hue, its distance
     from the centre the strength. Double-click clears it; arrow keys turn and
     strengthen it.
   - A readout shows hue and strength, then the range's Luminance, and the sections'
     Blending and Balance.
7. **Tests:**
   - **Rust:** Oklab round trips; nothing set changes nothing; teal shadows and orange
     highlights land on their ranges; brightness keeps greys grey; Balance and
     Blending shape the weights; sanitising; the fast cube root (< 2e-6 relative
     error); greys graded exactly as in Oklab; deep reds keep their colour; the table
     against direct gains.
   - **Rendering:** a toned black and white render (blue shadows, warm highlights);
     the fast and reference renderers agree with grading on.
   - **Import:** Lightroom import of Color Grading and of Split Toning.
   - **Frontend:** wheel points and ranges.
   - **Release self-test, Nikon Z 6:** black and white measures channel spread 0;
     split-toned, 21.5 (rendered in 7.3 ms).
8. **Still left out on import:** Calibration (the next milestone), masks and healing,
   Lens Corrections, crop and transform, the profile, colour noise reduction.

## Deviations from the design

The design has no colour grading. Recorded in ADR 0016: the section, built from the
design's tokens and controls.
