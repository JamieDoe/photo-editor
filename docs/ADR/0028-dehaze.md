# ADR 0028: Dehaze, and one scene map for every surroundings-based stage

- Status: Accepted (Phase 4, milestone 6)
- Date: 2026-09-29

## Context

The design's Light section has Dehaze (−100…100) behind "More controls", after Whites
and Blacks. Dehaze is the last slider that `docs/PRODUCT.md` lists for Phase 4.

Removing haze changes the scene that later stages measure. It darkens hazy areas and
raises their contrast. If Highlights/Shadows and Texture/Clarity kept measuring the
hazy scene, they would act on brightness that is no longer there.

## Decision

1. **Model.** Haze follows Koschmieder's model, `I = J·t + A·(1 − t)`, with the airlight
   `A` and the transmission `t` estimated by the dark-channel prior (He et al.):
   - `A` is the mean colour of the haziest 0.1 % of map cells, after a 7-cell patch
     minimum of the darkest channel.
   - `t = 1 − 0.9 · patch-min over channels of I/A`.
   - `t` is refined with a guided filter whose guide is luminance / luminance(`A`).
     A full-resolution pixel evaluates the model with its own brightness, so the
     coarse patches leave no halos along horizons and branches.
   - At `+s`: `t' = max(1 − s·(1 − t), 0.2)` and `J = (I − A)/t' + A`. The 0.2 bound
     limits how far hazy areas are stretched.
   - At `−s`: airlight is blended in, `I·(1 − h) + A·h` with `h = s·(0.1 + 0.3·(1 − t))`,
     so it adds more where there is already haze. The first strength, `0.2 + 0.5·…`,
     turned −70 into near white-out on the Sony sample.
2. **Scene map.** Every stage that looks at surroundings now derives from one
   256-px colour map of the scene (`ops::scene::SceneMap`):
   - Highlights/Shadows (ADR 0023), Clarity (ADR 0026) and Dehaze all use it.
   - The guided filter is shared (`GuidedMap`).
   - The map is the gain-free average scaled by the white-balance and exposure gains,
     which is exact because averaging is linear. Only a change of image re-reads the
     full image.
3. **Pipeline:** white balance → exposure → **dehaze** → tone → detail → …
   - The tone map is built from the dehazed map luminance.
   - The detail stage measures dehazed luminance per pixel.
   - Highlights/Shadows, Texture, Clarity and Sharpening therefore see the dehazed
     scene.
4. **CPU cache.** One source at a time, holding:
   - the gain-free map;
   - the rescaled map for the current gains;
   - the dehaze model for (gains, amount);
   - the surroundings map for (gains, dehaze amount).

   While any other slider is dragged, nothing is rebuilt.
5. **Recipe version 8** adds `dehaze`; older recipes read it as 0. The renderer version
   is unchanged: refactoring the tone map onto the shared map moves existing goldens
   by at most one code, within tolerance.
6. **UI, as in the design:** Dehaze follows Whites and Blacks behind the Light
   section's "More controls". The tone curve graph the design shows there is the next
   milestone.

## Measurements (Nikon Z 6, 1516×1010 interactive, three runs)

Bench recipe, now including Dehaze 20:

| Measure | Cost |
|---|---|
| Dehaze stage | +0.7 to +1.2 ms |
| Detail stage (measuring dehazed luminance) | +5.6 to +6.4 ms (3.4–3.9 ms without dehaze) |
| Whole render, maps cached (dragging any slider other than white balance or exposure) | 13.3–14.4 ms (10.4 ms before this milestone) |
| Full-resolution render | 215–220 ms |
| Export render | 366–371 ms |

Release self-test, which changes exposure and every other control on every frame
(the worst case: the maps are rebuilt each frame):

- first version: median render 18.5–21.2 ms, 95th percentile 25–26 ms;
- after caching the gain-free map and replacing the airlight sort with a partial
  selection: median 16.1–18.3 ms, 95th percentile 24–26 ms;
- preview 28–30 fps.

## Consequences

- Dehaze costs nothing when it is 0. When it is set, it adds about 3 ms per
  interactive render. About 2 ms of that is the detail stage measuring dehazed
  luminance per pixel.
- In the all-sliders worst case, the slowest 5 % of frames exceed a 60 fps frame on
  this machine. Next options if single-slider drags need to be faster on low-end
  hardware: SIMD for the per-pixel map evaluations (tone, dehaze, detail), or the GPU
  backend (Phase 8).
- Dehaze uses no extra full-image pass. Its model is built at map size.
- The GPU spike does not implement the stage; it reports it as unsupported.
- Lightroom's dehaze also shifts colour balance and saturation in some cases. Ours only
  removes the estimated airlight; Vibrance and Saturation stay separate.
