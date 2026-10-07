# ADR 0063: Export metadata ("Keep metadata", "Strip location")

- Status: Accepted
- Date: 2026-10-07

## Context

The design's export dialog has three switch tiles under the rows: **Keep metadata**
(on), **Strip location** (off) and **Watermark** (off). Until now exports carried no
metadata at all. A shared photo lost its camera, lens, exposure and capture date, which
photographers expect to keep, and there was no way to choose.

The location matters most. A photo posted online with GPS coordinates can reveal
someone's home. The choice has to be the photographer's, and stripping must be
complete.

## Decision

1. **Two switches: Keep metadata and Strip location.**
   - Remembered as `export.keepMetadata` (default on) and `export.stripLocation`
     (default off), as the design has them.
   - Strip location is disabled while Keep metadata is off. Nothing is kept then, so
     there is no location to strip.
   - **The presets leave them alone:** the design's presets don't set them, and
     changing them doesn't deselect a preset.
   - The export batch carries both. The single-photo export reads them from settings.
   - **Watermark** stays hidden until it is built.
2. **What is copied: capture facts only.**
   - **Main directory:** camera make and model, and Orientation 1 (the pixels are
     written upright).
   - **Exif directory:** exposure time, f-number, ISO, Exif version 2.32, capture time
     (DateTimeOriginal), focal length, lens model, the colour space (1 for sRGB,
     "uncalibrated" for P3 and Adobe RGB, which the embedded profile describes) and the
     exported pixel size.
   - **GPS directory:** latitude and longitude, unless stripped.
   - **Never copied:** the source's maker notes and embedded thumbnails. They can hold
     serial numbers and other details nobody chose to share, and they describe the
     original rather than the export. Stripping location therefore can't miss a
     coordinate hidden elsewhere: only the fields listed above are written.
   - **Source:** the facts come from the file's header, through the decoder that opens
     it (`raw::PhotoMetadata`). That is the same data the Library's info panel shows.
     A file whose header can't be read still exports, without the facts.
3. **One set of entries for every format** (`export::metadata`):
   - **JPEG:** an APP1 Exif segment after SOI and JFIF, before the ICC profile.
   - **PNG:** an `eXIf` chunk.
   - **TIFF:** its own Exif and GPS directories, pointed to from the image directory.
     Two small local value types fill gaps in the `tiff` crate: several RATIONALs in
     one field, and UNDEFINED. The pointers are LONG, as the Exif standard defines
     them; the crate's IFD type is rejected by readers. Values and directories are
     padded to word boundaries, which the crate doesn't do for directories outside the
     image sequence.
   - **No new dependency:** the EXIF block is a ~100-line little-endian TIFF writer.
     `kamadak-exif` (BSD-2-Clause, already used to read JPEG metadata) is a
     dev-dependency of the export crate, used to read exports back in tests.
4. **Tests:**
   - **Export crate:**
     - every fact reads back as written (make, model, lens, date, 1/250 s, f/5.6, ISO,
       focal length, orientation, colour space, size, latitude and longitude with
       their hemispheres);
     - stripping the location leaves everything else, and a wide space is marked
       uncalibrated;
     - missing facts are left out;
     - exposure times read as photographers write them (1/250, 0.3 s, 2.5 s), and
       dates and degrees convert;
     - JPEG puts EXIF after JFIF;
     - **every format, every choice:** JPEG, PNG and TIFF each carry everything, no
       location, or nothing; the profile is still embedded and each file still
       decodes.
   - **Settings:** the defaults (kept, location kept), and stored choices load.
   - **macOS ImageIO** reads the same files: a JPEG, PNG and TIFF with a location show
     the make, capture date, GPS (51.5072 N, 0.1276 W) and the Adobe RGB profile. A
     self-test export of the Nikon Z 6 shows Nikon, Z 6, the NIKKOR Z 24-70mm f/4 S,
     1 s at f/6.7, ISO 100, 52 mm and its capture time. With Keep metadata off there
     is no EXIF at all.
   - **Release self-test:** the same 1,350 px JPEG with everything (171,689 bytes),
     without the location (171,689; the test photo has none) and with nothing (171,417).
5. **Performance:** reading the source's header takes about 0.25 ms per photo (Nikon
   Z 6, Canon EOS R6 and Ricoh GR III files, release build). Writing the block is
   negligible next to an export's 1–2 s.

## Deviations from the design

Recorded in ADR 0016:

- Watermark and the size estimate are still hidden.
- With two tiles the row is two columns, not the design's three.

## Consequences

- **Rating, keywords and title** would go in XMP, which Lightroom and most tools read
  for these. The app has ratings but no keywords or titles yet. XMP can follow as its
  own step, using the same three containers (JPEG APP1, PNG `iTXt`, TIFF tag 700).
- **Time zone:** capture times are written as the camera recorded them. The offset
  (`OffsetTimeOriginal`) isn't read from sources yet.
- **Copyright and artist** need a place to enter them (Settings) before they can be
  written.
