# ADR 0056: Search and Recently imported

- Status: Accepted
- Date: 2026-10-01

## Context

The design's Library sidebar has a search box at the top ("Search places, cameras,
dates") and a **Recently imported** entry among the library-wide views. PRODUCT §3.1
lists search and recently imported for V1. Albums (ADR 0055) filled in the rest of the
sidebar.

## Decision

1. **Recently imported** is a library-wide collection: present photos first indexed in
   the last 30 days.
   - "Imported" here means joined the library (a photo's `created_at_ms`). The app
     indexes folders in place rather than importing copies.
   - Catalogue schema 8 indexes that column.
   - It sits after Indexed photos with its count, opening like Picks.
   - A newly indexed library is all recent for 30 days, which is true.
2. **Search** finds the present photos (in granted folders) that match every word of
   the query. Up to 8 words are used. A word matches:
   - **as text:** the file's path from its library folder's own name down
     ("2026 Iceland/Day 1/DSC_0001.NEF"), or the camera make or model, or the lens.
     - Folders above the library are left out: they would match every photo.
     - Folder names are where places usually are, so a place finds its photos without
       location data.
   - **or as a date:**
     - a four-digit year (1900–2200);
     - a month name or its start of three letters or more ("sept");
     - a day of the month (1–31);
     - an ISO prefix ("2026-09").
     - Each of these also matches as text, so a number in a folder name still counts.
   - Matching ignores case, and `%` and `_` are taken literally.
3. **UI:**
   - The design's search box at the top of the sidebar searches as you type: 180 ms
     after the last key, only the newest query's results are kept.
   - Results show as their own view ("Search · across all folders", the query as the
     title), over the folder, collection or album underneath. Clearing the box (×,
     or Escape) brings that view back; opening another view clears the search.
   - Ticking, Add to album, marks, the filter and Edit's filmstrip work on results as
     on any view.
   - With nothing found, the view suggests what to try.
4. **Tests:**
   - **Catalogue:**
     - search finds photos by folder (a place), camera with lens, year, month name,
       month with day, and ISO prefix;
     - words combine; folders above the library never match; `%` is literal;
     - word parsing and pattern escaping;
     - the recent count leaves out missing files.
   - **Release self-test:** through the real commands and catalogue:
     - the fixtures folder's name, and the test photo's camera with its year, find it;
     - a nonsense word finds nothing;
     - Recently imported holds the photos just indexed and matches its count.
5. **Performance** (PERFORMANCE §38), on 20,000 photos:
   - a search takes 21–42 ms in the background, while typing pauses;
   - Recently imported takes 28 ms, and the counts 5 ms.

## Deviations from the design

Recorded in ADR 0016:

- The placeholder reads "Search folders, cameras, dates". Places are found through
  folder names; photos with GPS positions have no place names to search yet.
- There is a clear button in the field.

## Consequences

- **Later:**
  - place names from GPS positions (reverse geocoding, which must stay local);
  - keywords and captions (not stored yet);
  - a full-text index (SQLite FTS) if libraries far beyond 100,000 photos make the
    `LIKE` scan slow.
