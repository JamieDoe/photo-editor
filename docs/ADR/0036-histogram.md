# ADR 0036: Histogram, draggable

- Status: Accepted (Phase 5, after milestone 6)
- Date: 2026-09-30

## Context

The design puts a histogram at the top of the adjustments panel, above the exposure
details:
- red, green and blue filled, and a luminance line;
- shadow and highlight clipping triangles.

The user asked for it to be draggable, as in Lightroom: dragging across the graph
adjusts the tone controls. The UI must not process images (CLAUDE.md §4).

## Decision

1. **Counted in Rust, on the rendered frame.**
   - `renderer::Histogram::of` counts the display-encoded output, as the photo appears
     on screen: 256 bins each for red, green, blue and luminance (Rec. 709 weights on
     the encoded values).
   - It is one parallel pass: 0.4 ms at 1516×1010 and 1.2 ms at 3032×2020.
   - The engine adds one to every viewer frame, cache hits included. Thumbnails get
     none.
2. **Transport:** in the binary preview frame, between the 28-byte header and the
   pixels. A header flag (`FRAME_FLAG_HISTOGRAM`) marks it: 4 planes × 256 × u32, 4 KB.
   The UI reads the counts and draws them. It never touches pixels.
3. **Drawing** (`histogramGraph.ts`):
   - Pairs of bins, lightly smoothed, give 128 points per curve.
   - The tallest inner point fills the height. Spikes are cut off at five times the
     typical height, with each curve's three tallest points left out of that typical
     height. Spikes come from pure black or white, or from a large flat area such as
     sky, and would otherwise flatten everything else.
   - Colours and blending follow the design: screen blending on the dark theme, and
     multiply on the light theme (which the design does not have).
   - The triangles light when at least 0.02 % of pixels have a channel at 0 (blue
     triangle) or 255 (accent triangle).
4. **Dragging, as in Lightroom.** The width is split into five zones:

   | Zone | Share of the width |
   |---|---|
   | Blacks | 0–10 % |
   | Shadows | 10–35 % |
   | Exposure | 35–65 % |
   | Highlights | 65–90 % |
   | Whites | 90–100 % |

   - Hovering tints the zone and shows its name and value where the exposure details
     are.
   - Dragging left or right moves that slider. The full width moves half its range
     (5 EV for Exposure, 100 for the others), in its steps.
   - A double-click resets the zone's slider.
   - The sliders stay the precise, keyboard-accessible way to set the same values.

## Deviations from the design

Recorded in ADR 0016:
- the zone tint and the name-and-value readout while hovering or dragging;
- the light theme's blending.

## Consequences

- Every viewer frame is 4 KB larger. That is negligible next to its pixels (6 MB at
  1516×1010).
- The histogram is of the preview, not the full-resolution image. It is a sample of
  the same picture, which is enough for a display, as in other editors.
- The tone curve (next) can draw the same histogram behind it.
