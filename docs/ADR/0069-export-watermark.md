# ADR 0069: Export watermark

- Status: Accepted
- Date: 2026-10-08

## Context

The design's export dialog has a third switch tile, **Watermark**, beside Keep
metadata and Strip location. It shows only the switch, not what the watermark says or
where it goes. Photographers sharing online commonly put a short line of text in a
corner ("© 2026 Name"). This was the last part of the dialog hidden (ADR 0016).

Exports are made in Rust (the queue runs without the UI), which has no text renderer.
A font crate and a font file would add a dependency and a download.

## Decision

1. **Text, drawn by the UI, laid on by Rust:**
   - **Drawing:** when an export starts, the UI draws the text once on a canvas in its
     own typeface (Geist, semibold, 160 px): white, with a soft dark shadow so it
     reads on light and dark photos alike. It's sent with the batch as an RGBA PNG
     (about 75–90 KB).
   - **Laying on** (`export::watermark`):
     - the PNG is decoded once per batch and shared by every photo;
     - each photo gets the drawing scaled so its height is a share of the photo's
       short edge: 3.5 % (small), 5.5 % (medium) or 8.5 % (large);
     - it's placed at a 3 % margin and blended at 70 %;
     - it's averaged over 2 × 2 samples per pixel, so it stays smooth when scaled
       down;
     - a line too long for the photo is shrunk to fit between the margins.
   - **When:** after resizing and sharpening, so its edges stay as drawn; before the
     colour space conversion (it is sRGB white); 8- and 16-bit alike.
   - **No new dependency and no download:** the UI already has the typeface, and the
     drawing is a few kilobytes of canvas work, not image processing.
2. **The settings** (`export.watermark`), behind the design's switch, which shows a
   row under the switches when on:
   - **Text:** up to 120 characters. The placeholder suggests "© 2026 Your name"; with
     no text nothing is laid on.
   - **Position:** five small frame icons for top left, top right, centre, bottom left
     and bottom right (the default).
   - **Size:** S, M (the default) or L.
   - **Defaults:** off, and remembered like the dialog's other choices. Read leniently,
     field by field; overlong text is cut.
   - **Presets leave it alone,** like the other switches.
3. **The queue only:** the dialog exports through the queue. The single-photo export
   command (used by the self-test) takes no watermark.
4. **Tests:**
   - **Laying on:**
     - the text sits in its corner at its size and margin, with every pixel outside it
       unchanged;
     - each of the five positions lands where it says;
     - a half-transparent drawing lays half as much;
     - 16-bit stays 16-bit;
     - a long line is kept within the margins;
     - a bad PNG is an error.
   - **Settings:** off by default, a stored watermark loads, unknown values fall back,
     and long text is cut.
   - **The drawing (dev mock in the browser):** a PNG of the text is made; none when
     off or blank. The dialog shows the third switch and, when on, the text, position
     and size row.
   - **Release self-test** (`exportWatermark`):
     - the UI draws "© 2026 Self-test" and the queue lays it on a 1,350 px JPEG of the
       Nikon Z 6 (180,572 bytes against 171,417 plain);
     - looked at, it sits in the bottom right at the large size, legible on the wooden
       table.

## Deviations from the design

Recorded in ADR 0016: the design shows only the switch; the text, position and size
row under it is this ADR's.

## Consequences

- **An image watermark** (a logo) could use the same path: the UI would send the
  photographer's PNG instead of drawing text.
- **Opacity, colour and font** are fixed (70 %, white with a shadow, the app's
  typeface). They can become choices if photographers ask.
