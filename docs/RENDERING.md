# Rendering

Status: Phase 0. Renderer version `1` (`renderer::RENDERER_VERSION`), recipe version
`1` (`renderer::RECIPE_VERSION`).

## 1. Model

```text
Original file ──decoder──► LinearImage ──┐
                                         ├─► RenderBackend ─► OutputImage
EditRecipe ──RenderPlan::from_recipe─────┘
```

- **`EditRecipe`**: the persisted edit. Parameters only, versioned, deterministic
  serialisation (`canonical_bytes` feeds cache keys). Sanitised on every use: clamped
  to range, NaN replaced by defaults, `-0.0` normalised.
- **`RenderPlan`**: backend-agnostic list of `Stage`s with parameters resolved
  (e.g. temperature → channel gains, EV → multiplier). Identity stages are omitted.
- **`RenderBackend`**: executes a plan. `CpuRenderer` is the production backend;
  `gpu-spike` implements the same trait for evaluation (ADR 0005).

Adding a stage means adding a `Stage` variant, its scalar reference implementation in
`renderer::ops`, a CPU kernel, and golden cases. Backends and callers do not change.

## 2. Internal representation

| Buffer | Format | Why |
|---|---|---|
| `LinearImage` | interleaved RGB `u16`, scene-linear, sRGB/Rec.709 primaries, 65535 = source white | half the memory of `f32`; 16 bits of linear precision is ~16 stops, enough for source data |
| per-chunk scratch | interleaved RGB `f32` | all processing is float; values above 1.0 survive between stages |
| `OutputImage` | RGBA8 (display) or RGB8 (export), sRGB-encoded | RGBA lets the webview blit directly |

Values only become >1.0 inside the pipeline (e.g. positive exposure); sources never
exceed white, so `u16` storage loses nothing.

## 3. Pipeline (Phase 0)

```text
RAW decode (LibRaw)            camera WB (as shot), demosaic, camera matrix -> linear sRGB
    │                          (JPEG: sRGB decode + linearise via LUT)
    ▼
White balance (temperature,    per-channel gains, relative to the as-shot light
               tint)           (ADR 0024)
Exposure                       multiply by 2^EV
Dehaze                         dark-channel haze removal on the scene map (ADR 0028)
Tone (highlights, shadows,     local gains from an edge-aware surroundings map, and
      whites, blacks)          end-point gains (ADR 0023)
Detail (texture, clarity,      local contrast at two scales, no halos (ADR 0026), and
        sharpening)            capture sharpening, default 40 (ADR 0027)
Contrast                       S-curve around mid grey (scene-referred)
Base look (Standard)           camera-like tone curve: lift, toe, shoulder (ADR 0022)
Colour mixer                   hue/saturation/luminance per colour band (ADR 0025)
Colour (vibrance, saturation)  chroma scale around Rec.709 luminance
    ▼
Output transform               clip [0,1], sRGB OETF, 8-bit quantise
```

The Light section's tone curve graph is computed by `renderer::tone_curve` (ADR 0029).
It shows the Light controls' response for a neutral tone, against the default
rendering.

### Stage definitions (reference implementations in `renderer::ops`)

- **Dehaze** (`-100..100`, ADR 0028): the airlight and transmission are estimated
  on the 256-px scene map by the dark-channel prior. The transmission is refined by a
  luminance-guided filter and evaluated per pixel.
  - `+s` removes `s` of the estimated haze (transmission at least 0.2).
  - `−s` blends in airlight, more where the scene is already hazy.
  - Later stages (tone, detail) measure the dehazed scene.
- **Temperature / Tint** (`-100..100`, ADR 0024): relative to the photo's as-shot
  light, which the decoder reports (D65 for JPEGs).
  - Temperature moves the assumed light `1.2 mired` per unit along the Planckian
    locus. Tint moves it `1/3000 Duv` per unit across the locus.
  - Gains = as-shot light / assumed light in linear sRGB, normalised so neutral
    luminance is preserved.
  - *Simplification:* applied in linear sRGB after the camera matrix, not in camera
    space before it (see §8).
- **Exposure** (`-5..5 EV`): linear multiply.
- **Highlights / Shadows** (`-100..100`): local and hue-stable (equal gain on R, G, B).
  - The gain in stops comes from the surroundings' brightness: a fast guided filter
    of log luminance on a fixed 256-px map, so preview and export match.
  - Shadows gives up to 2 stops, from mid grey down. Highlights gives up to 1.5 stops,
    within 3 stops of white.
- **Whites / Blacks** (`-100..100`): the gain comes from the pixel's own brightness,
  in the top 1.5 stops (1 stop) or from 4 stops below white down (1.5 stops).
- **Texture / Clarity** (`-100..100`, ADR 0026): gains in stops from log2
  luminance.
  - Texture is the pixel minus a small blur (radius 0.15 % of the long edge, two box
    passes). It fades out in deep shadows.
  - Clarity is the small blur minus the edge-aware map. It is zero along strong
    edges, so it has no halos.
  - ±100 doubles or removes the band (Clarity at 2× strength). Gains are limited to
    ±1.5 stops.
- **Sharpening** (`0..150`, default 40, ADR 0027): unsharp mask on log2 luminance
  with a 3×3 binomial blur at the rendered size. 100 multiplies one-pixel detail by
  2.5. Each pixel's change is limited to ±0.5 stop; it fades out in deep shadows.
  Recipes before version 7 read it as 0.
- **Contrast** (`-100..100`): per channel, in a gamma-2.2 perceptual domain; curve
  fixes 0, mid grey (0.18) and 1; slope at the pivot is `2^(±0.8)` at the extremes;
  values above 1 pass through, so contrast alone never clips highlights.
- **Base look** (`look`: `standard` | `flat`, ADR 0022):
  - **Standard** is a per-channel log-logistic S-curve after a 1.25 EV lift
    (slope 1.65, pivot 0.55), normalised so sensor white stays white. Its
    parameters were fitted to six cameras' own JPEGs.
  - **Flat** is no curve: the look of version-1 recipes, kept for their edits.
- **Colour mixer** (8 bands × hue, saturation, luminance, each `-100..100`, ADR 0025):
  - Works on square-rooted display values. The pixel's hue blends the two nearest
    band centres (smoothstep); the centres are 0/30/60/120/180/225/270/300°.
  - Hue rotates around the grey axis (±30°). Saturation scales chroma (0–2×). Both
    keep luminance.
  - Luminance is a ±1 stop gain. Near-grey pixels fade out.
- **Vibrance** (`-100..100`, ADR 0024): chroma scaled around Rec.709 luminance,
  weighted per pixel.
  - Positive: up to 2× for muted colours, none for fully saturated ones, and skin
    hues get at most 40 %.
  - Negative: mutes strong colours more, never to grey.
- **Saturation** (`-100..100`): `Y + (rgb − Y)·(1 + s/100)` with Rec.709 luminance,
  clamped at 0. `-100` gives exact luminance-preserving monochrome.
- **Output**: hard clip at 1.0. With the Standard look, highlights roll off through
  its shoulder before the clip. The Flat look on a JPEG reproduces the source exactly
  (tested).

## 4. CPU backend

`CpuRenderer` compiles the plan into fused kernels:

- consecutive gain stages (WB, exposure) merge into one multiply;
- per-channel curves (contrast) become a 4097-entry piecewise-linear LUT;
- the output transform is a 16385-entry `f32 → u8` table.

Rows are split into chunks of ~64K pixels and processed in parallel (rayon). Each
chunk converts its `u16` source rows into a per-thread reusable `f32` scratch buffer,
runs all kernels while the data is in cache, and writes encoded bytes straight into
the output. **No full-size intermediate image is allocated for any stage.**
Cancellation is checked once per chunk.

`render_into` accepts a caller-owned output buffer for reuse.

### Stages that look at neighbourhoods

The tone stage (ADR 0023) is the first. It needs only a *low-resolution* view of the
whole image, built once per render before the parallel pass and cached while it
cannot change. Kernels receive each chunk's row span (`RowSpan`) to look it up, so
it still runs in the single fused pass.

The detail stage (ADR 0026) needs full-resolution neighbourhoods.
- Each chunk reads the source rows around it (`RowSpan::source`), measuring on its
  rows plus the blur's reach either side.
- Chunks are made tall enough (≥ 8 × radius rows) that this overlap stays small.
- The result equals a whole-image blur (tested), so the stage still fuses into the
  single pass.

Sharpening and noise reduction will follow the same pattern.

## 5. Resolution levels

| Quality | Source | Target |
|---|---|---|
| Thumbnail | smallest pyramid level ≥ 256 px | small views of an open image |
| Interactive | smallest level ≥ 85% of min(viewport, 1280 px) | while dragging |
| Detail | smallest level ≥ viewport (device px) | 180 ms after changes settle |
| Export | full-resolution decode (separate path) | final output |

Library thumbnails are a separate path: they come from the camera's embedded preview
(or a reduced decode plus default render), not from an open image's pyramid. See
ADR 0015.

The preview pyramid is built from a **reduced-resolution decode**: LibRaw
`half_size` (skips demosaic, bins 2×2) when half resolution still has a long edge
≥ 1600 px; JPEGs are box-downsampled after decode. Each level halves the previous one
with a 2×2 box filter in linear light. Levels are power-of-two apart, so interactive
renders accept a level down to 85% of target instead of jumping to one with ~4× the
pixels.

Consequence: "Detail" is currently capped at half the sensor resolution. True 1:1
zoom needs a full-resolution decode path for the viewer (§7).

## 6. Caching and versioning

- Preview cache key = `SourceId` (size + mtime + head/tail content fingerprint; path
  excluded) + FNV-1a hash of the canonical recipe + rendered width/height + pixel
  format + `RENDERER_VERSION`.
- `RENDERER_VERSION` must be bumped whenever the same recipe would render differently.
  Golden tests (below) detect such changes.
- `EditRecipe::from_json` accepts older versions (migration hook) and rejects newer
  ones with an explicit error instead of silently misrendering.

## 7. Correctness testing

- `renderer::ops` unit tests: identity points, monotonicity, range, invariants.
- LUT vs reference: output LUT within 1 code; contrast LUT within 1/255 display.
- CPU backend vs scalar reference over the chart: max 1 code difference.
- Golden images (`tests/fixtures/golden/renderer/*.png` + `*.recipe.json`): procedural
  chart × 12 recipes, tolerance 1 code, decoder-independent. Single adjustments use the
  Flat look (images unchanged since version 1); `standard_*` lock the Standard look.
- RAW golden (`tests/fixtures/golden/raw/*.png`): synthetic DNG → LibRaw → renderer,
  tolerance 4 codes (absorbs LibRaw version drift).
- The synthetic DNG simulates a sensor with unequal channel sensitivities and an RGGB
  mosaic; tests assert LibRaw's WB + matrix recover the scene-linear chart values.

Regenerate goldens intentionally with `UPDATE_GOLDEN=1` (see `tests/fixtures/README.md`)
and bump `RENDERER_VERSION`.

## 8. Known gaps before this is a real raw converter

- White balance belongs in camera space before the colour matrix (needs camera matrix
  from the decoder and our own demosaic/scaling control).
- One base look for every camera: no per-camera profiles or colour matrices beyond
  LibRaw's. The cameras' JPEGs are still slightly more saturated. There is no
  highlight reconstruction.
- Working space is linear sRGB primaries; a wider working space (e.g. linear
  Rec.2020/ProPhoto) should be evaluated before adding colour grading.
- No colour management of the display (assumes sRGB display) or export (no ICC).
- Orientation for JPEGs is ignored.
