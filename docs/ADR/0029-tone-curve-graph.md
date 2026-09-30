# ADR 0029: The tone curve as a graph of the Light controls

- Status: Superseded by ADR 0037 (the graph is now the photographer's editable curve)
- Date: 2026-09-29

## Context

`docs/PRODUCT.md` lists a tone curve for Phase 4. The design places a "Tone curve"
graph at the top of the Light section's "More controls", above Whites, Blacks and
Dehaze. It is a picture of what the Light sliders do, with no points to drag. Its
curve comes from rough formulas in the mock-up.

An editable point curve (Lightroom's Point Curve) is a larger feature, not in the
design. The user was offered the choice, and building the graph first was the stated
plan.

## Decision

1. **Build the graph as designed:** a 288×120 box with a quarter grid, the dashed
   diagonal and the curve.
2. **The curve is real, computed by the renderer** (`renderer::tone_curve`) and not
   copied into TypeScript.
   - Both axes are display values.
   - x is a neutral tone as the default recipe shows it. y is the same scene tone
     under the recipe's exposure, Whites/Blacks, contrast, base look, and
     Highlights/Shadows as they act on an even area of that brightness.
   - An unedited photo draws the exact diagonal (tested, and checked in the release
     self-test through the real command).
   - Choosing the Flat look shows as a curve below the diagonal in the shadows.
3. **Local effects are not in the curve.** Dehaze, and Highlights/Shadows beside
   brighter or darker surroundings, depend on the picture, so no single curve can
   show them. The graph shows their effect on even areas.
4. **Transport:** a `tone_curve` IPC command returns 49 points (as the design samples
   it). The UI asks only when a field it depends on changes (exposure, contrast, the
   four tone controls, look) and ignores stale replies. It costs microseconds.

## Consequences

- The Light section now matches the design completely.
- A draggable point curve can come later as its own milestone. It would add a recipe
  field and a render stage. The graph would then show those points on top of the
  sliders' curve.
