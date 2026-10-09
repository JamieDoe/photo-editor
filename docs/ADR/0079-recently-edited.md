# ADR 0079: Recently edited

- Status: Accepted
- Date: 2026-10-09

## Context

PRODUCT §3.1 lists "Recently edited" among the Library's V1 views. The sidebar had
All photos, Recently imported (ADR 0056), Picks, Rated and Rejected. Getting back to
the photos being worked on, across folders, meant remembering where they were.

## Decision

1. **A library-wide collection** (`Collection::RecentlyEdited`): present photos whose
   edit was saved in the last 30 days.
   - **Same window as Recently imported**, from the edit's `updated_at_ms`, which the
     catalogue already keeps.
   - **Resetting an edit removes the photo**, because the edit is deleted.
2. **The latest edit first.**
   - **In the catalogue:** this collection is listed by edit time (the newest first),
     where other collections are listed by capture time.
   - **In the library:** the view keeps that order. The sort menu shows "Last edited",
     disabled; the other views keep the chosen sort.
   - **Why:** its use is getting back to recent work.
3. **The sidebar:** below Recently imported, with the pencil icon and a count. The
   count comes from `collection_counts` (`edited`).
4. **Staying current:** when an edit is saved, the counts are refreshed. When the
   view is showing, it is fetched again, so a photo just edited moves to the top.
5. **The view is remembered** like the others (`LibraryCollection::Edited`).

## Consequences

- **The design:** it has no "Recently edited" row; this one follows its Recently
  imported row (ADR 0016).
- **Favourites,** the other library item in PRODUCT §3.1, waits for a product
  decision on what it means: a view of existing marks, a separate heart, or favourite
  folders.
