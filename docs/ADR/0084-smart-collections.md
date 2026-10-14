# ADR 0084: Smart collections

- Status: Accepted
- Date: 2026-10-14

## Context

PRODUCT §3.1 lists smart collections first among the Library's "Later" items. ADR 0055
deferred "smart albums". Lightroom users keep saved rules such as "★ 4 and up, picked,
2025" that fill themselves as photos are culled.

The Library already has the pieces: ratings, flags and colour labels (ADRs 0018, 0064),
edits, search words for folders, cameras, lenses and dates (ADR 0056), albums (ADR
0055) and a remembered last place (ADR 0065). The design has no smart collections.

## Decision

1. **A smart collection is a name and rules, all of which a photo must match:**
   - **Rating:** any, unrated, or at least one to five stars;
   - **Flag:** any, picked, not rejected, unflagged, or rejected;
   - **Label:** any, none, or one of the five colours;
   - **Edited:** either, edited, or not edited;
   - **Words:** as the search box takes them (ADR 0056): folders (places), cameras,
     lenses and capture dates.

   At least one rule is needed: no rules would be All photos. Names are tidied like
   album names ("Give the smart collection a name.").
2. **Stored as rules, matched each time** (`catalogue::smart`):
   - **Catalogue schema 10:** `smart_collections`, one column per rule, NULL for "any",
     each checked, so the catalogue needs no JSON. As with every upgrade, a backup is
     taken first (ADR 0021), and backups keep the collections.
   - The photos are looked up when the collection is opened, and the counts when the
     sidebar refreshes, so a collection is never out of date and stores no photo list.
   - Listing is as for any collection: present files in granted folders, by capture
     time, in the chosen sort.
3. **Commands:** `list_smart_collections`, `create_smart_collection`,
   `update_smart_collection` (name and rules), `delete_smart_collection`,
   `count_smart_rules` (while rules are chosen) and `smart_collection_photos`.
4. **Library UI** (built from Albums' pieces; recorded in ADR 0016):
   - **Sidebar:** a "Smart collections" section after Albums: an icon, name and count
     per row; **+** on its label or the **New smart collection** row opens the rules;
     an empty state explains them.
   - **The rules** are a popover: the name, then one labelled select per rule and the
     words field. It shows how many photos match as the rules change (counted 150 ms
     after the last change).
   - **From a search:** the search's header has **Save as smart collection**, which
     opens the rules with the search's words (and as the name).
   - **Viewing one:** the crumb reads "Smart collection · ★ 4 and up · Picked ·
     “iceland”". **⋯** opens the rules to change, rename or delete it. Deleting asks
     first; its photos stay.
   - **Marking a photo out of it** (a star less, a reject) takes it out of the view at
     once, as from Picks: the rules are mirrored on marks in the frontend. Words are
     the catalogue's to judge and don't change with marks.
   - **Counts** refresh at start, after indexing, and 400 ms after a burst of marks or
     edits settles.
   - **The last place** can be a smart collection (`smartCollection` in settings), and
     Edit's filmstrip and ← → step through it.
5. **Tests:**
   - **Catalogue:**
     - each rule alone and combined;
     - words as search takes them;
     - collections fill themselves as marks change;
     - listing by name with counts, changing and deleting;
     - missing files left out;
     - encodings round-trip.
   - **app-core:** names tidied; rules needed.
   - **Frontend:** marks matched as the catalogue does; the rules' summary.
   - **Release self-test:**
     - a photo labelled purple is found by a label rule with its folder's name;
     - collections without rules or names are refused;
     - a narrower rule changes the count;
     - the place is remembered, and deleting keeps the photo.
   - **The dev mock was checked in the browser:**
     - opening one;
     - a photo rated out of it leaving the view, with its count;
     - the rules popover counting as a rule changed, then saving;
     - saving a search as a smart collection;
     - deleting.
6. **Performance** (PERFORMANCE §60), on 20,000 photos:
   - rules on marks count in 1–7 ms;
   - words cost what a search does (12–85 ms);
   - the sidebar's five counts take about 0.1 s, on a blocking thread.

## Consequences

- A library opened by this version is at schema 10; an older version refuses it, as
  for every schema upgrade (the pre-upgrade backup restores it).
- **Later:**
  - rules on the camera or lens as such (words cover them for now);
  - date ranges ("the last 30 days");
  - "any of these rules";
  - membership of an album.
