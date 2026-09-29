# ADR 0030: Noise reduction, and one interactive render in flight

- Status: Accepted (Phase 5, milestone 1)
- Date: 2026-09-29

## Context

The design's Detail section ends with Noise reduction (0–100, default 0). It has to
smooth two kinds of noise:

- **Luminance noise:** grain, strongest in the shadows.
- **Colour noise:** blotches of false colour.

It must keep edges and real colour, match between the preview and the export, and
stay affordable in the interactive render and in memory at 24 MP.

## Decision

1. **Luminance** is a guided filter (He et al.) of log2 luminance, self-guided, inside
   the detail stage.
   - Its radius is 0.08 % of the long edge: 1 px at 1516, 5 px at 6064.
   - Its threshold is twice the expected noise at the local brightness. That starts
     at 0.03 stop at white and doubles every two stops down (shot noise), times the
     slider.
   - It is a hue-stable gain.
   - Texture, Clarity and Sharpening then measure the **denoised** luminance, so they
     don't bring the noise back.
2. **Colour** is smoothed on a whole-image map of 512 cells on the long edge.
   - The map holds the source after the gains and dehaze, like the scene map
     (ADR 0028).
   - The ratios R/Y and B/Y are guided-filtered with log luminance as the guide, and
     each pixel interpolates its smoothed ratios.
   - A pixel moves towards them only by a noise-sized amount (0.1–0.35 in ratio). Real
     colour edges, which the luminance guide can't separate, keep their colour: no
     bleeding (checked on the chart and the Nikon sample).
   - Luminance is kept.
3. **Cost and memory:**
   - The colour map is built once per render and cached per source (the gain-free
     average) and per gains, dehaze and amount (the smoothed map).
   - Without dehaze it ignores exposure (ratios and the guided filter's output don't
     change with a constant exposure factor), so exposure drags reuse it.
   - Its two channels are filtered in parallel.
   - Because the colour part reads a map and not rows, only the luminance filter adds
     to the stage's reach.
   - Kernel buffers now belong to one render (`KernelScratch`) instead of to threads,
     so they are freed when it ends.
   - The horizontal box pass, now the hot loop, sums shifted rows for small radii
     (vectorised) and runs eight rows at once for large ones.
4. **Recipe version 9** adds `noiseReduction`; older recipes read it as 0.
5. **Preview scheduler (UI):** at most one interactive render is in flight.
   - Changes made meanwhile replace each other, and the newest is sent when the render
     returns.
   - Before, every change was sent at once and Rust cancelled the render in progress.
     With every control and exposure changing each frame, and renders slower than a
     frame, no render ever finished: the release self-test showed zero frames. Slow
     machines would hit the same.
   - Now frames arrive at whatever rate the machine renders. Detail renders are still
     superseded by new changes.

## Measurements

Nikon Z 6, `main` and this branch alternated three times each, bench recipe with
Noise reduction 30. The machine had a load average of 7–13 from other work.

| Measure | `main` | With noise reduction |
|---|---|---|
| Interactive render, 1516×1010, every control set | 11.8 ms | 15.3–16.7 ms |
| Full-resolution render | 199–204 ms | 308–314 ms |
| Export render | 321–323 ms | 488–569 ms |
| Peak memory (bench process) | 1483–1485 MB | 1549–1586 MB |

Default recipe alone: 2.9 ms, and 6.5–6.7 ms with Noise reduction 30 (measured with
an earlier, 768-cell colour map).

How the stage got there:

| Version | Interactive | Full resolution | Peak memory |
|---|---|---|---|
| First (colour per chunk at full resolution) | +26 ms | ~900 ms | 2.2 GB |
| Faster box blur, shared guide stats | +12 ms | — | — |
| Colour as one map, per-render buffers | +3.5–4.9 ms | ~310 ms | ~1.58 GB |

Release self-test, all controls dragged together, load average 14:

- 28–29.5 fps previews, 17–27 fps during an export;
- median render 22–25 ms (inflated by the load).

## Consequences

- The Detail section's four sliders are built.
- Colour smoothing is at a fixed map resolution: blotches much smaller than a map
  cell (about 12 px at 24 MP) are left to the luminance part.
- Heavier denoising (non-local means, AI denoise) remains possible later behind the
  same slider or as a separate tool.
