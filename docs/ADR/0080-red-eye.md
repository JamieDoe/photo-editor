# ADR 0080: Red-eye removal

- Status: Accepted
- Date: 2026-10-09

## Context

PRODUCT §6 lists red-eye among V1 retouching, beside heal, clone and spot removal. A
flash lighting the retina through a wide pupil leaves it red.

## Decision

1. **A correction is a circle over an eye** (`renderer::redeye::RedEye`):
   - **Where:** its centre in the source photo's own coordinates and its radius as a
     fraction of the long edge, like heal spots. It stays on the eye whatever the
     crop, straightening or turn, and moves with an EXIF turn (ADR 0078).
   - **Settings:** Pupil size and Darken, 0 to 100 (50 by default).
   - **In the recipe:** `redEyes`, version 31. Applied to the source after removals
     and spots, in the same cached retouch step, so other sliders don't redo it.
2. **What counts as pupil:** `redness`, how far red outweighs green and blue
   (`(r − max(g, b)) / r`).
   - **Threshold:** above 0.55 at the default Pupil size; Pupil size moves it from
     0.65 to 0.45.
   - **Skin is left alone:** it is reddish, but in linear light its red is under twice
     its green (redness about 0.4), where a red pupil's is many times it.
   - **Edge:** the weight fades out over the circle's last quarter.
3. **The fix:**
   - **Colour:** the red is replaced by the pupil's own green and blue, softened over a
     pixel each way, so its texture stays and the edge isn't cut out.
   - **Brightness:** it is darkened (up to 75 % at Darken 100) only where the pixel
     itself is red.
   - **Catchlights:** a white catchlight beside the pupil keeps its brightness.
4. **Placing one: click on the eye.** The engine (`find_red_eye`, on a preview level
   of about 1500 px) finds the red region nearest the click within the brush, and
   sizes the circle to it: 1.5 pupils across, so the soft edge clears the rim. If
   there is no red there, a toast says to try a larger brush centred on the pupil.
5. **The Retouch panel's fourth tool,** Red eye, after Remove, Heal and Clone.
   - **On the photo:** corrections are dotted rings; the selected one is solid white.
   - **Editing:** drag a ring to move it; Delete removes it. Selecting one shows its
     Pupil size and Darken.
   - **The panel's count and Clear all** include them.
   - **Edits:** they belong to the photo. They travel with retouching in copy and
     paste, and presets keep the photo's own.

Along the way: the Rust recipe's identity check left out removals. An edit with only
removals counted as no edit, so saving it would have deleted it. It now counts
removals and red-eye corrections, as the editor's check already did.

## Consequences

- **Cost:** finding a red eye takes under half a millisecond at 24 MP; applying one
  takes about 4 ms, mostly copying the image.
- **Later:** Lightroom's pet-eye mode (green or yellow glow).
- **Tests:**
  - a synthetic face, where the pupil is found, centred and sized, and turns dark and
    neutral, while the catchlight, the white of the eye, the skin and red lips stay;
  - Pupil size and Darken;
  - a full render;
  - an old edit on an upright photo, with a red-eye correction.
