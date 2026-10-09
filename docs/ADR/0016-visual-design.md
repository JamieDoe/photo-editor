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
| Library sidebar "All photos" with its count (ADR 0065) | as designed |
| Sidebar Picks / Rated / Rejected with counts | as designed |
| Edit header: name, mono file details, "● Edited", "N of M" counter | as designed |
| Floating photo toolbar under the photo: zoom ("Fit" / "100%"), crop, masks, compare, rating stars, pick, reject | as designed (zoom: ADR 0070) |
| Panel: exposure strip, collapsible sections, sliders with reset-on-hover | as designed |
| Histogram above the exposure details: red, green and blue filled, luminance line, clipping triangles | as designed (ADR 0036), and draggable as in Lightroom |
| Light section: Exposure (in EV), Contrast, Highlights, Shadows; "More controls" with the tone curve graph, then Whites, Blacks and Dehaze | as designed; the curve is editable (ADR 0037) |
| Colour section: Temperature in kelvin on a blue–amber track, Tint on a green–magenta track, Vibrance, Saturation | as designed |
| Detail section: Texture, Clarity, Sharpening (default 40), Noise reduction; "More controls" headed "Finishing" with Vignette and Grain | as designed |
| Photo toolbar "Crop" button; crop mode (dimmed outside, thirds grid, handles, size label; toolbar with ratios, Straighten, Reset, Done); Geometry section (aspect ratio, Straighten, Crop) | as designed, including Auto level |
| Geometry "More controls" headed "Perspective & lens": Vertical, Horizontal, Lens correction switch ("NIKKOR Z 24–70mm f/4 S · auto"), Remove chromatic aberration switch | as designed (ADRs 0034, 0035, 0075): Lens correction shows the lens's name and "· auto" when the photo's file has a profile, and "No lens profile in this photo's file", disabled, when it hasn't |
| Colour mixer behind "More controls": label and range name, eight dots (Blues first), Hue / Saturation / Luminance | as designed |

### Deliberate deviations

| Where | Deviation | Why |
|---|---|---|
| Library cards | Click selects, double-click opens (design: click opens) | Culling needs a selection to rate with the keyboard |
| Library header | Grid/List switch, Refresh, Set as default | The details list and folder actions predate the grid; the design has neither |
| Colour labels and sort (ADR 0064) | Not in the design: a label dot before the name on cards, list rows and the filmstrip; five swatches after pick/reject in the Edit toolbar; a five-swatch label filter and a Sort menu (Capture time, Newest first, File name, Rating) after the header filter | Asked for (Lightroom parity for culling); built from the design's own pieces (toolbar buttons, the small segmented container, the settings select) |
| Edit header | "Open photo…" button | Opening a file outside the library; the design has no equivalent |
| Edit header | Edited and save notes ("Not saved…", "Couldn't save"); Reset moved to the panel footer as in the design (ADR 0048) | Honest save state for photos outside the library |
| Edit header on narrow stages | The file details, then the name, end in an ellipsis (in full on hover); below a 760 px stage the save note shortens to "Not saved" (in full on hover); below 600 px "Open photo…" becomes an icon button | The design has no narrow layout; the header must fit the 800 px minimum window (a 480 px stage) without hiding an action |
| Edit panel top | "Look: Standard / Flat" row where the design has Presets | Base looks (ADR 0022) are profile choices; presets are not built yet |
| Stage | Dimmed photo and "Loading…" pill while the next photo opens | ADR 0020; the design shows no loading state |
| Light section | Exposure range ±5 EV (design ±4); "More controls" opens itself when a hidden slider is edited | Existing edits are never clamped; an edit is never out of sight |
| Colour section | Temperature shows its relative value (e.g. "+20") on JPEGs | A JPEG has no as-shot light to express in kelvin (ADR 0024) |
| Colour mixer | Accent mark on the dots of edited ranges | Otherwise an edit in a range not on screen is invisible |
| Crop mode | Handles sit just inside the rectangle (design: 3 px outside); the crop view is the largest straightened area in the photo's shape | Handles stay whole at the photo's edge; no empty corners can be chosen (ADR 0032) |
| Auto level | A status line under the button / above the crop toolbar | The design shows a toast; the app has no toast system yet (ADR 0033) |
| Crop toolbar | Rotate left, Rotate right and Flip icon buttons after the ratios; on narrow stages the Straighten label goes, then the ratios (still in the Geometry panel) | The design has no rotation (ADR 0039); the toolbar must fit the 800 px minimum window |
| Masks (ADR 0040): Masks button, mask toolbar (chips, Add, overlay switch, Done), accent tint with a gradient's three lines, Selective section (rows, Exposure / Warmth / Clarity card, Add tiles) | Subject, Sky, Brush, Linear and Radial offered, and People (not in the design) after Sky, Subject and People only where the computer makes them (ADR 0074); "Subject · detected" as the design's row kind, the design's green dot for Subject and blue one for Sky, and a blue-green one for People; six Add tiles as two rows of three; a generated mask has no handles; "Finding the subject…" on its button while it is made; with five kinds, the card's Add and Subtract rows put the kinds on their own line; the brush's Paint/Erase, Size, Feather and Flow in the mask card, and a minus in the brush rings when erasing (ADR 0042); start and end handles on a linear gradient's dashed lines; a radial's side and top handles and its dotted fade-start ellipse; Feather (radial) and Invert in the mask card, and an eye button on each mask row to hide it (ADR 0041); Density, the mask's shapes (with each one's Add/Subtract/Intersect menu) and Add and Subtract rows of Brush, Linear and Radial in the mask card (ADR 0043); empty-state text naming what can be detected ("pick out the subject or the sky") | The handles set angle, size and fade directly; the product requires feather, invert, add, subtract and density on every mask |
| Tone curve | The photographer's own point curve: points to add, drag and remove, a readout and Reset in the title row, the histogram behind. It does not bend with the sliders as the mock-up's does. An RGB · Red · Green · Blue switch above the graph (ADR 0038) | Requested (a true curve, as in Lightroom); a curve that both follows the sliders and takes points would be ambiguous |
| Histogram | Dragging adjusts Blacks, Shadows, Exposure, Highlights or Whites by zone; hovering tints the zone and shows its name and value in place of the exposure details; multiply blending on the light theme | Requested (Lightroom's behaviour); the readout says what a drag will change; the design is dark only |
| Remove chromatic aberration | The switch's subtitle says "Measuring…" and then the result, for three seconds | Turning it on measures the photo; the design has no in-progress state |
| Settings | Whole screen (grouped cards, segmented controls) | The design has no settings; built from its tokens and components |
| Panel footer (ADR 0048) | Reset, Copy, Paste (dimmed until something is copied) and the toast, as designed | A chevron beside Copy chooses which setting groups to copy (crop and masks off by default); ⇧⌘C / ⇧⌘V |
| Filmstrip and batch (ADR 0049) | 128 px strip of 96×64 thumbnails with stars, edited dot and pick flag; white ring on the open photo, accent ring and tick on selected ones; filter switch; "N selected · Sync edits · ×" | The Library's filters (★ 3+ for Rated); ⌘-/⇧-click selection; the open and ticked rings drawn inside the thumbnail's edge (outside it, as designed, two neighbours' rings met across the 8 px gap); Export… opens the export dialog (ADR 0050) |
| Export dialog (ADR 0050) | Header with thumbnail, count and names; preset tiles; Quality and Size rows; Save to; Cancel and Export; the finishing toast | Web, Social and Full quality (a 16-bit TIFF, ADR 0057) only; the Format row offers JPEG, TIFF and PNG (not HEIC); Sharpen for (ADR 0059) adds None to Screen, Matte and Glossy; Colour space (ADR 0062) offers sRGB, Display P3 and Adobe RGB, converted and tagged, but holding sRGB's colours (ADR 0060), which its hints say; Keep metadata and Strip location (ADR 0063) are built as two tiles, Strip location disabled while nothing is kept; the estimate beside Cancel (ADR 0068) is the open photo's, as "each" for several; the Watermark switch (ADR 0069) opens a row for its text, position (six frame icons, Repeat among them) and size (S, M, L), which the design doesn't show; progress and stop in the top bar |
| Tone curve regions and B&W mix (ADR 0051) | — | "Regions" sliders (Highlights, Lights, Darks, Shadows) under the tone curve graph; the colour mixer titled "B&W mix", with a hint, when Saturation is −100 |
| Colour grading (ADR 0052) | — | A "Colour grading" section after Detail: a Shadows/Midtones/Highlights/Global switch, one colour wheel with a hue and strength readout, Luminance, Blending and Balance; built from the design's tokens |
| Calibration (ADR 0053) | — | A "Calibration" section after Colour grading, closed by default: Shadows Tint, then each primary's Hue and Saturation under small uppercase headings, with coloured tracks like the white balance sliders; a new icon of three overlapping primaries |
| Retouch (ADR 0054) | Section with Remove, Heal and Clone, a hint, Brush size 5–200; a sensor-dust banner with Fix all; dashed amber rings on the photo | Remove, Heal and Clone (Remove since ADR 0066: drag to paint, filled on release; a small pin where each removal started, its area shown when selected); the selected spot white with its dashed source joined to it; the spot count and Clear all under the slider; opening the section puts the photo in retouch mode (click to add, drag to move, Option-click for the source, Delete, `[` `]`); Red eye (ADR 0080) as a fourth tool, not in the design: click on an eye, dotted rings for corrections, Pupil size and Darken for the selected one |
| Sensor dust (ADR 0058) | "2 sensor dust spots found in the sky · Fix all", then "2 spots removed · Undo"; dashed amber rings on the photo | "N sensor dust spots found" without "in the sky" (no scene classification); clicking a ring heals that spot; spots made by hand show as a quiet white ring, leaving dashed amber for dust found |
| Albums (ADR 0055) | Sidebar "Albums" section: swatch, name and count per album | The first photo as the swatch (the gradient while empty); + on the label and a New album row for a new album, an empty state without albums; Folders and Albums fold under their labels; ⌘/⇧-click ticks in the grid and list with the accent ring and "N photos ticked ×"; Add to album, Remove from album and a ⋯ rename/delete popover in the header; drag photos onto an album |
| Search and Recently imported (ADR 0056) | Search field at the top of the Library sidebar ("Search places, cameras, dates"); "Recently imported" with a count | "Search folders, cameras, dates" (places come from folder names; GPS has no place names yet); a clear button; results as their own view ("Search · across all folders", the query as the title) |
| Recently edited (ADR 0079) | — (not in the design) | A row below Recently imported, with the pencil icon and a count: photos edited in the last 30 days, the latest first; the sort menu shows "Last edited", disabled, in that view |
| Favourites (ADR 0081) | — (not in the design) | A row after Rated, with a filled star and a count: the photos rated five stars |
| Presets (ADR 0046) | Strip under the histogram, 64×44 previews with names, accent ring on the applied one | Save… and Import… before the design's Auto (ADR 0071, built as designed, with ⇧⌘U; Shift-double-click on a tone slider autos that one, as in Lightroom); Export… in a saved preset's menu, and a popover listing what an import brought in (ADR 0047); ⋯ on a saved preset opens a popover to rename, update or delete it; previews are real renders of the photo |
| Undo and redo (ADR 0044) | Two icon buttons in the Edit header, between the save status and the photo counter; ⌘Z and ⇧⌘Z | The design has no undo control; placed with the edit's other status and actions |
| Before / after (ADR 0045) | Compare button, white divider and round handle, Before and After pills, drag anywhere on the photo | The `\` shortcut; "Before…" while the before image renders; no focus-header button (that header is not built) |
| Focus mode (ADR 0072) | Top bar icon button, accent while on; hides the side panels and the filmstrip | Before Settings (the design has no Settings button); F toggles it and Esc ends it; the photo toolbar and name row stay |
| Zoom (ADR 0070) | "Fit" / "100%" button first in the photo toolbar | The button shows the level ("67%"); Z toggles Fit and 100 %, animated; a click on the photo zooms in on that spot, a click when zoomed fits; drag or scroll to pan; pinching zooms smoothly from Fit to 800 %; ⌘+ / ⌘− step through levels, ⌘0 fits; crop and compare fit it; in retouch and mask modes clicks paint, so scroll or Space-drag pans, and the mask toolbar starts with the same zoom button |

### Not built yet (and so not shown)

Search, Recently imported, albums, presets, compare,
the filmstrip, batch selection and the Export dialog.

## Consequences

- New screens start from the tokens and components above; the design artifact is the
  reference.
- The brand mark and "Photo Editor" are placeholders. Naming the product changes one
  component and the window title.
- A generic accent colour is not reserved for errors: errors use `--danger`, and the
  accent means "edited" or "focused".
