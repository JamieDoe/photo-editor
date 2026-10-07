# ADR 0065: All photos, and remembering the Library's view and place

- Status: Accepted
- Date: 2026-10-07

## Context

The design's sidebar starts with **All photos** and its count. The app showed
"Indexed photos" as a count you could not click (recorded as a deviation in ADR
0016). Seeing the whole library at once is how photographers browse, and the new sort
and colour label filter (ADR 0064) are most useful across everything.

The Library's view choices (grid or list, the star filter, the label filter and the
order) also reset at every launch, so each session started by setting them again.
The Library also always started in the default folder, wherever it had been left. The
photographer asked for it to reopen where they left off, as Lightroom does.

## Decision

1. **All photos is a library-wide collection,** beside Recently imported, Picks, Rated
   and Rejected:
   - every photo whose file is present, inside the granted folders;
   - shown as the design has it: the sidebar's first row, with its count;
   - opened like the other collections: "Across all folders", with the filter, label
     filter and sort.
   - **The count is of present photos.** The old "Indexed photos" figure also counted
     photos whose files had gone missing. Counting present photos is folded into the
     Recently imported query, so it needs no extra scan.
2. **Listing a long collection without a filesystem call per photo:**
   - Collections, searches and albums keep only photos inside granted folders. That
     check canonicalised every path: one system call each.
   - Catalogue paths are already canonical (stored that way when indexed), so listings
     now use `FolderAccess::covers`: a prefix check against the granted roots, with no
     disk access.
   - It refuses any path that is relative or contains `..`, rather than trusting it.
   - **Reading a listed file still goes through the full check**
     (`FolderAccess::check`): thumbnails, opening and exporting are unchanged. A path
     that moved out of a grant since indexing is at worst listed, never read.
   - **Cost at 20,000 photos** (release bench, `--index-scale 20000`, two runs):

     | Step | Time |
     |---|---|
     | Catalogue query | 26 ms |
     | Grant filter, canonicalising | 234–238 ms |
     | Grant filter, prefix check | 3.4–3.5 ms |
3. **The Library's view is remembered** in settings as `library.view`:
   - layout (grid or list);
   - the filter (All, Picks, ★ 3+);
   - the label shown alone, if any;
   - the sort.
   - **Lifetime:** they still survive switching to Edit, and now quitting too. They are
     global, not per folder.
   - **Reading:** each field is read on its own. A value this version doesn't know
     (written by a newer one) is that field's default, and the rest still load. One
     small `Deserialize` impl does this; field-level `deserialize_with` would have
     confused the TypeScript generator.
   - **Saving:** changes go through the existing debounced settings save. Stored
     choices are adopted once, when settings load, and never over a choice made
     since. The view holds no paths, so the guard that keeps folder paths out of UI
     updates is unaffected.
   - **Shared types:** the frontend's layout, filter and sort types are now the
     generated ones, not separate copies.
4. **The Library reopens where it was left.**
   - **What's recorded:** the last folder, collection (including All photos) or album
     opened, as `library.lastPlace`. A search is not recorded: it's transient, and
     the view under it is.
   - **At launch:** that place opens. The default folder is now the fallback, used
     when the place is gone (a deleted album, a disconnected drive), on first launch,
     or before anything has been opened.
   - **Recorded by Rust, never by a settings update:** folders in settings are never
     set from the UI. A `remember_place` command stores a folder only when it lies
     inside a granted folder, by its canonical path. It grants nothing, and the folder
     is checked again when it is listed at the next launch.
   - **The guard:** the one that keeps folder paths out of UI settings updates also
     keeps `lastPlace` as stored. Unchanged places are not rewritten.
   - **Reading:** like the view, the place is read leniently. A kind this version
     doesn't know (or damage) means no place, and the default folder opens.
5. **Tests:**
   - **Catalogue:** All photos lists the present photo but not a missing one, and the
     counts include it.
   - **Folders:**
     - `covers` accepts paths under a grant (files need not exist);
     - it refuses siblings, paths elsewhere, `..` and relative paths.
   - **Settings:**
     - the defaults;
     - a stored view loads;
     - unknown values fall back field by field while the rest load;
     - the JSON written.
   - **Settings and guard:**
     - each place kind round-trips;
     - an unknown kind or collection reads as no place, and the rest of the library
       settings still load;
     - a UI update cannot set the place.
   - **Release self-test** (`lastPlace`):
     - a collection is recorded;
     - `<folder>/.` is recorded as the canonical folder;
     - `/` (outside the grants) is refused and leaves the place as it was;
     - a settings update that tries to change it is ignored.
   - **Release self-test** (`allPhotosAndView`):
     - All photos lists as many photos as the sidebar counts, including every photo
       in the test folder;
     - a view (list, picks, blue, newest first) round-trips through the settings
       store.
   - **Dev mock:** the sidebar row (2,400), and the view's title and crumb.

## Consequences

- **Very large libraries** send the whole collection over IPC as one listing, as
  every collection already does. If libraries of hundreds of thousands of photos
  appear, the listing can be paged; the grid is already virtualised.
- **The selected photo and scroll position** are not restored; the place opens at the
  top. They can be added to the same record if wanted.
- **Per-folder views** (as Lightroom keeps them) can replace the global view later,
  keyed by folder or collection, if the global one proves limiting.
