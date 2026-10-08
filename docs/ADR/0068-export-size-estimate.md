# ADR 0068: Export size estimate

- Status: Accepted (the size estimate; the watermark follows as its own change)
- Date: 2026-10-08

## Context

The design's export dialog shows the files' expected size beside Export ("≈ 4.2 MB",
"each" for several). It was hidden (ADR 0016) until it could be built. File sizes
depend on the photo's content, the edit, the format, quality, size, sharpening and
colour space. A formula of pixels alone (as the design's prototype uses) can be off
by several times between a clear sky and a forest.

## Decision

1. **Estimated from a sample of the photo itself** (`Engine::estimate_export`):
   - a preview level of about 1,024 px (by what the crop keeps) is rendered with the
     photo's current edit;
   - it's sharpened, converted to the colour space and encoded exactly as the export
     would be;
   - its size is then scaled to the export's pixel count (after crop and the
     long-edge limit).
   - It runs on the interactive lane in tens of milliseconds (57 ms for the Nikon Z 6
     in the release self-test), and a newer request cancels one still running.
2. **Scaling, fitted to real exports** (`export::estimate`): files don't grow in
   proportion to their pixels. A larger image of the same scene has less detail per
   pixel, and the sample packs a little more detail than the export.
   - **The model:** `bytes = factor × sample bytes × (pixels / sample pixels) ^
     exponent`, per format.
   - **The fit:** `bench --estimate` exports each of the six camera fixtures at
     1,350 px, 2,048 px and full size, in JPEG at quality 85 and 95, PNG and 16-bit
     TIFF, and records the samples. A least-squares fit of the logs gives:

     | Format | Exponent | Factor |
     |---|---|---|
     | JPEG (85 and 95 together) | 0.903 | 0.900 |
     | PNG | 0.959 | 0.937 |
     | 16-bit TIFF | 0.994 | 0.985 |

   - **Accuracy** on those exports (PERFORMANCE §49): the median error is within
     about 4 % for every format and size. The worst cases are full-size JPEGs (up to
     26 %, the Canon EOS R6) and PNGs (14 %); TIFF stays within 4 %.
   - The fit is measured on the same six photos it was made from. Photos very unlike
     them may err more, which "≈" allows for.
3. **In the dialog:**
   - the estimate sits beside Cancel in the design's monospaced grey, written as the
     design does: one decimal under 10 MB, whole megabytes from 10, kilobytes under
     one, "each" for several photos;
   - it's asked for 150 ms after the choices settle, and the newest answer wins;
   - hovering it says it's estimated from a preview of the photo, and the size it
     will be written at.
   - **Several photos:** estimated from the open photo (the dialog's header photo),
     shown as "each".
   - **No open photo:** no estimate is shown.
   - Metadata (about 1 KB) isn't counted.
4. **Tests:**
   - **The model:** the same size gives the sample by its factor, and larger exports
     grow no faster than their pixels.
   - **Formatting:** megabytes, whole megabytes, kilobytes, and "each".
   - **Release self-test** (`exportSizeEstimate`): the estimate for the full-size JPEG
     export of the Nikon Z 6, with the same edit and settings, is within 30 % of the
     file made (3.7 % in practice), in under 2 s (57 ms).
   - **Dev mock:** the estimate shows in the footer and follows the preset.

## Deviations from the design

Recorded in ADR 0016: with several photos, the estimate is the open photo's, as
"each".

## Consequences

- **The watermark** (the design's third switch) is the remaining hidden part of the
  dialog, built separately.
- **Per-photo estimates** for a multi-photo export (a sum rather than "each") would
  need each photo decoded. They could come from thumbnails if wanted.
