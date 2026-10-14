# ADR 0085: Luminance and colour range masks

- Status: Accepted
- Date: 2026-10-14

## Context

PRODUCT §5.2 lists colour range and luminance range among the masks to come. They are
the last ones on that list that need no model. Lightroom users rely on them on their own
("lift the shadows", "cool the blues"). They rely on them even more to narrow another
shape: a gradient limited to the sky's colour, a brush limited to the highlights.

The mask system (ADRs 0040–0043) asks each shape only how much it covers a point, and
combines shapes by adding, subtracting and intersecting. Range shapes differ in one way:
their coverage depends on the pixel, not only on its place.

## Decision

1. **Two shapes** (`MaskShape::Luminance`, `MaskShape::Colour`), combined like any
   other: as a mask's first shape, or added, subtracted or intersected.
   - **Luminance range:** fully on where lightness is between Low and High (0..100),
     fading beyond each end over Feather (0..100: up to a quarter of the scale; 0 is a
     hard edge). A new one covers 60–100, the brighter part.
   - **Colour range:** fully on near any of up to five picked colours, fading further
     away. Range (0..100) is how far:
     - the distance is in Oklab, with lightness counted half, so a colour's lighter
       and darker parts stay in;
     - coverage is full within 40 % of the reach.

     A new one has no colour and covers nothing until one is picked.
2. **What they read:** the photo as it enters the local adjustments: the image being
   rendered after white balance and exposure, in Oklab, with lightness above white
   taken as white.
   - A mask's own Exposure never moves it.
   - The global white balance and exposure do, as they change what the photo looks
     like.
   - Tone, colour and detail controls, which come after the masks, don't.
3. **Rendering** (`renderer::masks::range`):
   - **Computed once:** a range shape's coverage of the image being rendered is worked
     out in parallel rows, then read like a brush's map at every stage (gains, the
     tone stage's surroundings, clarity).
   - **Passed with the plan:** the maps travel with the generated masks' maps (ADR
     0074), under the shape's settings as their name.
   - **Cached:** the latest six (by image, gains and settings) are kept, so dragging
     the mask's own sliders reads the map again.
4. **Tint and picking without rendering** (`range::FrameSampler`): a frame point is
   mapped to the photo as the framing does, read from a pyramid level, the lens's
   vignetting lifted and the plan's gains before its masks applied (`RenderPlan::
   gains_before_masks`).
   - `range_mask_view` gives a shape's coverage over the shown crop for its tint. The
     tint draws it as it draws a detected mask's view.
   - `pick_mask_colour` gives the Oklab colour averaged over a few pixels around a
     point.
5. **UI** (recorded in ADR 0016):
   - **Where to add them:** Luminance and Colour after Radial, in the toolbar's Add,
     the Add tiles and the mask card's Add and Subtract rows.
   - **Luminance:** a band on a black-to-white bar, with Low, High and Feather.
   - **Colour:**
     - while it is edited, the photo is an eyedropper: a click picks the colour there,
       in place of the others; Shift-click adds one, up to five;
     - "Click the photo to pick a colour" shows over the photo until one is picked;
     - the picked colours are swatches (click one to remove it), with Range under them.
6. **Recipe version 33.** Older recipes have no range shapes; nothing to migrate.
7. **Tests:**
   - **Renderer:**
     - the luminance band and its fade;
     - colour reach growing with Range;
     - the nearest of several colours counting;
     - gains moving lightness;
     - sanitising (swapped ends, too many colours);
     - the map and the sampler against the reference;
     - a render lifting a dark half and not a bright one;
     - global exposure moving what the range sees;
     - a colour darkening only its half;
     - no colour adjusting nothing.
   - **Frontend:**
     - new shapes' defaults;
     - turns and flips leave them be;
     - view keys;
     - picked colours shown as they look.
   - **Release self-test** (Nikon Z 6):
     - a 0–35 range at +1 EV lifts the darkest tenth by 35 levels and leaves the
       brightest exactly;
     - the colour picked at the middle shows full coverage in the engine's view there;
     - at −1 EV it darkens the middle by 13 levels.
   - **The dev mock was checked in the browser:**
     - adding each;
     - the eyedropper hint, then a pick: swatch and tint;
     - the band and its sliders;
     - the eight-kind layouts.
8. **Performance** (PERFORMANCE §61):
   - a preview's map takes 6 ms (13 ms with five colours);
   - dragging the mask's sliders renders in 6.8 ms against 2.1 ms with no mask;
   - a 24 MP export spends 70–190 ms on a map.

## Consequences

- Range shapes narrow any mask, which covers most uses of Lightroom's "Intersect with
  range".
- **Left out for now:**
  - an eyedropper for luminance (the band is set with its sliders);
  - showing the luminance range's own histogram;
  - a range over hue only;
  - range shapes don't follow spot removal: they read the photo before retouching in
    the tint and after it in the render, which differs only on a retouched spot.
