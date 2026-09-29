# ADR 0033: Auto level

- Status: Accepted (Phase 5, milestone 4)
- Date: 2026-09-30

## Context

The design has an "Auto level" button in the crop toolbar and the Geometry section. It
levels the horizon and reports "Horizon levelled · −1.4°". It must not "level" photos
that have nothing to level by: organic scenes and texture.

## Decision

1. **Measure, in the renderer** (`geometry::auto_level`, on a preview level of about
   1000 px):
   - A structure tensor on log2 luminance, after a light blur so pixel noise does not
     count as edges.
   - Scharr gradients, which are nearly free of the axis bias of central differences.
     That bias pulled small tilts towards 0°.
   - A 7×7 averaging window.
   - Edges within 15° of horizontal or vertical vote for the straighten angle that
     would level them. The vote is weighted by strength × coherence⁴, so texture,
     which points every way, adds no preferred angle.
   - The answer is the histogram peak (0.1° bins, smoothed, refined to sub-bin).
2. **Decline when unsure.** The peak must hold at least 7.5 % of the evidence within
   ±0.5°. On the samples:

   | Scene | Confidence |
   |---|---|
   | Pure noise | 0.04 |
   | Organic (Sony parks) | 0.05 |
   | Canon (sculptures, perspective railing) | 0.07 |
   | Ricoh beach (a real horizon, level) | 0.09 |
   | Nikon, Fuji interiors | 0.12 |

   This is a threshold tuned on few photos; `level_estimate` exposes the confidence
   for future tuning.
3. **Accuracy:** within 0.35° on soft-edged synthetic horizons and verticals, with
   heavy texture, for tilts from −7.5° to 11° (tested). Straightening by the answer
   and measuring again gives about 0°.
4. **Transport and UI:**
   - An `auto_level` command runs as an interactive job (28–52 ms in the release
     self-test).
   - The buttons sit where the design has them. The result sets Straighten, which
     re-fits the crop.
   - A short status line says "Horizon levelled · −1.4°" or "No clear horizon found".
     The design shows a toast; the app has no toast system yet (recorded in ADR 0016).
5. **Display:** slider values show as many decimals as their step, so Straighten reads
   "−1.4°" as in the design.

## Consequences

- Scenes with strong perspective (converging verticals) can mislead it. Vertical
  and Horizontal perspective correction (Phase 5, "Perspective & lens") will handle
  those.
