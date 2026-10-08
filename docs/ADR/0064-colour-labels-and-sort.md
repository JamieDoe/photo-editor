# ADR 0064: Colour labels and sorting

- Status: Accepted
- Date: 2026-10-07

## Context

Culling in other photo tools leans on three marks: stars, pick/reject flags and colour
labels. Photographers give labels their own meanings ("to print", "client picks",
"needs retouching"). The Library had stars and flags (ADR 0018) but no labels, and it
always listed photos in one fixed order: folders by file name, collections, searches
and albums by capture time. Lightroom users switching over expect both.

The design has neither labels nor a sort control. The photographer asked for them (the
roadmap's next item after export), so they are built from the design's own pieces and
recorded as a deviation in ADR 0016.

## Decision

1. **A colour label is a third mark** on the photo, beside rating and flag:
   - none, red, yellow, green, blue or purple, as in Lightroom;
   - stored like the others: on the photo (so it follows a moved file), never written
     into the original;
   - **catalogue schema 9:** a `label` column (0–5, checked) and a partial index. As
     with every schema upgrade, a pre-upgrade backup is taken first (ADR 0021).
   - A photo with only a label is listed with the folder's marks. Collections,
     searches and albums carry it.
2. **Setting a label:**
   - **Keys** 6, 7, 8 and 9 for red, yellow, green and blue, as in Lightroom (purple
     has no key there either). Pressing the key of the label a photo already has
     removes it, as clicking the current star or flag does.
   - **Edit toolbar:** five swatches after pick/reject. The current one is ringed;
     clicking it removes it.
   - In the Library, the keys apply to the selected photo, as ratings and flags do.
3. **Seeing labels:** a 7 px dot before the name on cards, in the list's marks column
   and on filmstrip thumbnails. The five hues are tokens (`--label-red`…) shared by
   both themes.
4. **Filtering by label:** five swatches in the Library header, after All / Picks /
   ★ 3+. Choosing one shows only that label; choosing it again shows all. It combines
   with the other filter (say, ★ 3+ and red), with collections, searches and albums.
5. **Sorting:** a Sort menu in the Library header:
   - **Capture time** (oldest first, the default as in Lightroom), **Newest first**,
     **File name** and **Rating** (highest first, then capture time);
   - **File names sort as people read them:** DSC_9 before DSC_10, case ignored
     (`Intl.Collator`, numeric).
   - Photos without a known capture time (not indexed yet) come after the rest, by
     name. Ties go to the name, so the order is stable.
   - It applies to every view: folders, collections, searches and albums. Edit's ← →
     and the filmstrip follow the same order.
   - **Behaviour change:** folders used to list by file name and now default to
     capture time. For most camera folders the two orders match.
   - Sort and the label filter last for the session, like the grid/list layout and
     the filter. (Since ADR 0065 all four are remembered across launches.)
6. **Tests:**
   - **Catalogue:**
     - labels round-trip through the stored encoding;
     - a label is independent of rating and flag;
     - a photo with only a label lists with the folder's marks;
     - collections carry it, and it can be cleared;
     - the schema migrates to 9.
   - **Frontend:**
     - keys 6–9 map to labels, and the current label's key removes it;
     - clicks toggle;
     - a change leaves the other marks alone;
     - the label filter alone and combined with ★ 3+;
     - each sort order, with unindexed photos last and natural name order;
     - sorting leaves its input untouched;
     - sorting does n log n work, checked by counting reads of the photos, not by
       timing (PERFORMANCE.md §46); 20,000 photos sort in 33–58 ms.
   - **Release self-test:** purple is set through the real command, read back from the
     folder listing and the Picks collection, still listed once rating and flag are
     cleared, then removed.
   - The dev mock was checked in the browser: dots on cards, the header filter
     ("97 photos of 2,400" for green), File name order, and the Edit toolbar picker
     (8 removes green).

## Deviations from the design

Recorded in ADR 0016: the label dot, picker, filter and Sort menu are not in the
design.

## Consequences

- **Smart collections** (say, "red and ★ 4+") could build on the same marks later.
- **XMP:** writing labels and ratings into XMP sidecars or exports (ADR 0063
  consequences) would carry them to other tools.
- **Persisted view choices:** remembering sort, layout and filters across launches
  (per folder, as Lightroom does, or globally) is a small settings change when wanted.
  Built globally in ADR 0065.
