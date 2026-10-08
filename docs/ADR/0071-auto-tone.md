# ADR 0071: Auto tone

- Status: Accepted
- Date: 2026-10-08

## Context

The design's preset strip has an Auto button: "Analyse the photo and set a starting
point". Its prototype sets Exposure, Contrast, Highlights, Shadows, Whites, Blacks and
Vibrance, and confirms with "Auto tone applied". ADR 0016 listed it as not built.
Lightroom's Auto is the reference most photographers know: one click, one undo step,
⇧⌘U.

## Decision

1. **Measured through the real pipeline** (`renderer::auto_tone`):
   - a sample of the photo (a preview level of about 512 px, by what the crop
     keeps) is rendered as edited, with the six tone sliders at zero;
   - each slider is then found by bisection until the rendered sample (its
     sRGB-encoded luma histogram) meets its target;
   - the result follows the renderer's own curves, and stays right if those change.
   - **Order:** Exposure, then Contrast, Highlights, Shadows, Whites and Blacks, each
     measured with the ones before it set.
   - **Vibrance** comes from the sample's colourfulness: muted photos get up to +20,
     colourful ones less, in steps of 5.
2. **Bands, not targets.** The first version aimed every slider at an exact target.
   - **Why that failed:** on the six camera fixtures it pushed sliders to their
     limits (Contrast −30 with Whites +60 and Blacks −47 to −60), and it darkened a
     well-exposed overcast scene by 0.6 EV. The default render already matches each
     camera's JPEG (ADR 0022), so exact targets rework photos that need little.
   - **Now:** each slider stays at zero while the photo is inside its band, and
     otherwise moves only to the band's nearer edge:

     | Slider | Band (rendered luma) | Range |
     |---|---|---|
     | Exposure | median 0.40–0.52 | −2 to +2.5 EV, in 0.05 |
     | Contrast | middle-half spread at least 0.26 (only added) | 0 to +30 |
     | Highlights | at most 3 % above 0.92 (only recovers) | −70 to 0 |
     | Shadows | at most 6 % below 0.06 (only opens) | 0 to +60 |
     | Whites | 99.5th percentile 0.90–0.985 | −40 to +40 |
     | Blacks | 0.5th percentile 0.012–0.06 | −40 to +30 |

   - **On the fixtures** (`bench --auto`, before/after images):
     - the Z 6 and Ricoh are left at Exposure 0;
     - the overcast A7 III park gains depth (Blacks −40) without being darkened;
     - the A7R IV landscape's nearly white sky comes back blue (Highlights −70) and
       its hedge opens (Shadows +26);
     - the others move a little.
3. **Engine and IPC:** `Engine::auto_tone` runs on the interactive lane, a newer
   request replacing one still running. It takes the photo as edited (white
   balance, colour, crop and masks are kept), and removals use the fill a render
   would. The IPC command is `auto_tone(imageId, recipe)`.
4. **In the editor:**
   - **The button:** Auto, as designed: accent-tinted, with a wand, after Import…
     and Save…, which keep their place before it.
   - **The shortcut:** ⇧⌘U, as in Lightroom.
   - **The result:** applied to the newest edit as one step ("Undo Auto tone"),
     confirmed with the design's "Auto tone applied". The button is disabled while
     Auto runs.
5. **Tests:**
   - **Statistics:** percentiles, shares and colourfulness of a known image.
   - **The solver, through the CPU renderer:**
     - a dark scene is brightened into the band;
     - a bright one is darkened and its highlights recovered;
     - a flat one gains contrast and range;
     - a night scene's Exposure stays within its limit;
     - every setting is in range, whole, and the same each time;
     - a scene whose median is already mid-band keeps Exposure, Contrast and Shadows
       at zero, only its clipped top recovered.
   - **Engine:** the edit's own tone sliders don't change the result.
   - **Release self-test** (`autoTone`): on the Nikon Z 6, Auto takes 19 ms through
     IPC, the photo with it has its median in the band, and the button is there.

## Consequences

- **A gentle starting point, not a look:** photos that are already fine change
  little; dark, flat and clipped ones change most.
- **Cost:** about 15–60 renders of a 512 px sample, 13–100 ms (PERFORMANCE §51).
- **Not done:**
  - Auto per setting (double-clicking a slider's label, as in Lightroom);
  - auto white balance (the design's Auto doesn't set it);
  - subject-aware weighting (faces, sky). The bands treat every pixel alike.
