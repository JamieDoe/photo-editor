# ADR 0082: TIFFs decoded strip by strip, in parallel

- Status: Accepted
- Date: 2026-10-13

## Context

ADR 0076 read a TIFF with the `tiff` crate's whole-image decode: one thread
decompresses every strip into a full-size buffer of samples, which is then made linear
and, for a thumbnail or a preview, halved down to size.

Large 16-bit TIFFs (exports from other raw converters, scans) are common in a
photographer's folders. A 24 MP one, Deflate-compressed with the horizontal predictor
as Lightroom and Capture One write them, took 0.65 s to open and 0.7 s to make a
thumbnail; a 61 MP one 1.7 s and 1.8 s. Almost all of it is decompression on one
thread (550 ms of the 660 ms for 24 MP).

ADR 0076 named two speed-ups: a TIFF's own embedded thumbnail, and decoding strips
in parallel.

## Options

1. **The TIFF's embedded thumbnail.** None of the large TIFFs at hand has one: each is
   a single image. Photoshop keeps a 160-pixel JPEG in its private resources, too
   small for a 512-pixel thumbnail. The `tiff` crate can't decode a SubIFD image
   either. Not done.
2. **A faster inflater.** `zlib-rs` (Zlib licence) behind `flate2` saved about 5 %;
   `fdeflate`, about 20 % of decompression, but `tiff` only uses `flate2`, so it would
   mean decompressing and undoing predictors ourselves. These files barely compress
   (147 MB of samples in 119 MB), so Huffman decoding sets the cost. Not done.
3. **Strips (or tiles) decoded in parallel, straight to linear light, reduced as they
   are read.** **Chosen.**

## Decision

1. **By chunks** (`raw::tiff_chunks`): the output is cut into bands of rows, and each
   worker opens the file and decodes the strips or tiles under its bands
   (`read_chunk`). Bands are whole chunks high and whole reduction boxes high, so no
   chunk is decoded twice.
2. **Straight to linear:** each pixel goes through its curve as it is read (sRGB's,
   or the embedded profile's and then into sRGB's primaries: ADR 0077), as the
   whole-image decode did. Converting before averaging keeps a preview and an export
   alike where colours beyond sRGB are brought in.
3. **Reduced as it is read:** a decode at a smaller scale (previews, thumbnails of
   edited photos) or a thumbnail adds each pixel into its `f` × `f` box, `f` the power
   of two that halving would reach. The size and the result are those of halving `f`
   times, rounded once instead of at every step (within 4 levels in 65,535). Neither
   the file's full-size samples nor a full-size linear copy is held.
4. **Thumbnails are averaged in linear light,** like the pyramid, then encoded as
   8-bit sRGB. They were averaged as encoded values; fine detail now keeps its
   brightness.
5. **Plane by plane:** TIFFs that store a strip of red, then of green, then of blue
   are read a plane at a time. The whole-image decode returned only their first plane
   and failed; they now open. Planes stored in tiles are refused with a clear message:
   the `tiff` crate sizes their edge tiles wrongly, and they are rare.
6. **Threads:** the work uses the calling lane's pool. Opening in the editor has every
   core. Thumbnails stay on the browse lane's few threads (ADR 0015's budget), so a
   folder's throughput hardly changes; each thumbnail arrives sooner.

## Consequences

- **Cost** (release build, PERFORMANCE §58):
  - **Opening in the editor:** 0.65 → 0.15 s for 24 MP and 1.7 → 0.34 s for 61 MP.
  - **One thumbnail:** 0.7 → 0.25 s for 24 MP and 1.8 → 0.65 s for 61 MP.
  - **A screenful of 48 TIFFs:** 2.4 → 2.7 a second, bounded by decompression on the
    browse lane's three threads. A faster inflater (option 2) is what would help next.
- **Memory:** a full decode no longer holds the file's samples beside the linear
  image, only a band of sums per thread: its peak falls from 304 to 240 MB for 24 MP
  and from 742 to 568 MB for 61 MP.
- **Responsiveness:** an export's decode now takes the background lane's threads for
  a quarter of a second instead of one thread for a second, as its render already
  did. Interactive previews during an export were unchanged within the machine's noise
  (p50 77 → 83 ms, max 102 → 108 ms, 24 MP).
- **Code:** one path for every TIFF; the whole-image read is gone (tests keep it as
  the reference).
