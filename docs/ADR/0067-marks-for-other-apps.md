# ADR 0067: Marks for other apps

- Status: Accepted (part 1: exports; part 2: sidecars beside RAWs; part 3: reading
  other apps' sidecars)
- Date: 2026-10-07

## Context

Star ratings, rejects and colour labels (ADRs 0018, 0064) live only in the app's
catalogue. Photographers move files between tools: an export opened in Lightroom,
Bridge, Capture One, Photos or the Finder showed none of their culling. Those tools
share marks through XMP, Adobe's metadata format, which every one of them reads.
ADR 0063 left this open: rating and keywords "would go in XMP".

## Decision

1. **Exports carry the photo's marks as XMP**, with its other metadata:
   - **Rating:** `xmp:Rating`, 1–5 stars. A reject is written as `-1` in place of
     the rating, as Bridge and Lightroom do.
   - **Colour label:** `xmp:Label`, by the names Lightroom and Bridge use (Red,
     Yellow, Green, Blue, Purple).
   - **Picks are not written:** no XMP property for them is shared between tools.
   - **Nothing to say, nothing written:** an unrated, unrejected, unlabelled photo
     gets no packet.
2. **One standard packet in each format's standard place:**
   - **JPEG:** an APP1 segment with Adobe's XMP signature, after the Exif segment and
     before the colour profile.
   - **PNG:** an uncompressed `iTXt` chunk named `XML:com.adobe.xmp`.
   - **TIFF:** tag 700 (XMLPacket), word-aligned like the Exif fields (ADR 0063).
3. **Governed by Keep metadata:** marks are metadata. With the switch off, no XMP is
   written; Strip location doesn't affect them.
4. **Where the marks come from:** the catalogue, by the photo's file.
   - **The export queue** looks them up for each photo alongside its saved edit.
   - **The single-photo export** looks them up for the open photo.
   - **A file opened from outside the library** has no marks, and none are written.
   - **A failed lookup** is logged, and the export goes ahead without marks.
5. **Tests:**
   - **The packet:** rating and label are written in the form other tools read; a
     reject is -1 whatever the stars; a label alone has no rating; nothing is written
     for no marks.
   - **JPEG:** the XMP segment comes after JFIF with the right signature and length.
   - **Every format and metadata choice:** a rated, labelled photo's JPEG, PNG and
     TIFF carry the marks wherever metadata is kept, and not with it off.
   - **macOS ImageIO** (which the Finder and Photos use) reads `xmp:Rating` 4 and
     `xmp:Label` Purple from all three formats.
   - **Release self-test** (`marksInExports`): the test photo, rated 4 and labelled
     purple through the real commands, exports 344 bytes larger with metadata kept,
     and byte-for-byte the same with it off.

## Part 2: sidecars beside RAWs

1. **An opt-in setting**, "Sidecar files for other apps" in Settings → Library, off
   by default: it writes files into the photographer's folders, which nothing else in
   the app does.
2. **What's written:** `NAME.xmp` beside a RAW, Lightroom, Bridge and Capture One's
   convention (`DSC_0012.NEF` → `DSC_0012.xmp`), with the same rating and label as
   exports.
   - JPEGs get none: other apps read their marks from inside the file, and originals
     are never modified.
   - RAW files are never written to.
3. **When:**
   - when a photo's stars, reject or label change (one or many at once), for the
     RAWs among them;
   - when the setting is turned on, for every marked RAW in the granted folders, in
     the background.
   - Turning it off leaves the sidecars written so far.
   - A sidecar that can't be written is logged and never fails the change that caused
     it.
4. **Never damaging another app's sidecar** (`export::sidecar`, a pure function
   tested without files):
   - Only the rating and label change. Every other byte stays, including attributes,
     elements, other namespaces and Lightroom's develop settings.
   - The prefix the file binds to the XMP namespace is used (Lightroom's `xmp`, older
     files' `xap`); one is declared only if none is.
   - Marks written as attributes (`xmp:Rating="3"`) or elements
     (`<xmp:Rating>3</xmp:Rating>`) are both replaced. Look-alike names
     (`xmpMM:Rating`) are not touched.
   - **Unchanged marks leave the file untouched**, so another app's sidecar is never
     rewritten for nothing.
   - **Unrecognised files are left alone:** a file with no `rdf:Description` is logged
     and not modified, and so is one larger than 4 MB.
   - **Deleting:** a sidecar is removed only when it is exactly this app's own packet
     with its marks cleared. Clearing marks in another app's file removes just the
     marks.
   - Writes are atomic (a temporary file renamed into place).
5. **Tests:**
   - **Sidecar update:**
     - a new sidecar is the export packet;
     - a Lightroom-style sidecar keeps its creator tool, develop settings, tone curve
       and toolkit, with one namespace declaration;
     - clearing marks removes only them;
     - element-form marks are replaced;
     - an older prefix is used, and a missing one is declared;
     - our own sidecar is deleted when cleared;
     - matching marks leave files untouched;
     - an unrecognised file is refused;
     - look-alike names are kept.
   - **On disk (app-core):**
     - a sidecar follows the marks (written, unchanged, deleted), and the RAW's bytes
       never change;
     - another app's sidecar is updated, not replaced, and stays when cleared;
     - an unreadable sidecar is left alone;
     - only RAWs get sidecars, named by Lightroom's convention.
   - **Catalogue:** the marked photos are listed for writing everything when the
     setting is turned on.

## Part 3: reading other apps' sidecars

1. **When a RAW first joins the library** (indexing records it as new) and has a
   sidecar beside it, `NAME.xmp`, its rating, reject and colour label are taken in.
   That covers adding a folder culled in Lightroom, Bridge or Capture One. Moved,
   changed and already-known photos are not touched.
2. **The app's own marks always win:** the catalogue sets imported marks only on
   photos with none of their own (one guarded update). Rescans never import again,
   so a mark cleared here never comes back from an old sidecar.
3. **Reading** (`export::sidecar::read_marks`, pure):
   - **Rating:** 1–5 is taken as stars, and -1 as a reject.
   - **Label:** recognised by its English name, any case.
   - **Ignored:** labels renamed in another app's custom label set, out-of-range
     ratings, and files without the XMP namespace.
   - Attributes, elements and the older `xap` prefix are all read.
   - What this app writes, it reads back.
4. **Only RAWs:** a JPEG has its marks inside it, and a `NAME.xmp` beside a JPEG
   belongs to the RAW of the same name. Sidecars over 4 MB aren't read.
5. **Read-only:** the sidecars are never written here. Part 2's opt-in setting
   decides that.
6. **Shown:** the Library's status after indexing says how many came across ("…
   · ratings and labels from 12 sidecars"), and the folder reloads with them.
7. **Tests:**
   - **Reading:**
     - a Lightroom-style sidecar's 2 stars and Yellow;
     - elements and `xap`;
     - a reject;
     - a label alone;
     - nothing from unrated files, unknown labels, out-of-range ratings or a missing
       namespace;
     - this app's own packet reads back.
   - **Catalogue:** imported marks go only to photos without marks of their own.
   - **Indexing (app-core, real files):**
     - two of three RAWs with sidecars take their stars, label and reject;
     - the RAW without one and the JPEG of a pair stay unmarked;
     - the sidecar is unchanged;
     - after a mark is cleared here and the sidecar changed in another app, a rescan
       imports nothing.
   - **Status text:** the count, singular and plural.

## Consequences

- **No two-way sync:** a sidecar changed in another app after a photo joined the
  library isn't read again; the app's catalogue stays the source of truth. A "read
  marks from sidecars" action could be added if photographers go back and forth.
- **Pick flags** stay this app's own: no shared XMP property holds them.
- **Keywords, titles and copyright** can join the same packet once the app has them.
