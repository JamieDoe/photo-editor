# ADR 0067: Marks for other apps (part 1: in exports)

- Status: Accepted (part 1: exports; sidecars and reading Lightroom's back are to
  follow)
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

## Consequences

- **Part 2, sidecars:** an `.xmp` file beside each RAW, which Lightroom and Capture One
  read when importing, so marks travel with the originals. RAW files themselves are
  never written to.
- **Part 3, reading back:** stars and labels from existing sidecars, such as
  Lightroom's, read in when a folder is added, so switching tools keeps past culling.
- **Keywords, titles and copyright** can join the same packet once the app has them.
