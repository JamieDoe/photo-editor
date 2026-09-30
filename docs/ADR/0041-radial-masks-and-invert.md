# ADR 0041: Radial gradients, feather and invert

- Status: Accepted (Phase 6, milestone 2)
- Date: 2026-09-30

## Context

The product requires radial gradients, and invert and feather on every mask
(`docs/PRODUCT.md` §5.1). The design has a Radial mask (purple dot) whose overlay
fills an ellipse. It shows no feather or invert controls.

## Decision

1. **Shape `radial`:**
   - `centre`: frame fractions, like other points.
   - `radius`: two radii along the ellipse's own axes, as fractions of the frame's
     diagonal. The diagonal doesn't change when the photo is turned, so Rotate and
     Flip move only the centre and the angle. A circle stays a circle on any photo
     shape.
   - `angle`: turns the first axis clockwise from horizontal, in degrees.
   - `feather` (0..100): the share of the radius the fade takes.
   - Coverage is `1 − smoothstep(1 − feather, 1, d)`, where d is the elliptical
     distance (1 on the edge). Feather 0 gives a hard edge; 100 fades from the
     centre. The effect is inside, as the design's overlay shows.
2. **`invert` on every mask** (omitted from the JSON when off): coverage becomes
   1 − coverage, for linear and radial alike. Inverting a radial makes an off-centre
   vignette; inverting a linear gradient flips which side is adjusted.
3. **`hidden` on every mask** (omitted from the JSON when off): the mask is kept, with
   its shape and adjustments, but not applied. The plan leaves it out, so previews,
   thumbnails and exports all agree. Each row in the Selective section has an eye
   button to hide or show it (crossed out when hidden). Hidden masks are dimmed in the
   list and the toolbar, and their tint is not drawn; their handles stay, for editing.
4. **Recipe v18.** Older readers would not know `radial`; older recipes read
   unchanged.
5. **UI:**
   - **Adding:** "Radial" is in the mask toolbar's Add group and the Add tiles. A new
     radial starts as a circle in the middle of the shown picture, a little under
     half its short side across, feather 50.
   - **On the photo:**
     - the dashed ellipse, and fainter, dotted, where the fade starts;
     - the tint drawn with an SVG radial gradient transformed into the ellipse,
       using the renderer's stops;
     - a centre handle to move it;
     - side handles to stretch and turn it (they follow the pointer);
     - top and bottom handles for the other radius.
   - **Mask card:** below a dashed divider, Feather (radial only; its spec comes
     from the engine) and an **Invert** switch, styled like the lens switches.
   - Inverted masks' tint covers the outside.
6. **Cost:** a radial adds about 0.6 ms over a linear gradient at 1516×1010 (a square
   root per pixel), 2.5–2.7 ms over no mask (docs/PERFORMANCE.md §29).
7. **Self-test:** an inverted −1 EV radial in the middle darkens the top rows by 44.6
   levels and leaves the middle unchanged (0.00).

## Deviations from the design

Recorded in ADR 0016: the Feather and Invert controls (the product requires them),
the per-mask eye button (requested), the handles, and the inner fade ellipse.
