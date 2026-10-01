# ADR 0059: Output sharpening ("Sharpen for")

- Status: Accepted
- Date: 2026-10-01

## Context

The design's export dialog has a **Sharpen for** row (Screen, Matte, Glossy) and a
**Colour space** row (sRGB, Display P3, Adobe RGB). Downsizing for the web softens
detail, and prints need more sharpening than screens. Matte paper softens detail more
than glossy.

The pipeline works in sRGB from the start: LibRaw decodes into sRGB primaries, clipping
whatever lies outside, and every adjustment runs there. A P3 or Adobe RGB export would
therefore be correctly converted and tagged, but could hold no colour sRGB cannot. The
photographer chose to build Sharpen for now, keep Colour space hidden, and widen the
working space as its own milestone. Colour space then follows with real wide-gamut
output.

## Decision

1. **Sharpen for: None, Screen, Matte, Glossy.**
   - These are the design's three, plus None, for files that will be edited further.
   - Remembered as `export.sharpen`, default Screen. An unknown value from a newer
     version reads as Screen.
   - The presets set it as the design has them (Screen for Web, Social and Full
     quality). Changing it by hand deselects the preset.
   - The export batch carries it, as does the single-photo export.
2. **An unsharp mask on brightness** (`export::sharpen`), applied after any resize, at
   the size written:
   - **The detail:** the display-encoded luma less its Gaussian blur.
   - **Threshold:** the detail fades in smoothly from a small threshold to twice it,
     so flat areas and faint grain are left alone.
   - **Colour:** the same amount is added to each channel, so colours keep their hue.
     Alpha is untouched, and 16-bit images are sharpened at 16 bits.

   | Medium | Sigma | Amount | Threshold |
   |---|---|---|---|
   | Screen | 0.6 px | 0.45 | 0.010 |
   | Matte (300 ppi) | 1.1 px | 0.90 | 0.008 |
   | Glossy (300 ppi) | 0.9 px | 0.60 | 0.008 |

3. **Bounded memory.** Besides the output, it keeps two single-channel float planes:
   luma and its horizontal blur. The vertical blur is taken while each output row is
   written. The first version kept float copies of every sample and of the output,
   about 1 GB at 24 MP; this one holds about 190 MB.
4. **Colour space** stays hidden until the working space is wide (the next
   milestone).
5. **Tests:**
   - **Export crate:**
     - edges get crisper with overshoot on both sides, flat areas far from them are
       untouched;
     - prints are sharpened more than screens, and matte the most;
     - None changes nothing, and faint grain below the threshold is left alone;
     - a coloured edge keeps its channel differences;
     - 16-bit images stay 16-bit.
   - **Settings:** a stored choice loads, and an unknown one reads as Screen.
   - **Release self-test (Nikon Z 6, 1,350 px JPEG through the export queue):** 165 KB
     with no sharpening, 167 KB for Screen and 182 KB for Matte. The sharpened file
     holds more fine detail.
6. **Performance** (PERFORMANCE §41):
   - **The step alone:** 58–86 ms at 24 MP using every core.
   - **In a full-size export:** on the export lane's smaller thread pool, render
     with sharpening takes about 230 ms, and the whole export about 1.4 s, as before
     sharpening.

## Deviations from the design

Recorded in ADR 0016:

- None is added to Sharpen for.
- Colour space is still hidden, waiting for a wide working space.

## Consequences

- **Wide working space (next milestone):** decode into a wide gamut, run the
  adjustments there, convert for the display, and offer P3 and Adobe RGB exports with
  generated profiles (the sRGB generator, ADR 0057, takes other primaries).
- **Print resolution:** print sharpening assumes 300 ppi. A print size and resolution
  in the dialog could later scale the radius.
