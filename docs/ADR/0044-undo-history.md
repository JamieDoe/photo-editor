# ADR 0044: Undo and redo

- Status: Accepted (Phase 7, milestone 1)
- Date: 2026-09-30

## Context

Phase 7 requires history (`docs/PRODUCT.md` §36). Undo must work on application state,
never copies of the image, and history must be compact (§23). Every later workflow
feature (presets, pasting edits, batch changes, Reset) makes large changes that need
to be undoable in one step. The design has no undo control.

## Decision

1. **History is recipes, per photo, in the editor** (`EditHistory`,
   `features/editor/history.ts`).
   - Each step keeps the recipe from before the change (for undo) or after it (for
     redo).
   - Edits replace only what they change, so recipes share the rest (masks, curves,
     brush strokes). A step costs about what it changed, never pixels.
   - Undoing and redoing go through the same path as any edit: the photo re-renders
     and the edit is autosaved.
2. **One step is one action:**
   - Everything changed while a pointer is held down is one step: a slider drag, a
     mask handle, a brush stroke, the tone curve, the histogram, or the crop. The
     gesture ends after the pointer-up's own handlers, so a control's last change on
     release joins it.
   - Otherwise, changes to the same controls less than a second apart are one step (a
     slider nudged with the arrow keys).
   - Anything else is its own step: a click, Reset, Auto level.
   - A step that ends where it started is dropped.
   - After an undo or redo, the next change always starts a new step. A new change
     clears redo.
3. **Named steps.**
   - A step is named after what it changed: the slider's label ("Exposure"), or its
     group ("Masks", "Tone curve", "Crop and geometry", "Colour mixer", "Lens
     corrections", "Look").
   - A step that changed several things is "Edits".
   - The buttons' tooltips say "Undo Exposure (⌘Z)".
4. **Bounded:**
   - 200 steps per photo; the oldest are dropped.
   - Histories are kept for the 30 most recently opened photos this session.
   - Reopening a photo continues its history, but only if its edit is still what the
     history ended at. Otherwise history starts afresh from the edit as it is.
5. **Not persisted yet.** History lasts the session. Saving it (the product's
   `edit_history` table) is a later step if wanted: the steps are recipes, so they can
   be stored as they are.
6. **UI:**
   - Undo and Redo icon buttons in the Edit header, between the save status and the
     photo counter. They are disabled when there is nothing to undo or redo.
   - ⌘Z undoes; ⇧⌘Z or ⌘Y redoes. Text fields keep their own undo.
   - macOS's default Edit menu also binds ⌘Z. The web view gets the key first, and the
     handler takes it.
7. **Self-test:** three exposure changes with the pointer held are one step named
   "Exposure".
   - Undoing renders the photo exactly as before the drag (mean luma 154.81), with an
     identical recipe.
   - Redoing renders the dragged photo (190.68).

## Deviations from the design

Recorded in ADR 0016: the design has no undo or redo control.
