# ADR 0016: Visual design adopted from the app design; fonts bundled locally

- Status: Accepted (Phase 2)
- Date: 2026-09-28

## Context

The user supplied an app design (the claude.ai artifact "Premium RAW photo editor
design") covering the Library, the Edit workspace and Export. The name it shows is a
placeholder, and the product is not named yet. The user decided to restyle the
existing screens to the design now, and to match it for every screen built from here on.

## Decision

1. **Tokens, not one-off colours.** `apps/desktop/src/styles.css` defines the design's
   palette as CSS custom properties, and components use only those:
   - surfaces: `--canvas` #0B0C0E, `--surface` #111215, `--raised`, `--control`;
   - 6% white hairlines;
   - three text levels: #EDEDEF, #A3A4AA, #7D7F86;
   - one warm accent, #F0B45E, used for "edited" dots, focus rings and highlights;
   - a white primary button.
2. **Light theme kept.** The design is dark only. The existing Appearance setting stays,
   with a light variant of the same tokens, and dark remains the default.
3. **Type:** Geist for UI text and Geist Mono for numbers and file details. The type
   scale is 11–16 px (13 px body, 11 px uppercase section labels).
4. **Fonts are bundled, never fetched.**
   - The design loads Geist from Google Fonts. The app uses `@fontsource-variable/geist`
     and `@fontsource-variable/geist-mono` instead: variable woff2 files served from the
     app bundle, so it stays local-first and works offline.
   - Dependency review:
     - licence: SIL OFL-1.1 (fonts) and MIT (packaging). Bundling OFL fonts in
       proprietary software is permitted; the fonts may not be sold on their own.
     - size: about 170 KB of woff2 per family across all subsets; only the needed
       subsets load.
     - no runtime code.
5. **Components, as in the design:**
   - a 52 px top bar: placeholder mark and working name, a Library/Edit segmented
     switch, then Settings and Export on the right;
   - a 240 px left sidebar with uppercase section labels and 32 px rows;
   - a 320 px right panel of 46 px collapsible sections with an edited dot;
   - thin sliders whose fill runs from the neutral point, with a zero mark on bipolar
     ranges. An edited value becomes a "Reset" button on hover (double-click still
     resets).
   - The design has no Settings screen. Settings opens from a gear in the top bar and
     uses the same tokens (grouped cards, segmented control).
6. **Only working UI.** Design elements whose features do not exist yet are left out:
   - search, collections, albums, histogram, presets;
   - crop and masks tools, undo/redo, the filmstrip.
   Each is added, styled per the design, when its feature is built.
   The Library grid (3:2 cards with a caption, 16 px by 20 px gaps, a column count
   that keeps cards at most 280 px wide) was the first screen added this way. A
   Grid/List switch keeps the details list, which the design does not have.
   Ratings and flags followed (ADR 0018): stars and a pick badge on cards, rejects at
   38% opacity, a white selection ring, and the star and flag controls in Edit's header.
   One deliberate change: a click *selects* a card and a double-click opens it (the
   design opens on click), because culling needs a selection.

## Following the design as screens are built

- **The reference** is the design artifact "RAW Photo Editor"
  (claude.ai/artifact/GRNipwGPYLYuQNfM8NpH3A, file `Main.dc.html`).
- **The rule:** every UI change starts from it, copying structure, placement and
  styles. Anything placed differently, or not in the design, is listed here.

### Built as designed

| Design element | Status |
|---|---|
| Library grid (3:2 cards, caption, stars, pick badge, rejects at 38%) | as designed |
| Library header filter "All / Picks / ★ 3+" | as designed |
| Sidebar Picks / Rated / Rejected with counts | as designed |
| Edit header: name, mono file details, "● Edited", "N of M" counter | as designed |
| Floating photo toolbar under the photo: rating stars, pick, reject | as designed (zoom, crop, masks and compare to follow) |
| Panel: exposure strip, collapsible sections, sliders with reset-on-hover | as designed |
| Light section: Exposure (in EV), Contrast, Highlights, Shadows; "More controls" with the tone curve graph, then Whites, Blacks and Dehaze | as designed (the curve is the renderer's real response, ADR 0029) |
| Colour section: Temperature in kelvin on a blue–amber track, Tint on a green–magenta track, Vibrance, Saturation | as designed |
| Detail section: Texture, Clarity, Sharpening (default 40) | as designed (Noise reduction, and Vignette / Grain behind "More controls" to follow) |
| Colour mixer behind "More controls": label and range name, eight dots (Blues first), Hue / Saturation / Luminance | as designed |

### Deliberate deviations

| Where | Deviation | Why |
|---|---|---|
| Library cards | Click selects, double-click opens (design: click opens) | Culling needs a selection to rate with the keyboard |
| Library header | Grid/List switch, Refresh, Set as default | The details list and folder actions predate the grid; the design has neither |
| Library sidebar | "Indexed photos" is a count, not a view | "All photos" as a library-wide view is not built yet |
| Edit header | "Open photo…" button | Opening a file outside the library; the design has no equivalent |
| Edit header | Reset (whole photo) and save notes ("Not saved…", "Couldn't save") | Honest save state for photos outside the library; one-step reset |
| Edit panel top | "Look: Standard / Flat" row where the design has Presets | Base looks (ADR 0022) are profile choices; presets are not built yet |
| Stage | Dimmed photo and "Loading…" pill while the next photo opens | ADR 0020; the design shows no loading state |
| Light section | Exposure range ±5 EV (design ±4); "More controls" opens itself when a hidden slider is edited | Existing edits are never clamped; an edit is never out of sight |
| Colour section | Temperature shows its relative value (e.g. "+20") on JPEGs | A JPEG has no as-shot light to express in kelvin (ADR 0024) |
| Colour mixer | Accent mark on the dots of edited ranges | Otherwise an edit in a range not on screen is invisible |
| Settings | Whole screen (grouped cards, segmented controls) | The design has no settings; built from its tokens and components |

### Not built yet (and so not shown)

Search, Recently imported, albums, histogram, presets and Auto, Noise reduction, Vignette, Grain, crop, masks, compare, zoom, the filmstrip, batch selection and Export
dialog.

## Consequences

- New screens start from the tokens and components above; the design artifact is the
  reference.
- The brand mark and "Photo Editor" are placeholders. Naming the product changes one
  component and the window title.
- A generic accent colour is not reserved for errors: errors use `--danger`, and the
  accent means "edited" or "focused".
