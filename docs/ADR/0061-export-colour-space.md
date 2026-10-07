# ADR 0061: Export colour space (sRGB, Display P3, Adobe RGB)

- Status: Accepted
- Date: 2026-10-01

## Context

The design's export dialog has a **Colour space** row: sRGB, Display P3 and Adobe RGB.
It was hidden until the gamut was settled (ADR 0059). The photographer chose option 2
of ADR 0060: keep editing in sRGB, bring the colours beyond it in softly (part 1), and
offer the row as a correctly converted, tagged export (part 2, this ADR).

A P3 or Adobe RGB file from this pipeline therefore holds the same colours as the sRGB
file. It is still worth offering:

- print labs and print workflows often ask for Adobe RGB;
- a wide-gamut file opens unchanged in tools that work in that space;
- the file is ready for a wider working space later (ADR 0060, option 1), which would
  fill the same row with more colour without changing the dialog.

## Decision

1. **The row: sRGB, Display P3, Adobe RGB.**
   - Remembered as `export.colourSpace` (`srgb`, `displayP3`, `adobeRgb`), default
     sRGB. An unknown value from a newer version reads as sRGB.
   - **Presets**, as the design has them: Web and Social are sRGB; Full quality (the
     16-bit TIFF) is Adobe RGB. Changing the row by hand deselects the preset.
   - **Hints say what the file holds:** P3 and Adobe RGB are "tagged for" wide-gamut
     screens and print workflows, and "the colours are sRGB's". The dialog does not
     promise colours the pipeline cannot hold.
   - The export batch carries it, as does the single-photo export.
2. **The conversion** (`export::colour::convert`), after resize and sharpening:
   - **Render at 16 bits.** Any space other than sRGB renders as 16-bit RGB, so an
     8-bit JPEG or PNG is rounded once, after conversion, not twice.
   - **Decode** the sRGB-encoded samples to linear light (a table per input depth).
   - **Matrix** to the space's primaries: sRGB to XYZ, then XYZ to the target. All
     three spaces share the D65 white, so no white adaptation is needed. Results are
     clipped to 0–1: sRGB colours lie inside both wider spaces, so only rounding is
     clipped.
   - **Encode** with the space's own curve: the sRGB curve for sRGB and Display P3,
     gamma 563/256 (≈2.2) for Adobe RGB.
   - sRGB to sRGB only changes the depth (16 to 8 bits where the format needs it).
3. **Profiles** (`export::icc`): the ADR 0057 generator is generalised from sRGB to any
   space by primaries, white and curve.
   - ICC v2.1 matrix/TRC display profiles: colorants are adapted to D50 with Bradford,
     and each curve is a `curv` tag (the sRGB curve as a 1,024-point table, or a single
     gamma value).
   - **Display P3:** primaries (0.680, 0.320), (0.265, 0.690), (0.150, 0.060) with the
     sRGB curve.
   - **Adobe RGB (1998):** primaries (0.64, 0.33), (0.21, 0.71), (0.15, 0.06) with
     gamma 563/256.
4. **Every format is tagged:**
   - **JPEG:** an `ICC_PROFILE` APP2 segment, inserted after SOI and the JFIF APP0
     segment. An sRGB JPEG stays untagged, as before (sRGB is what an untagged JPEG
     means).
   - **PNG:** an `iCCP` chunk for P3 and Adobe RGB; sRGB keeps its `sRGB` chunk.
   - **TIFF:** the space's profile (tag 34675), replacing the sRGB profile it always
     carried (ADR 0057).
5. **Tests:**
   - **Export crate:**
     - the generated colorants match the published ones (sRGB to 2e-4, P3);
     - every space makes a well-formed profile;
     - greys keep their values in P3, and white and black stay put in Adobe RGB;
     - sRGB's pure red sits inside both wider spaces, and sRGB's pure green in Adobe
       RGB is the well-known (144, 255, 60);
     - the matrices round-trip;
     - 16-bit in and out;
     - a JPEG gets its profile after JFIF, and every format carries the right profile.
   - **Settings:** a stored choice loads, and an unknown one reads as sRGB.
   - **Release self-test:** the test photo is exported through the queue as a 1,350 px
     JPEG in each space (three different files) and as Full quality's 16-bit Adobe RGB
     TIFF.
6. **Performance** (PERFORMANCE §42): a full-size JPEG export of the 24.5 MP Nikon Z 6
   takes about 0.2 s longer in P3 or Adobe RGB (about 2.0 s against 1.8 s). The time
   goes on the 16-bit render and the conversion.

## Deviations from the design

Recorded in ADR 0016:

- The wider spaces hold sRGB's colours (ADR 0060, option 2), and the hints say so.

## Consequences

- **A wide working space** (ADR 0060, option 1) would replace the conversion's source,
  not the row or the profiles: the matrix would start from the working space instead
  of sRGB, and sRGB output would need real gamut mapping.
- **Display P3 on screen:** the editor still shows sRGB. A P3 display transform
  belongs with option 1.
- **sRGB JPEGs stay untagged:** some print labs want every file tagged. If that comes
  up, the sRGB profile can be embedded as well.
