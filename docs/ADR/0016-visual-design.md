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

## Consequences

- New screens start from the tokens and components above; the design artifact is the
  reference.
- The brand mark and "Photo Editor" are placeholders. Naming the product changes one
  component and the window title.
- A generic accent colour is not reserved for errors: errors use `--danger`, and the
  accent means "edited" or "focused".
