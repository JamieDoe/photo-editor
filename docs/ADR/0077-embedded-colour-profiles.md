# ADR 0077: Embedded colour profiles

- Status: Accepted (completes ADR 0076's colour)
- Date: 2026-10-09

## Context

JPEG, PNG and TIFF files were read as sRGB whatever profile they embedded. Files in a
wider space open duller than they are, and are exported that way:
- **TIFFs from Lightroom and Photoshop** are often Adobe RGB or ProPhoto.
- **iPhone JPEGs** are Display P3.
- **Our own wide-gamut exports** (ADR 0062) are Display P3 or Adobe RGB.

## Decision

1. **Read the embedded profile:**
   - **JPEG:** its APP2 `ICC_PROFILE` segments, through zune-jpeg.
   - **PNG:** its `iCCP` chunk. An `sRGB` chunk means sRGB, whatever else the file has.
   - **TIFF:** tag 34675.
2. **Parse matrix/TRC profiles** (`image_core::icc`), the kind every RGB working space
   and display profile is.
   - **RGB:** per-channel tone curves and XYZ colorants adapted to D50.
   - **Grey:** one curve.
   - **Curves:** gamma, sampled tables and parametric curves of types 0 to 4.
   - **Name:** the profile's own, from `desc` (version 2) or `mluc` (version 4).
   - **Refused:** profiles built only from lookup tables (CMYK, Lab, most printer
     profiles), with the reason. Their files are read as sRGB, as before.
   - **No dependency:** the reader is about 250 lines and needs none.
3. **Convert:**
   - **Path:** encoded values go through the curves to linear light, through the
     colorants to XYZ (D50), then to linear sRGB with the D50-adapted (Bradford)
     sRGB matrix.
   - **Colours beyond sRGB** are brought in with the soft gamut compression raw files
     get (ADR 0060). That is the working space the renderer edits in.
   - **How:** per-channel tables (256 or 65,536 entries) and the matrix, in parallel
     rows.
   - **sRGB files are untouched:** a profile within 1 % of sRGB's colorants and 0.004
     of its curve is treated as sRGB and read exactly as before.
4. **Thumbnails** convert the reduced 8-bit preview, so the library matches the editor.

## How it was checked

A colour chart exported by the app in sRGB, Adobe RGB and Display P3 (ADR 0062
embeds their profiles), as JPEG, PNG and TIFF, then opened again:

| File | Adobe RGB against the sRGB export | Display P3 against the sRGB export |
|---|---|---|
| JPEG | 0.68 levels | 0.65 levels |
| PNG | 0.51 levels | 0.44 levels |
| TIFF (16-bit) | 0.30 levels | 0.31 levels |

These are the mean differences of the rendered previews. The files' stored values
differ by 7.1 levels on average: read as sRGB, they would be that far off.

Library thumbnails of the Adobe RGB JPEG and TIFF and the Display P3 PNG agree with the
sRGB file's thumbnail within 2 levels.

## Consequences

- **What changes:** wide-gamut photos keep their colours in the editor, the library
  and exports.
- **What stays the same:** sRGB files and files without a profile render exactly as
  before.
- **Cost:** about 26 ms more to decode a 24 MP 16-bit TIFF and 30 ms more for a JPEG
  (PERFORMANCE §57). Thumbnails cost the same.
- **Not covered:**
  - lookup-table profiles;
  - PNG `gAMA` and `cHRM` chunks without a profile (taken as sRGB);
  - raw files' embedded previews (cameras set to Adobe RGB). Their decoded raw data
    is unaffected.
