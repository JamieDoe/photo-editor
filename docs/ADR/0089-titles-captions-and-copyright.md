# ADR 0089: Titles, captions, creator and copyright

- Status: Accepted
- Date: 2026-10-14

## Context

Photographers who share or sell their work put their name and copyright into every
exported file, and describe their photos with a title and caption that agencies,
websites and other apps read. Lightroom keeps these in its Metadata panel and writes
them as XMP (`dc:title`, `dc:description`, `dc:creator`, `dc:rights`) and EXIF
(ImageDescription, Artist, Copyright). The app had none of them; keywords (ADR 0088)
added the XMP writing they share.

## Decision

1. **Per photo, in the catalogue** (schema 12): `title`, `caption`, `creator` and
   `copyright` columns on photos, NULL for none.
   - **They follow photos** across moves and renames, like marks and keywords.
   - **Tidied:**
     - The title, creator and copyright are one line, at most 200 characters.
     - The caption keeps its line breaks (at most one blank line in a row), at most
       2,000 characters.
     - Empty clears the field.
2. **The usual creator and copyright, in Settings → Export**, written into every export
   of a photo without its own.
   - **Cut, not trimmed:** the stored value is only cut to 200 characters (as the
     watermark text is), because it comes back into the field while it is typed in.
     Exports trim it.
3. **Exports** write them when "Keep metadata" is on:
   - **XMP:** `dc:title`, `dc:description` and `dc:rights` (each an `rdf:Alt` in the
     default language) and `dc:creator` (an `rdf:Seq`).
   - **EXIF:** the caption, creator and copyright as ImageDescription, Artist and
     Copyright.
   - **TIFF text:** written as UTF-8, as JPEG and PNG EXIF carry it. The `tiff`
     crate's own ASCII writer refuses "©".
   - **With "Keep metadata" off, nothing is written,** copyright included: the switch
     means what it says. A "copyright only" choice, as Lightroom has, can follow if
     wanted.
4. **Search** matches titles and captions (PERFORMANCE §64: about 2%).
5. **Editing: one popover** ("Title and caption") for the same photos as Keywords.
   - **Opened from** a Caption button in the Library header and a lines button in the
     Edit header.
   - **Fields:** Title, Caption, Creator and Copyright. Creator and copyright show the
     Settings ones as placeholders.
   - **Several photos:** a field they differ in reads "Mixed" and is left alone unless
     typed into, so captioning a selection doesn't wipe their titles.
   - **Saving:** each field is saved 0.5 s after typing rests, and on closing.
   - Recorded in ADR 0016: the design has neither.
6. **Not in sidecars yet.** Sidecars (ADR 0067) carry marks and keywords only; Lightroom
   users' captions aren't read in either. Writing them back needs the same "leave
   another app's" rules as keywords, and can follow.

## Consequences

- Every export can carry the photographer's name and copyright without per-photo work,
  and agencies' and websites' fields fill from the title and caption.
- A caption can be found again by searching for a word in it.
- **The product name and identifier** are still placeholders.
