# ADR 0073: Whites acts near the photo's own white

- Status: Accepted (amends ADR 0023)
- Date: 2026-10-08

## Context

Since ADR 0023, Whites has acted on the top 1.5 stops below the sensor's white.

- **The problem:** many photos never reach that. Indoor and overcast scenes, or
  anything exposed to keep highlights, have their brightest tones well below it. On
  such photos Whites did nothing, whether dragged or set by Auto.
- **Where it showed:** found with Auto per setting (ADR 0071). On the Nikon Z 6
  indoor scene, Whites ±40 moved the white end by 0.001.
- **On the camera fixtures,** the brightest tones (99.5th percentile) sit 0.0–1.7
  stops below the sensor's white: 1.74 on the Z 6, 1.38 on the Canon EOS R6, 1.05 on
  the Ricoh, 0.50 on the Fujifilm, and 0.0–0.05 on the two Sonys.
- **Lightroom's Whites** acts on the photo's own brightest tones, wherever they are.

## Decision

1. **The photo's white:** where its brightest tones are, the 99.5th percentile of
   luminance (`ops::tone::white_point_stops`).
   - **Where it's measured:** as the tone stage sees the photo, after white balance
     and Exposure. Raising Exposure raises it.
   - **How:** on a regular grid of about 262,000 pixels, so a preview level and the
     full resolution measure alike. On the fixtures they agree within 0.02 stops.
   - **Range:** kept within 4 stops above and 10 stops below the sensor's white.
2. **Whites acts there:** on the top 1.5 stops below the photo's white and on
   anything brighter, ±1 stop at ±100, as before.
   - **Brighter photos behave as before:** for a photo that reaches the sensor's
     white, Whites is unchanged.
   - **Pushed past white:** the tone stage's lookup tables now reach 4 stops above
     the sensor's white, so tones an edit has pushed past it are graded too.
   - **The cost:** the white is cached per source and gains, so dragging Whites
     doesn't measure again. It's measured once per change of source, white balance
     or Exposure: about 262,000 luminance samples and a selection.
3. **Old edits render as they did:**
   - **Recipe version 26.** Older recipes that set Whites get `whitesFromSensor:
     true` on loading (written only when set), keeping Whites near the sensor's white.
   - **The rest:** recipes without Whites, and every new recipe, act near the
     photo's white.
   - **Copying and presets:** the flag is in the "Light and tone curve" settings
     group, so copy and paste and presets carry it with Whites.
4. **`RENDERER_VERSION` 6:** renders of recipes with Whites from version 26 change.
   Older recipes' renders don't.
5. **Tests:**
   - **Tone:** Whites near the photo's white lifts its brightest tones by its full
     stop where near the sensor's it didn't reach them. The photo's white is the
     same on an image and on it halved, and gains move it.
   - **Recipe:** a version 25 recipe with Whites keeps the sensor's white and writes
     the flag; one without doesn't. Version 26 acts near the photo's white.
   - **The kernel against the scalar reference,** with Whites in every stage: they
     match (within 1).
   - **Golden images:** `whites_plus50_blacks_minus50` is pinned to the sensor's
     white and unchanged; a new `whites_plus50_photo` shows the photo's.
   - **Auto:** a setting that can't change the photo stays at zero (now with a
     measure no setting changes).
   - **Release self-test:** on the Z 6, Auto sets Whites +40 and now it shows.

## Consequences

- **Whites works on every photo,** as in Lightroom. Auto's Whites now takes effect
  where it used to be left at zero (PERFORMANCE §51).
- **The crop counts:** the white is measured on what the tone stage renders (after a
  crop), so cropping away a bright sky can move it.
- **Blacks still acts from 4 stops below the sensor's white down.** Photos rarely
  lack deep tones, so it hasn't shown the same problem. It could follow the photo's
  black the same way if it does.
