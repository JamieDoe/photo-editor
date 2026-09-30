# ADR 0038: Red, green and blue tone curves

- Status: Accepted
- Date: 2026-09-30

## Context

The tone curve (ADR 0037) shapes all three channels together. Lightroom also offers a
curve per channel, for colour grading: warm highlights, cool shadows, or a colour cast
corrected by tone.

The user asked for them, with a channel picker. The design has neither.

## Decision

1. **Recipe v15:** `channelCurves: {red?, green?, blue?}`, each a list of points like
   `pointCurve`. A channel is written only when shaped, and the field only when one is.
   Older recipes have none and render as before.
2. **Order:** each channel's curve applies after the RGB curve, to its own channel.
   The two compose into one lookup table per channel per render.
   - Measured at 1516×1010: a four-point RGB curve adds 1.5 ms over no curve, and
     three channel curves on top add about 0.1 ms more.
   - An RGB-only curve keeps its single shared table.
3. **UI:**
   - A compact **RGB · Red · Green · Blue** switch sits between the Tone curve title
     and the graph. Each channel has a colour swatch, and the accent dot marks a
     shaped one.
   - The graph edits the chosen channel's points as before (ADR 0037). The curve is
     drawn in that channel's colour, over that channel's histogram (luminance for
     RGB).
   - Reset resets the shown channel. The Light section counts any shaped curve as an
     edit.
   - The switch is a radio group, so the keyboard reaches it.
4. **Self-test:** a red curve on the self-test's recipe (colour controls neutral)
   lifts red's mean by 31.8 and leaves green's unchanged (0.0).

## Consequences

- Saturation, Vibrance and the colour mixer run after the curves and mix channels.
  With them set, a red curve moves the other channels a little too, as in other
  editors.
- The switch is a deviation from the design (ADR 0016).
