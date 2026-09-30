# ADR 0037: An editable tone curve

- Status: Accepted (supersedes ADR 0029)
- Date: 2026-09-30

## Context

ADR 0029 made the design's "Tone curve" graph a display: the renderer's response to
the Light sliders, with nothing to drag.

The user asked for a true tone curve, as in Lightroom and most editors: click anywhere
on it and drag to reshape it.

## Decision

1. **A point curve, independent of the sliders.**
   - The photographer's own curve starts as the diagonal. The sliders do not move it
     and it does not show their effect, as in Lightroom, Capture One and darktable.
   - ADR 0029's response graph and its `tone_curve` command are removed.
   - This departs from the design's mock-up, which bends the curve with the sliders
     (recorded in ADR 0016).
2. **Where it acts.**
   - On display tones (sRGB-encoded, 0..1) in and out, per channel, right after the
     base look and before the colour mixer.
   - It shapes the picture as it appears. Like Lightroom's RGB curve, a steep curve
     also adds some saturation.
   - Above white it holds its end value, so a pulled-in white point caps the
     highlights.
3. **Shape: monotone cubic (PCHIP, Fritsch–Butland tangents).**
   - It passes through every point and never overshoots them, so a gentle S stays
     gentle and a peak stays at its point.
   - It is flat beyond the first and last points. Moving the end points sets the black
     and white levels (a matte or faded look, or a clip).
   - The renderer (`ops::point_curve`) and the UI (`pointCurve.ts`) evaluate it
     identically, pinned by a shared test case.
4. **Rendering:** a 4096-entry lookup table per render, like the base look. It costs
   0.4–1.2 ms at 1516×1010.
5. **Recipe v14:** `pointCurve: [[input, output], …]`.
   - It is written only when not the diagonal.
   - At most 16 points, at least 0.01 apart, rounded to 1/10000.
   - The recipe stays `Copy`: the points are stored inline.
6. **The graph** (Light › More controls, where the design has it):
   - **Adding:** press anywhere to add a point on the curve at that input and drag
     it. The drag is relative, so the curve does not jump to the pointer.
   - **Moving:** drag a point. Its input stays between its neighbours.
   - **Removing:** drag an inner point more than 24 px off the graph (it disappears
     while held there, as in Lightroom), double-click it, or press Delete.
   - **End points:** a double-click puts one back in its corner.
   - **Readout:** while a point is held or focused, the title row reads "In 64 · Out
     82" (0–255). Otherwise it offers Reset while the curve is shaped.
   - **Keyboard:** points are focusable sliders. The arrow keys move them by one
     level, or ten with Shift.
   - The photo's luminance histogram (ADR 0036) sits faintly behind the curve.
   - The Light section's edited dot and "More controls" count a shaped curve as an
     edit.

## Consequences

- Only the RGB (master) curve for now. Per-channel red, green and blue curves would
  use the same points, stage and graph, with a channel switch the design does not
  have.
- Lightroom's parametric curve (region sliders) is not built. Highlights, Shadows,
  Whites and Blacks already cover it.
