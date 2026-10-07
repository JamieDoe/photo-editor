# ADR 0060: Wide gamut: soft compression now, a wide working space later

- Status: Accepted: option 2, soft compression and a tagged Colour space export
- Date: 2026-10-01

## Context

The pipeline works in linear sRGB from the start. LibRaw decodes into sRGB primaries
(`output_color = 1`) as 16-bit integers, so any colour outside sRGB is clipped before
editing begins. Every adjustment then runs in sRGB, and so does the display.

A Display P3 or Adobe RGB export (the design's Colour space row) could hold no colour
sRGB cannot. The photographer asked for the working space to be widened first, as its
own milestone (ADR 0059).

## Measurements

Each camera file in the fixtures was decoded with LibRaw in linear Rec.2020
(`output_color = 8`, half size). Every well-exposed, unclipped pixel was then checked
against sRGB and P3. A pixel counts as outside when one channel converted to that space
is below −1% of its brightest channel.

| Camera (scene) | Pixels outside sRGB | Outside P3 | Clipped even by Rec.2020 | How far beyond sRGB |
|---|---|---|---|---|
| Fujifilm X-T3 (packaging, toys) | 3.2% | 0.4% | 0% | 3.1% |
| Nikon Z 6 (landscape) | 0.1% | 0% | 0% | 2.9% |
| Sony A7 III | 0.1% | 0% | 0% | 5.7% |
| Canon EOS R6 | 0% | 0% | 0% | — |
| Ricoh GR III (dunes, sky) | 0% | 0% | 0% | — |
| Sony A7R IV | 0% | 0% | 0% | — |

On the Fuji, the out-of-sRGB pixels are the printed pink band on a packet and a toy's
bright patches: saturated man-made colours. The natural scenes have essentially none.

That is typical. Wide gamut matters for saturated man-made colours, flowers, sunsets,
neon and stage lights, and almost never for skies, foliage or skin. It also matters
for **editing headroom**. Saturation, Vibrance, the colour mixer and colour grading can
push colours past sRGB, where they are clipped channel by channel today, which flattens
them and can shift their hue. A wide space would keep them until a final, deliberate
gamut mapping.

## What would change

About 26 source files assume sRGB primaries:

- **Decoding:** LibRaw's output space, the as-shot white chromaticity, and the JPEG
  decoder (sRGB files convert into the wide space).
- **Per-channel tone curves:** the base look, Contrast, and the RGB and channel point
  curves. A curve applied to each channel saturates differently in a wider space, so
  every photo's look would change unless retuned.
- **Luminance weights:** Rec.709 weights appear in about 40 places (tone, detail,
  dehaze, noise, saturation, vibrance, grain, dust, histogram).
- **Colour models:** the colour mixer's hue bands, vibrance, colour grading (Oklab
  via sRGB matrices), calibration (sRGB primaries by construction), and white balance
  gains (D65 sRGB).
- **Display:** a transform from the working space to the screen (sRGB, or P3 on Macs)
  with gamut mapping, plus the histogram and clipping warnings.
- **Export:** a Colour space row, profiles for P3 and Adobe RGB (the profile
  generator, ADR 0057, takes any primaries), and gamut mapping for sRGB output.
- **Compatibility:** the same recipe would render differently. That needs a renderer
  version, and either retuning every adjustment to match today's look in the wide
  space, or keeping today's sRGB path for edits made before. CLAUDE.md §13 forbids
  silently changing existing edits.

## Options

1. **Full wide working space** (as planned): decode into linear Rec.2020, retune every
   colour adjustment so existing edits look the same, add a display transform, then
   the Colour space row.
   - **Cost:** the largest change since the renderer was built, over several
     milestones.
   - **Benefit:** real colours beyond sRGB on a few percent of pixels in some photos,
     plus editing headroom.
2. **Wide at the edges only:** keep editing in sRGB, but decode the few out-of-gamut
   colours with a soft, hue-preserving compression into sRGB instead of a hard clip.
   Then add Colour space as a correctly tagged conversion.
   - **Cost:** small.
   - **Benefit:** saturated colours keep their hue and gradation instead of clipping;
     exports are valid P3 and Adobe RGB files, but with sRGB's range.
3. **Defer:** keep sRGB, leave Colour space hidden, and spend the time on features
   that matter to more photos.

## Decision

The photographer chose **option 2**. It comes in two parts.

### Part 1: soft gamut compression (this change)

1. **`image_core::gamut`:** the ACES reference gamut compression curve, per channel.
   - Each channel's distance from the brightest channel is eased past a threshold, so
     that a chosen limit lands on sRGB's edge.
   - Greys and colours well inside are never touched.
   - The curve is smooth and monotone, and the brightest channel is kept.
2. **Decoding:** LibRaw now decodes into **linear Rec.2020** (`output_color = 8`).
   - The Rust side converts each pixel to sRGB, compressing the colours beyond it
     (`FROM_REC2020`).
   - The as-shot white is unaffected: it comes from LibRaw's camera-to-sRGB matrix,
     whatever the output space.
3. **The curve is gentle and late.** It starts at 97% of the way to the edge, with
   limits at 20% beyond, sized for what cameras record (the fixtures reach 3–6%
   beyond). The few colours past the limits, Rec.2020's far corners, still clip as
   before.
   - ACES's defaults (80%, and limits covering all of Rec.2020) were measured first.
     They changed 6% of a landscape's pixels, by up to 35 levels, because they
     squeezed hard to make room for colours that never occur.
   - Kept, against a hard clip, the curve changes 1.9% of the Nikon's pixels (mean 4
     levels where changed, max 18), and 0–0.2% of the other deterministic files.
4. **Output:** where an edit can push colours past sRGB (Saturation, Vibrance, the
   colour mixer, colour grading), those adjustments now pass negative channels on
   instead of clamping them at zero.
   - The output compresses them (`ON_OUTPUT`: from 97%, limit 30% beyond) before
     encoding, in the fast and reference renderers and in the GPU spike's shader.
   - Plans without those adjustments encode exactly as before
     (`RenderPlan::compresses_output`). Compression always eases colours right at
     sRGB's edge a little, which plain photos and graphics should not see; the
     golden tests caught this.
5. **`RENDERER_VERSION` 4.** Cached previews and thumbnails are refreshed. Only the
   most saturated colours change.
6. **Tests:**
   - **Gamut:** colours within the limits land inside sRGB and those beyond are left
     to clip; colours well inside and greys are untouched; saturated colours just
     inside move only a little; the curve is smooth and keeps the brightest channel.
   - **Saturation:** a hard push goes negative and is brought back inside, keeping
     red to green.
   - **Goldens:** five renderer goldens with colour edits regenerated (the most
     saturated patches now sit just inside the edge instead of clipped flat), and
     the four raw goldens (the new decode).
   - **GPU parity:** passes with the shader updated.
7. **A finding along the way:** decoding a Fujifilm X-Trans file twice gives
   different pixels (3.4% of pixels, up to 63 levels). The other cameras are
   deterministic. This is in LibRaw's X-Trans path and is tracked separately.

### Part 2: the Colour space row (ADR 0062)

sRGB, Display P3 or Adobe RGB: a conversion from the sRGB working space with each
space's transfer curve and an embedded profile (the generator, ADR 0057, takes any
primaries). Built as ADR 0062.

## Option 1 later

Option 1, a full wide working space, stays possible when there is evidence it is
worth its cost. Nothing here stands in its way: the decoder already outputs Rec.2020,
so only the conversion into sRGB would move.

The measured gain from a full wide space is small for most photographs, while its
cost, and the risk to every existing edit's look, is large. Soft compression fixes the
visible problem (hue-shifted, flattened saturated colours) for little cost. Option 1
remains possible later; nothing in option 2 stands in its way.
