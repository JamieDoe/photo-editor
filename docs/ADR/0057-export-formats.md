# ADR 0057: TIFF and PNG export

- Status: Accepted
- Date: 2026-10-01

## Context

Exports were JPEG only (ADRs 0006, 0050). PRODUCT §3.2 lists JPEG, PNG and TIFF output
for V1.

The design's export dialog has a **Format** row (JPEG, HEIC, TIFF, PNG), with Quality
shown only for the lossy formats. Its **Full quality** preset is a TIFF at the original
size.

A TIFF for printing or further editing should keep more than 8 bits. The renderer only
produced 8-bit images.

## Decision

1. **Three formats.**
   - **JPEG** as before.
   - **TIFF:** 16-bit RGB, compressed losslessly (Deflate with the horizontal
     predictor), with an embedded sRGB ICC profile, marked 300 ppi.
   - **PNG:** 8-bit RGB, lossless, with the `sRGB` chunk.
   - **HEIC is not offered.** Encoding it needs an HEVC encoder, whose patents and
     licensing need a product decision first.
2. **A 16-bit output path.**
   - `PixelFormat::Rgb16` stores native-endian 16-bit samples. `OutputImage` sizes
     itself by bytes per pixel.
   - The CPU renderer writes 16-bit pixels with the exact sRGB curve rather than a
     table: the curve is steepest near black, where no practical table is finer than
     16 bits. This costs about 33 ms more on a 24 MP render (PERFORMANCE §39).
   - A 16-bit export renders 16-bit, resizes in linear light at 16 bits (decoding
     through a 65,536-entry table and encoding each sample exactly), and encodes.
     JPEG refuses 16-bit input.
3. **The sRGB profile is generated** (`export::icc`), not shipped as a file, so its
   terms are ours. It is an ICC version 2.1 matrix/TRC display profile:
   - sRGB's primaries adapted to D50 (Bradford);
   - D65 as the media white;
   - the sRGB curve sampled at 1,024 points, stored once and shared by the three
     channels;
   - a fixed date, so it is the same bytes every time.
4. **Dependencies:**
   - `tiff` 0.11 (MIT, pure Rust, the image-rs project), with only Deflate. It adds
     one new crate, `quick-error` (MIT/Apache-2.0); `flate2` and `half` were already
     in the build.
   - `png` was already a workspace dependency.
5. **Settings:** `export.format` (`jpeg`, `tiff`, `png`), default JPEG.
   - A format this version does not know (written by a newer one) reads as JPEG,
     rather than failing the whole settings file. An existing test already expected
     that.
   - The export batch carries the format.
   - Files are named with the format's extension. The single-photo save panel
     filters for it and adds it when missing.
6. **Dialog:**
   - The design's Format row: JPEG, TIFF and PNG, each with a hint.
   - Quality shows for JPEG only.
   - Full quality is now a TIFF at the original size, as designed (it was a JPEG at
     quality 95).
   - A preset matches its format, size and (for JPEG) quality.
7. **Each exported file reports its size in bytes** (`ExportedFileDto.bytes`).
8. **Tests:**
   - **Export crate:**
     - TIFF keeps every 16-bit value and carries the profile (read back with the
       `tiff` decoder);
     - PNG is lossless and has the sRGB chunk;
     - formats and their extensions; JPEG refuses 16-bit;
     - 16-bit resizing keeps a flat colour, and averages a black and white checker
       to linear mid grey;
     - the profile is well formed: size, signature and required tags, colorants that
       sum to D50, the curve's midpoint.
   - **Renderer:** 16-bit output matches 8-bit within rounding and keeps more than
     four times the levels on a dark ramp.
   - **Settings:** a stored TIFF choice loads; an unknown format reads as JPEG.
   - **Release self-test, Nikon Z 6, 1,350 px, through the real export queue:**

     | Format | File size | Time |
     |---|---|---|
     | JPEG | 267 KB | — |
     | PNG | 1.5 MB | 0.62 s |
     | 16-bit TIFF | 5.9 MB | 0.56 s |

     The PNG and TIFF times include decoding the raw file. The JPEG time is not
     measured separately here.

## Deviations from the design

Recorded in ADR 0016:

- HEIC is left out of the Format row.
- The Print preset, Colour space, Sharpen for, the metadata switches and the estimate
  are still left out.

## Consequences

- **Colour spaces:** wider spaces (Display P3, Adobe RGB) need the renderer's output
  transform and a profile per space. The profile generator takes other primaries
  easily.
- **JPEG and PNG** carry no ICC profile. Untagged JPEG is read as sRGB everywhere,
  and PNG has its sRGB chunk.
