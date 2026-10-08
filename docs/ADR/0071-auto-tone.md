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
5. **Auto per setting** (added on 2026-10-08):
   - **How:** Shift-double-click on a slider's name or track, as in Lightroom. A plain
     double-click still resets.
   - **Which:** Exposure, Contrast, Highlights, Shadows, Whites, Blacks and Vibrance.
     Their names say so on hover ("Shift-double-click for Auto Whites").
   - **What it finds:** that setting alone, the rest of the edit as it is, aimed at
     a **target** rather than kept in its band. A first version used the bands, and
     on a photo already within them every Shift-double-click left the slider where it
     was ("toast shows but doesn't move the slider").

     | Setting | Target (rendered luma) | Range |
     |---|---|---|
     | Exposure | median 0.46 | as Auto |
     | Contrast | middle-half spread 0.36 | −15 to +30 |
     | Highlights | 1 % above 0.92 (only recovers) | −70 to 0 |
     | Shadows | 2 % below 0.06 (only opens) | 0 to +60 |
     | Whites | 99.5th percentile 0.975 | −40 to +40 |
     | Blacks | 0.5th percentile 0.02 | −40 to +30 |

   - **The rest of the edit counts:** with Exposure pushed until the photo clips,
     Auto Whites pulls the white end in rather than out.
   - **The result:** one step ("Undo Auto Whites"), confirmed with its value ("Auto
     Whites +23"), or "Whites already suits this photo" when nothing changes.
   - **Settings that can't help stay put** (Auto too): a setting that can't bring the
     photo to its band or target, and changes it by less than 2 % of the measure on
     the way, stays at zero rather than going to its limit.
     - **Why:** Whites acts on tones within about 1.5 stops of the sensor's white
       (ADR 0023). On a photo with nothing that bright (the Z 6 indoor scene, white
       end 0.85), ±40 Whites moved it by 0.001, yet Auto had set +40.
     - **Measured:** on synthetic scenes, Whites ±40 moves the white end by nothing
       when the brightest tones are 1.7 or more stops below white, and by about 0.02
       when within a stop.
   - **The code:** `renderer::auto_tone::auto_setting` finds one setting
     (`ToneSetting`), and Auto is `auto_setting` for each in turn. Both run through
     the engine's same sample (`Engine::auto_setting`, IPC `auto_setting`).
6. **Tests:**
   - **Statistics:** percentiles, shares and colourfulness of a known image.
   - **The solver, through the CPU renderer:**
     - a dark scene is brightened into the band;
     - a bright one is darkened and its highlights recovered;
     - a flat one gains contrast and range;
     - a night scene's Exposure stays within its limit;
     - every setting is in range, whole, and the same each time;
     - a scene whose median is already mid-band keeps Exposure, Contrast and Shadows
       at zero, only its clipped top recovered.
   - **Engine:** the edit's own tone sliders don't change Auto's result. Auto Whites
     on the chart (pure whites) pulls in; darkened two stops, it raises.
   - **Per setting:**
     - on a photo Auto leaves alone, Exposure still moves the median to its target;
     - it follows the rest of the edit (Whites raised on a photo just short of
       white, pulled in once Exposure clips it);
     - a setting that can't change the photo (Whites, nothing near white) stays at
       zero.
   - **Release self-test** (`autoTone`): on the Nikon Z 6, Auto takes 19 ms through
     IPC, the photo with it has its median in the band, and the button is there.
     Blacks alone moves the black end towards its target, and Whites alone stays at
     zero on that photo, which has nothing near white.

## Consequences

- **A gentle starting point, not a look:** photos that are already fine change
  little; dark, flat and clipped ones change most.
- **Cost:** about 15–60 renders of a 512 px sample, 13–100 ms (PERFORMANCE §51).
- **Not done:**
  - auto white balance (the design's Auto doesn't set it);
  - subject-aware weighting (faces, sky). The bands treat every pixel alike.
- **Found along the way:** Whites only reached tones near the sensor's white. On
  photos with nothing that bright, the slider did nothing when dragged either. ADR
  0073 makes it act near the photo's own white, and Auto's Whites now takes effect
  there.
