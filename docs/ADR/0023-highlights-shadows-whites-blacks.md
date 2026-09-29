# ADR 0023: Highlights and Shadows are local (edge-aware); Whites and Blacks are end points

- Status: Accepted (Phase 4, milestone 1)
- Date: 2026-09-29

## Context

The design's Light section has Exposure, Contrast, Highlights and Shadows, with Whites
and Blacks behind "More controls". Highlights and Shadows can be done two ways:

- **A global curve:** each pixel changes by its own brightness. It is simple and
  fast, but lifting shadows also flattens the texture inside them, and pulling
  highlights down greys out bright detail.
- **Local:** each pixel changes by the brightness of its *surroundings*. Dark
  *areas* lift while the detail within them keeps its contrast. Lightroom and camera
  makers work this way. The classic risk is halos: glows along strong edges.

Switching from global to local later would change existing edits' look, so we chose
now.

## Decision

1. **Local Highlights and Shadows.**
   - A pixel is brightened or darkened by a number of stops, applied equally to R, G
     and B (hue-stable).
   - The amount comes from a **surroundings map**: a self-guided fast guided filter
     (He & Sun) of log luminance. Flat areas get their average, and steps well above
     half a stop are kept as edges, so edges do not glow. It is tested with a hard
     6-stop step, where each side keeps within 0.6 stop of its own level.
   - The map is built on a **fixed 256-px grid**, whatever the render size, so the
     preview and the full-resolution export see the same surroundings (tested).
   - **Shadows** acts on surroundings from about mid grey down, up to 2 stops at
     ±100. **Highlights** acts within about 3 stops of white, up to 1.5 stops. Weights
     are smoothsteps, so there are no tonal steps.
2. **Whites and Blacks** move the ends of the tonal range, by the pixel's own
   brightness: Whites affects the top 1.5 stops (1 stop at ±100), Blacks from 4 stops
   below white downwards (1.5 stops at ±100).
3. **Pipeline position:** white balance → exposure → **tone** → contrast → base look →
   saturation. It works on scene-referred values, so the Standard look's shoulder
   still rolls off what Whites pushes up.
4. **CPU backend:**
   - The map is built once per render, and **cached (one entry)** while it cannot
     change. Its key is the source buffer, a fingerprint of sampled pixels (a new
     image at a reused address is never mistaken for the old one; tested) and the
     gains before the stage.
   - The per-pixel pass interpolates the map row by row and reads the gains from
     1/64-stop lookup tables.
   - A step-by-step reference implementation stays in `renderer::ops::tone`, and the
     fast path must match it within one code.
5. **Recipe version 3** adds `highlights`, `shadows`, `whites`, `blacks`. Older recipes
   read them as 0, which renders exactly as before. The GPU spike does not implement
   the stage (it reports it as unsupported).
6. **UI, as in the design:**
   - Highlights and Shadows follow Contrast; Whites and Blacks sit behind "More
     controls / Fewer controls" above a dashed divider.
   - That area opens by itself when one of them is edited, so an edit is never hidden.
   - Exposure shows "EV", as in the design. Its range stays ±5 (the design shows ±4),
     so existing edits are never clamped.

## Measurements (development Mac, 1516×1010 interactive level, `bench`, bench recipe)

| Tone stage version | Stage cost | Full render |
|---|---|---|
| First (per-pixel `exp2`, map rebuilt every render) | +6.6 ms | 9.4 ms |
| Lookup tables, per-row map interpolation | +4.0 ms | — |
| Final (map cached while it cannot change) | +2.0 ms | 4.5 ms |

- **Map rebuild:** about 2.8 ms, on every frame while exposure or white balance is
  dragged with a tone slider set.
- **Noise:** the machine is noisy, so treat these as ±10 %.
- **Low-end hardware:** not yet measured.

## Consequences

- Shadow recovery keeps texture, and highlight recovery keeps detail, without halos
  on the samples. Verified visually on the Fuji X-T3 and Sony A7 III files.
- This is the renderer's first stage that looks at neighbours. The row-span plumbing
  and the per-render prepass are what clarity, texture and dehaze will reuse.
- The GPU spike gains an unsupported stage. A GPU version belongs to Phase 8, after
  benchmarking.
