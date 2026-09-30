# ADR 0046: Presets

- Status: Accepted (Phase 7, milestone 3)
- Date: 2026-09-30

## Context

Phase 7 requires presets (`docs/PRODUCT.md` §36). §21 defines a preset as an edit
recipe template to create, apply, update, delete, import and export.

The design shows a Presets strip under the histogram:
- six looks: Natural, Vivid, Warm film, Matte, Mono and Cool fade, each a handful of
  slider values;
- each look a 64×44 preview of the photo with its name under it;
- the applied one ringed in the accent colour.

Applying one in the design keeps the photo's exposure and geometry, and replaces
everything else.

## Decision

1. **A preset holds only a look.**
   - It is a recipe without the photo's own settings: its exposure (each photo needs
     its own), geometry, lens corrections (measured on that photo) and masks (drawn on
     it).
   - The renderer defines both sides: `EditRecipe::look_only` and
     `EditRecipe::with_look_of`. The editor's `applyPreset` mirrors the latter, and
     both are tested.
   - Applying replaces the whole look, so any look settings the preset leaves at their
     defaults are reset. It is one undo step named after the preset ("Undo Mono").
2. **Built-in presets** are the design's six, defined in Rust
   (`renderer::presets::builtin`, with stable ids such as `mono`). They cannot be
   changed.
3. **Saved presets** live in the catalogue: schema 6 adds a `presets` table.
   - Each row holds a name and the recipe as opaque versioned JSON, like edits.
   - Saving stores only the look (sanitised). Names are trimmed, their spaces
     collapsed, and cut to 60 characters. An empty name is refused (new error kind
     `InvalidInput`).
   - A preset written by a newer version, or damaged, is left out of the list, not
     deleted.
   - Deleting a preset leaves photos edited with it unchanged, since applying copies
     the settings.
4. **IPC:** `list_presets`, `create_preset`, `rename_preset`, `update_preset` and
   `delete_preset`.
   - A preset's id is `builtin:<name>` or `user:<row>`, and only `user:` ids can be
     changed.
   - The list is the built-in presets, then the saved ones, oldest first.
5. **Previews** are real renders of the open photo with each preset:
   - thumbnail quality (256 px long edge);
   - in their own render slot (`presets`, `VisibleThumbnail` priority, behind the
     viewer's and the comparison's frames);
   - one after another.
   They depend only on the photo's own settings, because a preset replaces the rest.
   Moving a look slider redraws none; changing exposure, geometry or masks redraws them
   300 ms after the change stops.
6. **UI:**
   - The strip sits under the histogram, as in the design. Clicking a preset applies
     it.
   - One preset at most is ringed, while the photo has its look. Several presets can
     share a look, such as one saved straight after applying Natural. The ringed one
     is the preset last chosen on the photo this session (applied, saved or updated).
     Otherwise it is a saved preset with that look before a built-in one, which also
     covers reopening.
   - The popovers follow the design's dialogs: a header with a 60×40 thumbnail, a
     title and a line under it, then the content.
   - **Save…** in the strip's header opens a popover with the photo's thumbnail, "Save
     as preset", a note on what is kept, and a name field with Cancel and Save.
   - A saved preset shows ⋯ on hover. It opens a popover with the preset's preview and
     name, then a menu:
     - **Rename** turns the title into a field with the name selected; Enter or leaving
       it saves, Escape goes back.
     - **Update to this photo's look** is off while the photo already has it.
     - **Delete…** swaps the menu for a confirmation.
   - Popovers float over the panel, so the scrolling strip does not clip them. They
     follow their button when something scrolls, and close on a click elsewhere or
     Escape.
7. **Not yet:**
   - importing and exporting presets as files (the next milestone);
   - the design's **Auto** button (an automatic starting point), which is a separate
     feature.
8. **Cost** (release self-test, Nikon Z 6, machine load 6–8): each preview renders in
   2–5 ms from request to frame, so all six take about 22 ms.
9. **Self-test:**
   - The six previews render.
   - Clicking Mono through the UI turns the photo black and white: mean channel spread
     0, against 33.8 before. It keeps the exposure, rings Mono, and undoes as "Mono".
   - A preset is created (its look only), renamed and deleted.

## Deviations from the design

Recorded in ADR 0016:
- **Save…** stands where the design has **Auto** (not built yet);
- the ⋯ button and the details popover for saved presets (the design has no way to
  manage presets);
- previews are renders of the photo, where the design used a drawing tinted by CSS
  filters.
