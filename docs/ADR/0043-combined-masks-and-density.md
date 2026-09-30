# ADR 0043: Masks of several shapes, and density

- Status: Accepted (Phase 6, milestone 4)
- Date: 2026-09-30

## Context

The product requires Add, Subtract, Invert, Feather and Density on every mask
(`docs/PRODUCT.md` §5.1), and Phase 6 lists mask combination. Until now a mask was one
shape, so "darken the sky but not the tree" or "lift this area, only where the
gradient reaches" could not be expressed. The design shows single-shape masks only.

## Decision

1. **A mask is a shape, then further shapes (`parts`), in order.** Each part has a
   `mode` and a `shape` (linear, radial or brush):
   - **Add**, `c + p − c·p`: covers where either covers.
   - **Subtract**, `c·(1 − p)`: takes the shape away.
   - **Intersect**, `c·p`: covers only where both cover.

   `c` is the coverage so far and `p` the part's. These combine coverages as
   independent layers, as the brush already composes its strokes and erases. Soft edges
   therefore blend smoothly where they overlap, with no crease where they cross (as
   `max`/`min` would give). Subtracting is intersecting with the complement.
2. **Invert and Density apply to the combination:** coverage `c` becomes `1 − c` if
   inverted, then is scaled by `density / 100`.
   - Density (0..100, default 100) is how strongly the mask applies. Its spec comes
     from the engine (`maskDensity`).
   - The brush's Flow stays the per-stroke amount (ADR 0042); within one stroke it
     already behaves as Lightroom's brush density.
3. **Recipe v20.** `parts` is written only when there are some, and `density` only
   when below 100, so older masks read and write unchanged.
4. **Rendering:** each shape is compiled once per render, as before. A pixel's
   coverage folds the parts in order. It skips the rest once nothing is left to
   intersect with or subtract from.
   - The plan drops a mask that is hidden, has no adjustments, is at density 0, or
     covers nothing. A mask covers nothing when its shapes combine to nothing (for
     example an empty brush with only subtracted shapes), unless it is inverted.
5. **The tint** (`MaskTint`): one canvas per active mask.
   - Each shape's coverage is drawn as alpha:
     - a gradient with the renderer's smoothstep stops;
     - a brush from its cached stroke layers (ADR 0042).
   - The shapes are combined with the canvas's compositing, which is the same
     arithmetic on alpha: Add is `source-over`, Subtract is `destination-out`, and
     Intersect is `destination-in`.
   - Invert fills and cuts the coverage out; Density is the alpha of the final
     colouring.
   - This replaced the SVG gradient tints of linear and radial masks, so all kinds are
     drawn one way.
6. **UI:**
   - **Mask card, after Exposure, Warmth and Clarity:**
     - **Density**.
     - Below a dashed divider, the mask's shapes, once there are two or more. Each
       row has the shape's dot and name, a mode menu (not on the first shape) and a
       remove button.
     - Two rows of one-click buttons, **Add** and **Subtract**, each offering Brush,
       Linear and Radial. A new shape is added in that mode and selected. Intersect
       is chosen from a shape's mode menu.
   - **Selected shape:** it gets the handles on the photo and its own controls: the
     brush's Paint/Erase, Size, Feather and Flow, or a radial's Feather.
   - **Delete key:** removes the selected shape, or the mask when it is the only one.
   - **Rotate and flip:** move every shape.
7. **Cost:** each further shape adds about 0.7–1.2 ms to a 1516×1010 render; Density is
   free (docs/PERFORMANCE.md §31).
8. **Self-test:** a −1 EV linear gradient over the top with a hard radial subtracted
   at its middle.
   - Inside the circle the photo is unchanged (0.00).
   - Beside the circle it is 42.2 levels darker.

## Consequences

- One generic mask representation keeps working for future AI masks (Subject, Sky):
  they become further shape kinds, combinable the same way.
- Per-shape invert (as some editors offer) is not provided; subtracting covers the
  common uses. It can be added as a part flag later without a migration.

## Deviations from the design

Recorded in ADR 0016: the design shows single-shape masks, and has no Density, shape
list or Add/Subtract rows.
