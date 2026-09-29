# ADR 0024: Temperature and Tint relative to the as-shot light; Vibrance

- Status: Accepted (Phase 4, milestone 2)
- Date: 2026-09-29

## Context

The design's Colour section has Temperature, shown in kelvin on a blue–amber track,
then Tint on a green–magenta track, Vibrance and Saturation. The colour mixer sits
behind "More controls" and is a separate milestone.

Before this, Temperature shifted the light relative to a fixed 6500 K reference,
whatever the photo was shot under. We had no as-shot white point and no Tint. Showing
kelvin needs each photo's as-shot light.

Temperature could be stored in two ways:

- **Absolute** (Lightroom): the recipe stores "4300 K, tint +8". This copies well
  between photos shot under the same light, but a preset carries one photo's light to
  another.
- **Relative** (our recipes since Phase 0, and the design's −100…100 slider): the
  recipe stores "+25 warmer". It works on any photo, JPEGs included, and presets stay
  portable.

## Decision

1. **The decoder reports the as-shot light.**
   - The LibRaw shim takes the camera's as-shot multipliers and the camera→sRGB matrix,
     and derives the light the camera's white balance neutralised:
     `rgb_cam · (pre_mul / cam_mul)`. It reads them before processing, which
     overwrites `pre_mul`.
   - The result is carried as a CIE xy chromaticity in `SourceInfo::as_shot_white`.
     Tests check that the synthetic DNG, whose AsShotNeutral is D65, returns D65.
   - Rendered images (JPEG) have none, and their white is D65.
   - The sample cameras give plausible values: 3050–6800 K, Duv −0.001…+0.012.
     exiftool wasn't available, so these weren't cross-checked against the cameras'
     own metadata.
2. **Sliders stay relative, stored as −100…100, on every photo.**
   - **Temperature** moves the assumed light by 1.2 mired per unit (±120 mired at
     ±100). In mired, a step looks alike under tungsten and daylight.
   - **Tint** moves it across the Planckian locus by Duv/3000 per unit (Adobe's scale).
   - Positive means a warmer or more magenta image.
   - **Gains:** as-shot light / assumed light in linear sRGB, normalised so neutrals
     keep their luminance.
   - **CCT/Duv:** from CIE 1960 uv against the Planckian locus (Kim et al.
     approximation, 1667–25 000 K). Accuracy is tested on D65 and illuminant A.
3. **Kelvin display.**
   - `ImageSummary` carries a `TemperatureScale` (as-shot kelvin and mired per unit).
   - The UI shows the assumed light, "5650 K" as in the design:
     `1e6 / (1e6/as_shot − amount·1.2)`. A shared test case in Rust and TS pins the
     formula.
   - Without an as-shot light (JPEGs), the slider shows its relative value.
4. **Vibrance.**
   - Works on display-referred values (after the base look), before Saturation.
   - Chroma is scaled around Rec.709 luminance, so brightness is kept.
   - **Positive:** up to 2× for muted colours, falling to none for fully saturated
     ones. Skin hues get at most 40 % of the effect.
   - **Negative:** mutes strong colours more than weak ones, never to grey.
5. **Recipe version 4** adds `tint` and `vibrance` (older recipes read them as 0).
6. **Renderer version 3.** Temperature is now relative to the real as-shot light, not
   6500 K. Existing temperature edits shift slightly (goldens: 2 and 8 codes out of
   255 at ±60). The library had no saved edits when this changed.

## Measurements

1516×1010 interactive level, Nikon Z 6, bench recipe, three runs:

| Stage | Cost |
|---|---|
| White balance (merged into the gains) | ~0.05 ms, unchanged |
| Vibrance | +0.40 to +0.47 ms |
| Whole bench recipe | 4.9 to 5.1 ms |

The release self-test changes Temperature, Tint and Vibrance on every drag frame:

- median render 2.8–3.3 ms, 95th percentile 3.6–5.5 ms;
- 30 fps preview on Nikon, Canon and Fuji.

## Consequences

- Presets made later will apply the same "warmer / greener" to any photo.
- Gains are still applied in linear sRGB, not camera space (RENDERING.md §8). Moving
  them before the camera matrix would change results again: it needs a renderer
  version bump and more decoder control.
- The GPU spike does not implement Vibrance; it reports it as unsupported.
