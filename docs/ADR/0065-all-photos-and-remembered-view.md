# ADR 0065: All photos, and remembering the Library's view

- Status: Accepted
- Date: 2026-10-07

## Context

The design's sidebar starts with **All photos** and its count. The app showed
"Indexed photos" as a count you could not click (recorded as a deviation in ADR
0016). Seeing the whole library at once is how photographers browse, and the new sort
and colour label filter (ADR 0064) are most useful across everything.

The Library's view choices (grid or list, the star filter, the label filter and the
order) also reset at every launch, so each session started by setting them again.

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
4. **Tests:**
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
- **Per-folder views** (as Lightroom keeps them) can replace the global view later,
  keyed by folder or collection, if the global one proves limiting.
