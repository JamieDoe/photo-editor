# ADR 0083: Chromatic aberration from the lens's own profile

- Status: Accepted
- Date: 2026-10-14

## Context

ADR 0075 applies the distortion and vignetting a Sony camera records in each raw file.
The same files record the lens's lateral chromatic aberration (tag `0x7035`), which
ADR 0075 left out: the "Remove chromatic aberration" switch (ADR 0035) measures
fringing from the photo, and applying both would correct it twice.

What the tag holds, in both Sony fixtures: a count (32), then 16 values for red and 16
for blue, at the same evenly spaced radii as the distortion curve. As darktable reads
them, each is `1 + v / 2^21`: red's (or blue's) radius over green's.

## How the reading was checked

Against the fringing the app measures from the photos themselves (ADR 0035), at the
corners of the full-size image:

| | Profile | Measured | Measured after the profile's correction |
|---|---|---|---|
| A7R IV (24–70 at 24 mm), red | +732 ppm (+4.2 px) | +1053 ppm (+6.0 px) | +1.7 px |
| A7R IV, blue | −244 ppm (−1.4 px) | −112 ppm (−0.6 px) | +0.6 px |
| A7 III, red | +305 ppm | +420 ppm (+1.5 px) | +0.5 px |
| A7 III, blue | −122 ppm | −251 ppm (−0.9 px) | −0.4 px |

The signs and the sizes agree; a scale of `2^-20` would double the profile's numbers.
The profile removes about two thirds of the fringing; the rest is within what the
measurement itself varies by between photos.

## Decision

1. **Read it** (`raw::lens`): `LensProfile::chromatic_aberration`, red's and blue's
   curves. A curve that doesn't fit the others' knots is left out, not the whole
   profile; on its own it is a profile.
2. **Apply it with the profile** (`renderer::lens`): in the framing resample, red and
   blue are sampled where the lens put them (`LensAt::fringe_points`), about the
   source's centre, after distortion is undone. The curves are read at the sensor
   radius; they change too slowly for the distortion to matter.
3. **One correction, never two:** while "Remove chromatic aberration" is on, its
   measurement replaces the profile's. It is measured on the photo as decoded, so it
   is the whole correction. The switch stays useful for lenses without a profile, and
   for those whose profile leaves fringes.
4. **Older edits render as they did:** recipe version 32. Recipes before it that
   apply a profile to an edit are marked `profileWithoutChromaticAberration`, and
   their profile corrects distortion and vignetting only. Photos never edited get the
   whole profile. Turning "Lens correction" off or on again clears the mark.
   - **It belongs to the photo:** in the "Crop, geometry and lens" group for copy and
     paste; presets keep the photo's own.
5. **No new control:** the design's "Lens correction" switch now covers it, as
   Lightroom's built-in profiles do.

## Consequences

- Sony photos open with their lens's fringing corrected, without the measurement's
  quarter of a second, and in previews, thumbnails and exports alike.
- **Cost:** framing a 15 MP view with the profile takes 35–45 ms more (140–150 →
  177–196 ms), once when the framing changes (PERFORMANCE §59). Slider changes cost
  nothing extra.
- **Next:** other makers' profiles (DNG `WarpRectilinear`'s per-plane coefficients,
  Fujifilm, Olympus/OM, Panasonic), each checked against a file that has them.
