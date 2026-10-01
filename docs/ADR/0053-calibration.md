# ADR 0053: Calibration

- Status: Accepted (Lightroom parity, milestone 3)
- Date: 2026-10-01

## Context

Lightroom's Calibration panel has seven sliders:

- Shadows Tint (green to magenta in the darkest tones);
- each of the red, green and blue primaries' Hue and Saturation.

They change the camera profile's primaries, so each one moves every colour that
contains that primary: one slider changes the whole palette at once. Film-look
presets lean on them, for example Blue Hue −100 with Blue Saturation up for "teal and
orange". Presets imported them as "left out" (ADRs 0051, 0052). The design has no
Calibration panel.

## Decision

1. **A recipe field `calibration`** (recipe v23, written only while a slider is set):
   `shadowTint`, then `redHue`, `redSaturation`, `greenHue`, `greenSaturation`,
   `blueHue` and `blueSaturation`, each −100..100.
   - It is copied as its own group, "Calibration" (on by default), as in Lightroom's
     Copy Settings.
2. **Where it runs:** on scene values, after Vignette and before Contrast and the base
   look, where a camera profile's matrix would.
   - The stages before it apply a scalar gain to each pixel (exposure, tone, detail,
     vignette), which commutes with a matrix, so placing it later gives the same result
     as applying it straight after white balance. The exceptions are dehaze and the
     masks' Warmth.
   - Because it runs before Saturation, black and white takes its colour out again,
     as in Lightroom.
3. **The primaries are one 3×3 matrix:** `I + Σ shiftᵢ (eᵢ − ⅓)ᵀ`.
   - A primary's shift is its Hue times a neighbouring primary's chroma, plus its
     Saturation times its own chroma. A primary's chroma is the primary less the grey
     of its luminance, so no shift changes luminance.
   - Positive Hue moves toward the next primary: red toward green (orange), green
     toward blue (cyan), blue toward red (purple). Negative Hue moves the other way.
   - A colour takes each primary's shift in proportion to how far that channel sits
     above the colour's channel average. Greys and white never move, and luminance
     never changes (before clipping at 0).
   - Each primary has its own strength, measured so that at ±100 a mid colour of it
     turns about 20° (Oklab hue) and its chroma changes by about ×1.25 and ×0.75.
     - Hue: red 0.25, green 0.7, blue 0.35.
     - Saturation: red 0.4, green 0.35, blue 0.45.
     - Equal strengths turned green about a third as far as red.
   - Like Lightroom's, each slider spills over onto other colours. Blue Saturation
     +100 also strengthens and warms skin.
4. **Approaches rejected, by measurement:**
   - **Moving each primary in Oklab and rescaling the matrix columns so white stays
     white.** The rescaling undid most of the saturation change: +100 gave mid blues
     only ×1.11, and a stronger scale levels off near ×1.08.
   - **Weighting by distance from luminance rather than from the channel average.**
     Green is most of luminance, so green colours hardly moved: Green Hue +100 turned
     a mid green only 3°.
5. **Shadow Tint uses colour grading's table (ADR 0052).** It is a shadows wheel at
   magenta (hue 300) or green (hue 120), with strength 0.6 × |tint| and Blending 25,
   which narrows the shadows. The tint stays in the darkest tones and leaves
   highlights neutral.
6. **Lightroom presets import it:** `ShadowTint`, `RedHue`, `RedSaturation`,
   `GreenHue`, `GreenSaturation`, `BlueHue` and `BlueSaturation`, from `.xmp` and
   `.lrtemplate` alike. Calibration is no longer listed as left out.
   - The values are taken as they are. How closely ±100 matches Lightroom's look is
     the next milestone's work (per-slider calibration against reference exports).
7. **UI:** a "Calibration" section after Colour grading, closed by default.
   - The sliders sit under the headings Shadows, Red primary, Green primary and Blue
     primary, the groups given by the engine.
   - Each slider's track shows its colours, as Lightroom's do: green to magenta;
     magenta to orange; yellow to cyan; cyan to purple; grey to the primary for
     saturation.
8. **Tests:**
   - **Rust:** nothing set changes nothing; greys stay grey at any setting; luminance
     never changes; each Hue turns its own colours the way its slider says; Saturation
     strengthens its own colours most; Shadow Tint tints shadows and spares highlights;
     sanitising.
   - **Rendering:** the fast and reference renderers agree with Calibration on. A
     render with Blue Hue −100 keeps greys within one level and turns a blue toward
     cyan.
   - **Import:** Lightroom import of Calibration from `.xmp` and `.lrtemplate`.
   - **Frontend:** written only while set.
   - **Release self-test, Nikon Z 6:** the primaries move colours by 1.5 levels on
     average, with mean brightness within 0.3 of the original. Shadow Tint +100 makes
     the frame 2.9 levels more magenta.
9. **Performance** (PERFORMANCE §36): the matrix adds about 1 ms to a 1516×1010
   render and Shadow Tint about 2.4 ms more. A Shadow Tint table over the square root
   of luminance (no cube root) saved about 0.4 ms, within the machine's noise, and was
   not kept.
10. **Still left out on import:** masks and healing, Lens Corrections, crop and
    transform, the profile, colour noise reduction.

## Deviations from the design

The design has no Calibration. Recorded in ADR 0016: the section, built from the
design's tokens and sliders, with a new icon (three overlapping primaries).
