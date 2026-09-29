# ADR 0019: Edits are saved automatically, per photo, in the catalogue

- Status: Accepted (Phase 2, milestone 7)
- Date: 2026-09-29

## Context

Until now an edit lasted until another photo was opened. PRODUCT.md workflow B ends
with "Save automatically". §2.4/2.5 require originals to stay untouched, with edits
kept as data. CLAUDE.md §13 asks for recipes that are deterministic, versioned and
portable, with migrations rather than silently invalidated edits.

## Decision

1. **Storage.** Migration 5 adds `edits(photo_id, recipe_version, recipe, updated_at_ms)`,
   one row per edited photo.
   - The catalogue stores the recipe as opaque JSON plus its schema version. Parsing,
     sanitising and migrating belong to the renderer (`EditRecipe::from_json`), so the
     catalogue never depends on the renderer.
   - Edits attach to the photo, so they follow moves and renames.
   - Resetting to the original (an identity recipe) deletes the row.
2. **Newer versions are never overwritten.**
   - A recipe from a newer schema (by column or by the JSON's own version) loads as
     `SavedEdit::TooNew`. The photo shows unedited, the UI says changes aren't saved,
     and `save_edit` refuses.
   - Opening a library in an older version of the app therefore loses nothing.
   - An unreadable recipe is logged and treated as unedited.
3. **Autosave.**
   - The editor saves 400 ms after the last change (a slider drag is one save).
   - Saves are serialised, so an older recipe can never land after a newer one.
   - Opening another photo writes the pending save first, and so does closing the
     editor.
   - Worst case on a crash: the last 400 ms of changes.
4. **Opening returns the saved recipe.** `open_image_path` returns the saved recipe
   with the image, so the first render already shows the edit. The camera's embedded
   preview, shown for the first ~30 ms, is still the unedited look.
5. **Only library photos have saved edits.**
   - A photo opened from outside the granted folders says "Not saved: this photo isn't
     in your library". Edits there last until another photo is opened.
   - Photos the index hasn't reached yet are recorded on first save (as with marks,
     ADR 0018).
6. **Thumbnails show edits.**
   - The recipe's canonical bytes join the thumbnail cache key.
   - An edited photo's thumbnail is rendered from a reduced decode, since the embedded
     preview can't show the edit. It is cached like any other.
   - Pre-generation passes each photo's recipe.
7. **UI (from the design):**
   - "● Edited" in Edit's header, with Reset.
   - An edited dot on cards and list rows.
   - Save progress ("Saving…", "Couldn't save") only when it matters.

## Measurements (M1 Max; `bench --thumbnails`, cold)

| File | Unedited (embedded preview) | Edited (decode + render) |
|---|---|---|
| Canon R6 CR3 / Sony A7 III ARW | 10–14 ms | 175–180 ms |
| Nikon Z 6 NEF | 4.5 ms | 366 ms |
| Sony A7R IV ARW (61 MP) | 11 ms | 425 ms |
| Fuji X-T3 RAF / Ricoh GR III DNG | 29–37 ms | 450–470 ms |

That is one render per edit, cached afterwards, on the browse lane (never the UI
thread). A screen full of freshly edited photos fills in over a second or two.

**Follow-up:** when an edit is saved for the photo open in Edit, render the thumbnail
from its in-memory preview pyramid (a few milliseconds) and store it under the new key.

## Consequences

- Edits, like marks, cannot be rebuilt from the files. They are protected by the
  automatic backups of ADR 0021.
- History and undo (Phase 7) will build on this: the `edits` table holds the current
  state, and history will be a separate table of earlier states.
- Presets, copy/paste and batch edits (Phase 7) are recipes too, and reuse
  `save_edit`.
