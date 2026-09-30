# ADR 0051: White balance as a light, B&W mix, the parametric curve, `.lrtemplate`

- Status: Accepted (Lightroom parity, milestone 1)
- Date: 2026-09-30

## Context

Importing Lightroom presets (ADR 0047) left out settings this app had no counterpart
for. The most common were:
- white balance in kelvin, which raw presets set;
- the parametric curve;
- the B&W mix;
- presets from Lightroom before 7.3 (`.lrtemplate`).

The agreed roadmap takes these first, as the cheapest way to make imported presets
land closer.

## Decision

1. **White balance set as a light.**
   - A recipe may set `whiteBalance: {kelvin, tint}`, with the tint on Adobe's scale:
     the light's Duv x 3000, positive being greener light and so a more magenta photo.
   - The renderer balances each photo from its own as-shot light to that light
     (`white_balance::gains_for`), and Temperature and Tint are ignored while it is
     set. A preset, a paste or a sync therefore lands on the same light across
     photos.
   - Lightroom presets with `Temperature`/`Tint` (not "As Shot") import it.
   - The temperature scale now carries the as-shot tint, and the Temperature and Tint
     sliders show the shift it amounts to for the open photo. Moving either turns it
     into that shift. Display-referred photos resolve it against D65.
   - It is in the White balance copy group. A test checks that a light equal to the
     as-shot one changes nothing, and that a light 500 K warmer equals the matching
     slider shift.
2. **B&W mix is the colour mixer's luminance.**
   - The mixer runs before Saturation, so at Saturation −100 each colour's luminance
     sets how light its grey is, which is what Lightroom's B&W mix does. A render test
     shows it.
   - Lightroom black and white presets import their `GrayMixer*` values as the mixer's
     luminance and drop HSL, which Lightroom ignores in black and white.
   - When the photo is black and white, the mixer is titled "B&W mix", with a line
     saying what Luminance does. No new stage was needed.
3. **The parametric curve** (`ops::parametric_curve`).
   - Four region sliders (Shadows, Darks, Lights, Highlights), with splits at 25, 50
     and 75 by default, applied before the point curve, as in Lightroom.
   - Each slider moves its region's middle by up to 45 % of the region. The curve
     through black, those four points and white is the point curve's monotone cubic:
     smooth, black and white fixed, never folding back (tested at every extreme).
   - It is composed into the point curve stage's lookup tables, so it adds no pass.
   - The sliders sit under the tone curve graph ("Regions"). The graph shows only the
     point curve, not the regions' effect.
   - Lightroom's region sliders and splits import.
   - The shape is this app's own, close to Lightroom's rather than identical.
4. **`.lrtemplate` presets.**
   - A strict reader handles the Lua subset those files use: numbers, strings with
     escapes, booleans, tables of named or listed entries, and comments. It refuses
     anything else, such as a function call.
   - The settings go through the same mapping as `.xmp`. Tone curves are read from
     their flat lists as pairs.
   - The name is the title (a translation key's text after "=") or the internal name.
   - Import… offers `.lrtemplate` files too.
5. **Recipe v21** writes the white balance and the parametric curve only when set.
6. **Still left out on import:** Color Grading and split toning, Calibration, masks and
   healing, Lens Corrections, crop and transform, the profile, and colour noise
   reduction. These are the next parity milestones.

## Tests

- **Rust:** gains for a light against the sliders' shift; B&W mix rendering; the
  parametric curve (identity, regions, no folding, split ordering); `.xmp` and
  `.lrtemplate` reading (fixtures with our own values); refusing Lua that isn't data;
  preset import of both formats.
- **Completeness:** the copy group test covers the new fields.
- **Frontend:** the slider values a light amounts to, and turning it into them; the
  region sliders.
- **Self-test** (extended, but not yet run on this branch; see the PR): the Nikon's
  own as-shot light set in kelvin changes nothing, and a warmer light warms it; the
  `.lrtemplate` fixture imports with its kelvin white balance.
