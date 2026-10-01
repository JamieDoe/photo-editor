# ADR 0055: Albums

- Status: Accepted
- Date: 2026-10-01

## Context

PRODUCT §3.1 lists collections and albums for V1, "implemented as database references
rather than copies".

The Library had folders and three built-in collections (Picks, Rated, Rejected). It
had no way to group photos of the photographer's own choosing, for a portfolio, prints
or a trip spread across folders.

The design's Library sidebar has an **Albums** section: each row has a small swatch,
a name and a count. It also has a search box and a "Recently imported" entry, which
are separate steps (see Consequences).

## Decision

1. **Catalogue schema 7:** `albums` (id, name, timestamps) and `album_photos`
   (album, photo, added time).
   - Membership references the photo, not its file, so an album follows a photo when
     its file is moved or renamed, as marks and edits do. A photo can be in any number
     of albums.
   - Deleting an album, or taking a photo out of one, never touches the photo or its
     file. Deleting a photo (its folder removed from the library) cascades its
     memberships.
   - Like marks, albums cannot be rebuilt from the files, so the catalogue's backups
     (ADR 0021) keep them.
2. **Listing:**
   - Albums are listed by name, ignoring case. Each has the count of its present
     photos and a cover: its first present photo by capture time, shown only if its
     folder is granted.
   - An album's photos are listed like a collection: present files only, oldest
     capture first, and only those inside granted folders.
3. **Commands:** `list_albums`, `create_album` (with photos), `rename_album`,
   `delete_album`, `add_to_album`, `remove_from_album` and `album_photos`.
   - Photos are named by path and must be inside granted folders, as for marks. A
     photo its folder's index has not reached yet is recorded on the spot.
   - Names are tidied: spaces closed up, at most 60 characters, never empty
     ("Give the album a name.").
4. **Library UI:**
   - **Sidebar:** the design's Albums section after Folders. Each row has a cover
     thumbnail (the design's gradient swatch while empty), the name and the count.
     **+** on the label, or the **New album** row under the list, makes a new, empty
     album with an inline name field. Click a row to view it.
   - Without albums, the section shows a short empty state with a New album button.
   - Folders and Albums fold away under their labels (remembered per viewer).
   - **Drag and drop:** the window's native file drop (Tauri's `dragDropEnabled`) is
     off. The app takes no files dropped from the Finder, and on macOS it swallowed
     the page's own drags, so photos could not be dragged onto albums.
   - **Header:** on a narrow window, the header's actions wrap onto their own rows
     under the title instead of overflowing.
   - **Ticking:** in the grid and list, ⌘-click ticks a photo and ⇧-click ticks a
     range. These are the filmstrip's batch ticks (ADR 0049), shared between
     Library and Edit. The accent ring marks ticked photos, and the header shows
     "N photos ticked ×".
   - **Adding photos:**
     - drag photos onto an album in the sidebar (dragging a ticked photo drags all the
       ticked ones; the row lights while they are over it);
     - or use **Add to album** in the header, which lists the albums and can make a
       new one with the photos.
     - It acts on the ticked photos, or else the selected one, and a toast confirms
       ("Added 3 photos to Portfolio", or "Already in Portfolio").
   - **Viewing an album:**
     - the header reads "Album", then the name, count and date range;
     - **Remove from album** takes the ticked or selected photos out;
     - **⋯** opens a popover to rename the album or delete it (after asking; "deleting
       an album keeps its photos").
     - Edit's filmstrip and ← → step through the album, as for a collection.
5. **Tests:**
   - **Catalogue:** albums hold photos, follow a moved photo, leave out missing files,
     ignore duplicates and unknown albums; removing, renaming and deleting keep the
     photos.
   - **app-core:** name tidying.
   - **Release self-test:** through the real commands and catalogue, an album is
     made, added to (a duplicate ignored), listed with a cover, removed from, renamed
     and deleted, and its photos are still there.

## Deviations from the design

Recorded in ADR 0016:

- An album with photos shows its first photo as the swatch; empty ones keep the
  design's gradient.
- The **+** on the Albums label, Add to album and Remove from album, the ⋯ popover,
  ⌘/⇧-click ticks and drag-to-album are additions: the design shows albums but not
  how they are filled.

## Consequences

- **Not built yet:** the sidebar's **Search** ("places, cameras, dates") and **Recently
  imported** are the next steps.
- **Later:** colour labels and sort orders (in PRODUCT §3.1, not in the design), smart
  albums, and nested albums.
