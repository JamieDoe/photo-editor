# ADR 0088: Keywords

- Status: Accepted
- Date: 2026-10-14

## Context

The library is organised by folders, marks, albums and smart collections. Keywords, the
words a photographer attaches to photos to find them later, were the last core library
feature missing before launch. PRODUCT.md lists keywords among the application metadata
the catalogue keeps. Lightroom users keep years of keywords in XMP sidecars and expect
them to come across.

## Decision

1. **Stored in the catalogue** (schema 11): a `keywords` table (each name once,
   ignoring ASCII case) and `photo_keywords` links.
   - **They follow photos**, not files, so they survive moves and renames, like marks
     and albums; backups keep them.
   - **One spelling:** adding "heron" where "Heron" exists links the existing one.
   - **Names are tidied:** trimmed, runs of whitespace made one space, at most 100
     characters.
   - **Unused keywords are forgotten:** one no photo has any more is deleted when it is
     taken off.
   - **Flat for now:** Lightroom's keyword hierarchy (parents, synonyms, export
     flags) can follow if asked for.
2. **Finding photos by keyword.**
   - **Search:** each search word also matches keywords containing it.
   - **Smart collections:** a Keyword rule matches one keyword exactly, ignoring case
     (a new nullable `keyword` column).
   - **Speed:** a word is matched with `p.id IN (…)`, worked out once per query
     (PERFORMANCE §63).
3. **Editing: the same popover from Library and Edit.**
   - **Where:** a "Keywords" button in the Library header, for the selected or ticked
     photos, and a tag button in the Edit header, for the open photo and any ticked in
     the filmstrip. ⌘K opens either, as in Lightroom.
   - **Chips:** the photos' keywords; one only some of them have shows how many
     ("2/5") and a + to add it to all.
   - **The field** takes several keywords separated by commas, and suggests the
     library's own as they are typed.
   - Recorded in ADR 0016: the design has no keywords.
4. **XMP, as other apps read it** (`dc:subject`, a bag of keywords):
   - **Exports** carry the keywords with the marks whenever "Keep metadata" is on.
   - **Sidecars** (when that setting is on, ADR 0067) get them when keywords change.
   - **Another app's sidecar:** only `dc:subject` is replaced; everything else is kept
     byte for byte.
   - **Lightroom's hierarchy:** its `lr:hierarchicalSubject` is trimmed to the paths
     whose last keyword is still there. It can't gain new ones: their parents aren't
     known.
   - **A sidecar's keywords are never wiped by a rating.** They are only made the
     photo's when keywords were just changed here, or the photo has some. Otherwise a
     photo whose sidecar holds another app's keywords, never brought in, would lose
     them when rated.
   - **Indexing:** photos new to the library take the keywords in their sidecars, as
     they take marks. Photos with their own keep theirs.

## Consequences

- Keywords added in the app reach Lightroom, Bridge and Capture One, through sidecars
  and exports, and Lightroom's come in with a folder.
- **Captions, titles, creator and copyright** follow in their own change; they share
  the XMP writing added here.
- Keywords embedded inside JPEG, TIFF or DNG files aren't read yet; nor are marks.
