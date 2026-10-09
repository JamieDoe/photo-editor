# ADR 0076: PNG and TIFF input

- Status: Accepted
- Date: 2026-10-09

## Context

PRODUCT §3.2 lists JPEG, PNG, TIFF, DNG and the major raw formats as V1 input. The
app read JPEG and, through LibRaw, raw files only. TIFFs come from scanners, from
other editors (often 16-bit), and from round trips through Photoshop. PNGs come from
screenshots and exports.

## Decision

1. **Two decoders behind the existing abstraction** (`raw::PngDecoder`,
   `raw::TiffDecoder`), registered after JPEG and before LibRaw. DNG stays LibRaw's.
   - **PNG:** the `png` crate (MIT/Apache-2.0).
   - **TIFF:** the `tiff` crate (MIT).
   - **Dependencies:** both were already dependencies for export (ADR 0046). TIFF
     reading adds its LZW decoder, `weezl` (MIT/Apache-2.0, from the same image-rs
     project), and JPEG-in-TIFF through the zune-jpeg we already ship. Fax
     compression is left out: it is for black-and-white documents.
2. **What's read:**
   - **Bit depths:** 8- and 16-bit grey, grey with alpha, RGB and RGBA.
   - **PNG:** palettes and low bit depths are expanded to 8 bits.
   - **TIFF:** uncompressed, LZW, Deflate or JPEG compressed. The first image of the
     file only.
   - **Precision:** 16-bit files keep it. Their values are linearised through a
     65,536-entry table, not cut to 8 bits.
   - **Grey and alpha:** grey becomes RGB. Alpha is dropped, so transparent areas show
     the colour the file stores.
   - **Not supported:** CMYK, palette and floating-point TIFFs. They are refused with
     a clear error.
3. **Colour:** values are taken as sRGB, as for JPEG, unless the file embeds another
   profile. ADR 0077 converts from Adobe RGB, Display P3, ProPhoto and other
   matrix/TRC profiles, for JPEG too.
4. **Metadata** (camera, lens, ISO, aperture, shutter, focal length, capture time,
   orientation, GPS):
   - **TIFF:** read by seeking through its IFDs, a few kilobytes. The EXIF reader we
     use reads a whole TIFF into memory, which is too costly for a large file while
     indexing. The IFD reader written for lens profiles (ADR 0075) is now shared
     (`tiff_ifd`).
   - **PNG:** its size comes from the header and EXIF from its `eXIf` chunk.
   - **Camera settings:** carried into the decode, so the sky finder (ADR 0074) can
     judge the scene's brightness.
5. **Rendered, not raw:** `raw::is_rendered_extension` (JPEG, PNG, TIFF) replaces the
   "everything but JPEG" rule.
   - **The library:** doesn't badge these files as RAW.
   - **Sidecars:** none are written beside them, and a `NAME.xmp` beside one is left
     to a raw of the same name, as for JPEG.
6. **Thumbnails** decode the file and reduce it: these formats can't be decoded at a
   smaller scale.

## Consequences

- PNG and TIFF files appear in folders and the library, open, edit, export and get
  thumbnails like JPEGs. The open dialog offers them, as it lists the registry's
  extensions.
- **Cost** (24 MP, release build, PERFORMANCE §56):
  - **Full decode:** about 1 s for a 16-bit LZW TIFF, 0.5 s for an 8-bit one and 0.3 s
    for a PNG. LZW decompression dominates, on one thread.
  - **Thumbnails:** cost the same. That is acceptable in the background.
  - **Possible speed-ups:** using a TIFF's own embedded thumbnail, and decoding strips
    in parallel.
- **Still to do:** EXIF orientation, which JPEG ignores too. ICC profiles: done in ADR
  0077.
