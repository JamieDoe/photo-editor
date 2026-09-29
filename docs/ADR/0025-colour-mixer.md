# ADR 0025: Colour mixer

- Status: Accepted (Phase 4, milestone 3)
- Date: 2026-09-29

## Context

The design's Colour section hides a colour mixer behind "More controls":

- a label and the chosen range's name;
- eight colour dots (Reds, Oranges, Yellows, Greens, Aquas, Blues, Purples, Magentas),
  Blues selected at first;
- Hue, Saturation and Luminance sliders for the selected range.

Its most common uses are darkening a blue sky, calming greens and adjusting skin
(oranges). An adjustment must never leave steps between neighbouring colours, and it
must never touch greys.

## Decision

1. **Recipe version 5** adds `mixer`: eight bands, each `{hue, saturation, luminance}`
   from −100 to 100.
   - It is optional, and written only when a band is edited. Recipes without it keep
     byte-identical JSON and cache keys.
   - An all-zero mixer sanitises to none.
2. **Where it runs:** after the base look (display-referred), before Vibrance and
   Saturation. It works on square-rooted values, which are close to how colours look.
3. **Bands:**
   - Each band has a centre hue: 0, 30, 60, 120, 180, 225, 270 and 300°.
   - Between two centres the settings blend with a smoothstep, so every hue has a
     smooth response.
   - Blues is centred at 225° rather than 240°. A typical sky (about 215°) then gets
     about 90 % of the Blues adjustment instead of 65 %, and pure blue keeps about
     75 %.
4. **Operations:**
   - **Hue** rotates the colour around the grey axis, up to 30° at ±100.
   - **Saturation** scales chroma, from 0× to 2×.
   - Both keep luminance.
   - **Luminance** is a gain of ±1 stop.
   - Pixels near grey fade out (saturation 0.04–0.2), because their hue is noise.
5. **Performance structure:**
   - Everything that depends only on hue is tabulated per degree.
   - Hues the mixer leaves unchanged are flagged, so pixels of untouched colours stop
     after the hue calculation.
6. **Spec from the engine:** `EngineInfo.mixer` lists the bands and the per-band
   controls, so ranges still live in Rust.
7. **UI, as in the design:**
   - "COLOUR MIXER" label with the range name, then the dots with a ring on the
     selected one, then the three sliders. Arrow keys move between ranges.
   - One addition (ADR 0016): a small accent mark on the dots of edited ranges, so an
     edit in a range not on screen isn't forgotten.
   - With the mixer, the design's slider details now also apply: no zero mark on
     custom tracks (Temperature, Tint) or on mixer sliders.

## Measurements (1516×1010 interactive, Nikon Z 6, three runs)

| Case | Mixer cost |
|---|---|
| First version, one band edited | +3.5 to +3.9 ms |
| After the flags and a branch in place of `rem_euclid`, one band edited | +1.48 to +1.55 ms |
| Same, all eight bands edited (worst case) | +3.0 to +3.9 ms |

Release self-test, dragging the mixer every frame together with Temperature, Tint
and Vibrance:

- median render 4.9–5.8 ms;
- 30 fps preview on Nikon, Canon and Fuji.

## Consequences

- It is a per-pixel stage like Vibrance. If low-end hardware needs it, a SIMD
  (branch-free) version is the next step.
- Masks will later reuse the same per-band code for colour-range selections.
- The GPU spike does not implement the mixer; it reports it as unsupported.
