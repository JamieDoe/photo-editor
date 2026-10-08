# ADR 0072: Focus mode

- Status: Accepted
- Date: 2026-10-08

## Context

The design's top bar has a Focus mode button ("Focus mode — hide panels"): an icon of
a frame with its panels' edges, accent-tinted while on. On, the side panels and the
filmstrip go and the photo takes their room. Turning it on goes to Edit. With zoom
(ADR 0070), a larger photo is worth more.

## Decision

1. **The button:** the design's icon button in the top bar, before Settings,
   `aria-pressed` and accent-tinted while on (`--accent`, `--accent-soft`). Turning
   it on from another workspace goes to Edit. Focus mode belongs to Edit: leaving Edit
   ends it.
2. **What it hides:** the adjustments panel and the filmstrip.
   - **What stays:** the top bar, the photo's name row (undo, redo, the counter) and
     the photo toolbar (zoom, crop, masks, compare, marks).
   - **Still usable in focus:** the tools on the photo, zoom, and the keyboard (marks,
     arrows, Z, ⇧⌘U).
   - **Kept while hidden:** the panel stays mounted (`hidden`), keeping its scroll,
     its open sections, and retouch mode if it was on.
3. **Keys:**
   - **F toggles it.** Lightroom's Tab would take keyboard navigation away.
   - **Esc ends it,** unless a tool (crop, masks, compare) takes Esc for itself.
4. **Tests:** release self-test `focusMode`: the button hides the panel, the viewer
   grows by the panel's width, and F brings it back to the same size. The dev mock
   was checked too: from Library the button goes to Edit with the panel hidden, F
   toggles, and Esc ends it.

## Consequences

- The photo gains the panel's 320 px; the viewer refits (and zoom keeps its centre).
- **Not done:** hiding the top bar too (Lightroom's ⇧Tab), and a full-screen window.
