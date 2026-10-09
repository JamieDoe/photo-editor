# ADR 0081: Favourites are the five-star photos

- Status: Accepted (decided with the photographer, 2026-10-09)
- Date: 2026-10-09

## Context

PRODUCT §3.1 lists "Favourites" among the Library's V1 views without saying what a
favourite is. The app already has three marks: picks and rejects (culling), ratings
(0 to 5 stars) and colour labels.

## Options

1. **A view of existing marks.** No new mark to learn. **Chosen.**
2. **A separate heart, as in Apple Photos.** A fourth way of saying "I like this",
   overlapping with picks. Lightroom users would have to decide what a heart means
   next to a pick, against "simple, not limited".
3. **Favourite folders pinned in the sidebar.** Useful, but navigation, not what
   "favourites" usually means. Recent folders and the default folder are already
   there.

## Decision

1. **Favourites is a library-wide view of the photos rated five stars**
   (`Collection::Favourites`).
   - **Not picks:** picks are for culling, and often many photos from a shoot. Five
     stars means the best, which is how Lightroom users already use it.
   - **Not Rated:** that view shows one star or more.
2. **The sidebar:** a row after Rated, with a filled star and a count.
   - **Empty:** "Press 5 to give the selected photo five stars and make it a
     favourite."
3. **Membership follows the mark:** starring or unstarring a photo while the view is
   open adds or removes it at once, as with Picks and Rated. The view is remembered
   like the others.

## Consequences

- No new mark, no schema change: a query on the rating.
- **The design:** it has no Favourites row; this one follows Rated (ADR 0016).
- PRODUCT §3.1's V1 Library list is complete.
