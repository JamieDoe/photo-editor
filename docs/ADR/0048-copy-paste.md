# ADR 0048: Copy and paste edits

- Status: Accepted (Phase 7, milestone 5)
- Date: 2026-09-30

## Context

Phase 7 requires copying an edit from one photo to others (`docs/PRODUCT.md` §36).

The design puts a footer under the Edit panel: **Reset**, then **Copy** and **Paste**.
Paste is dimmed until something is copied. A toast confirms each action ("Edits
copied", "Pasted to 3 photos"). In the design, Copy takes the whole edit, and Paste
applies it to the photos selected for batch.

A crop, a straightened horizon or a mask is drawn for one photo. Pasted onto another,
it is usually wrong, which is why Lightroom leaves them out of copied settings by
default.

## Decision

1. **Setting groups, defined once in the renderer** (`renderer::settings`) and sent in
   the engine info. They follow the panel's sections:

   | Group | Fields | Copied by default |
   |---|---|---|
   | Exposure | exposure | yes |
   | Light and tone curve | look, contrast, highlights, shadows, whites, blacks, dehaze, point and channel curves | yes |
   | White balance | temperature, tint | yes |
   | Colour | vibrance, saturation, colour mixer | yes |
   | Detail and effects | texture, clarity, sharpening, noise reduction, vignette, grain | yes |
   | Crop, geometry and lens | geometry, chromatic aberration | no |
   | Masks | masks | no |

   A test checks that every recipe field is in exactly one group, so a new setting
   cannot be forgotten.
2. **Copy** takes the open photo's recipe and the chosen groups. The chevron beside
   Copy opens **Copy settings**, a popover of the groups to tick. The choice lasts for
   the session.
3. **Paste** sets exactly the copied groups' fields on the open photo.
   - Everything else is kept.
   - A field the source does not have (no crop, no masks) is removed from the target,
     so pasting "Masks" from a photo without masks clears them.
   - It is one undo step, "Paste". Pasting what the photo already has says so and
     changes nothing.
   - The copied edits survive switching photos: they live with the editor, not the
     view.
4. **Shortcuts:** ⇧⌘C copies and ⇧⌘V pastes, as in Lightroom. ⌘C and ⌘V stay with
   text, and text fields keep their own keys.
5. **Toast:** a small reusable confirmation (`components/Toast`), styled as the
   design's. It is a light pill low over the window, one at a time, gone after 2.2 s.
6. **Reset moves to the footer**, as in the design. The Edit header keeps the Edited
   and save notes.
7. **Not yet:** pasting onto several photos. The paste is by field names, so the batch
   milestone will reuse the same groups and merge.
8. **Self-test** (release, through the UI):
   - A look with a crop is copied with the footer's Copy, then pasted onto another
     edit with ⇧⌘V.
   - The look and exposure come across and the crop does not.
   - The frame renders, the toasts read "Edits copied" and "Pasted to 1 photo", and
     undo says "Paste".

## Deviations from the design

Recorded in ADR 0016:
- the Copy settings chooser (the design copies everything);
- the shortcuts;
- "This photo already has these edits".
