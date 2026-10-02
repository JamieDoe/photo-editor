# ADR 0061: Deterministic X-Trans (Fujifilm) decoding

- Status: Accepted
- Date: 2026-10-01

## Context

Decoding the same Fujifilm X-Trans RAF twice gave different pixels. On the X-T3
fixture at preview scale (`DecodeScale::AtLeast(1500)`), 3.4–4% of pixels differed
between two identical decodes, by up to 63 levels in 8-bit sRGB terms. The five Bayer
fixtures (Canon CR3, Nikon NEF, Ricoh DNG, two Sony ARW) were identical every time.

The cause is two data races in LibRaw 0.22.2's OpenMP code. Both touch only X-Trans,
and neither occurs with one OpenMP thread. Unpacking (the parallel Fuji compressed
decoder) was deterministic; `dcraw_process` was not.

1. **Half-size binning (`copy_bayer`).** Preview decodes use `half_size`. Each 2×2
   sensor cell becomes one output pixel, and the raw rows are split across threads
   (`schedule(dynamic)`). A Bayer cell has one sample of each colour, so no output
   sample is written twice. An X-Trans cell can hold two or four greens, or two reds
   or blues, so rows `2k` and `2k+1` write the same output sample from different
   threads. Whichever thread writes last wins.
2. **Full-size demosaic (`xtrans_interpolate`).** The demosaic runs 512-row strips in
   parallel, overlapping by 16 rows. Each strip starts by copying its 8 context rows
   from the shared image, while the strip above may still be overwriting those rows
   with its final output. Up to 0.06% of full-size pixels differed run to run, by up
   to 1656/65535. Against single-threaded LibRaw, a 10-thread decode differed in 0.6%
   of pixels, all within 20 px of a strip seam.

Running all of `dcraw_process` single-threaded fixes both, but costs +75 ms on a
preview decode (+40%) and makes a full decode 7× slower (2.3 s → 15–18 s). Patching
LibRaw is not an option while we link the system library (ADR 0003).

## Decision

Decodes use a `LibRaw` subclass (`crates/raw/shim/pe_libraw_xtrans.cpp`). It is
created by `pe_libraw_new()` in place of `libraw_init(0)` and used through the same
C API. It hooks the two extension points LibRaw provides for this:

1. **`copy_bayer` (a virtual "hotspot")**: for X-Trans with shrink, the copy runs in
   parallel, but each task owns whole output rows and writes raw row `2k` before
   `2k+1`. The result is **byte-identical to LibRaw's single-threaded output**. Every
   other sensor uses LibRaw's own `copy_bayer`.
2. **`interpolate_xtrans_cb`** replaces LibRaw's strip loop:
   - The image is cut into a fixed grid of blocks: 492 px of output each, plus 12 px
     of context on inner edges. That makes each block one LibRaw tile. The last row
     and column of blocks take the remainder.
   - Each block is copied into a private buffer. LibRaw's own `xtrans_interpolate`
     demosaics it there, single-threaded, in a separate `LibRaw` instance. Then only
     the block's output region is copied back.
   - Blocks run on our own threads (up to the OpenMP thread cap) in four
     checkerboard phases. A block reads at most into its neighbours' output regions,
     and never into a region being written in the same phase. Every block's input is
     therefore fixed before its phase starts.
   - The grid does not depend on the thread count, so **the output is identical for
     any number of threads**. It is not identical to LibRaw's single-threaded output:
     single-threaded LibRaw reads already-demosaicked rows at its own tile seams,
     while a block reads whatever its phase leaves there. Differences are confined to
     seams (1.4% of pixels, median 43/65535, max 2238). A racy 10-thread decode
     differed from single-threaded LibRaw by the same maximum (2238): these seam
     pixels are sensitive to their context, whichever way it is chosen.
   - Cancellation is checked between phases.

OpenMP is still resolved with `dlsym`, as for the thread cap (ADR 0003). Without
OpenMP, LibRaw is single-threaded and already deterministic, so its own code paths
run unchanged. Windows has no OpenMP lookup yet. A Windows LibRaw built with OpenMP
would still race there, which is tracked with the thread-cap TODO.

Subclassing `LibRaw` relies on the installed headers matching the linked library,
which is the same assumption the C shim already makes about `libraw_data_t`. Bayer
files take LibRaw's own paths: their decodes are byte-identical to before.

## Measurements

Fujifilm X-T3 (26 MP), M1 Max, release build, LibRaw 0.22.2. Medians of 3–9 decodes,
with `main` and this change run alternately (machine load 3–13):
(measured before ADR 0060 added the Rec.2020 → sRGB step after LibRaw, which is
per-pixel and unaffected by this change)

| Decode | Threads | Before | After | Pixels differing between two decodes, before → after |
|---|---|---|---|---|
| Preview (3123×2085) | 10 | 193 ms | 189 ms | 3.6–4.1% → 0 |
| Preview | 5 (export lane) | 313 ms | 304 ms | |
| Full (6246×4170) | 10 | 2304 ms | 2351 ms (+1%, within noise) | up to 0.06% → 0 |
| Full | 5 (export lane) | 4.15 s | 3.66 s | |

- **Preview is no slower.** Inlining the pattern lookup more than pays for the
  ordering.
- **Full decode costs about the same.** A first version that gave every edge block a
  full 512 px tile used 117 full tiles where LibRaw's partial edge tiles add up to
  about 105, which cost +9%. Folding the remainder into the last block fixed that. At
  the export lane's 5 threads the decode is 12% faster, because about 29 blocks per
  phase balance better than LibRaw's 9 strips.
- **Peak memory** (`bench --decode-peak`, full decode):
  - Unchanged at 2 and 5 threads (424 vs 423 MB, with libmalloc's large-allocation
    cache disabled).
  - At 10 threads it rises from 640 to 734 MB (24.5 → 28.1 B/px), or 483 → 554 MB
    with the cache disabled. LibRaw's 9 strips leave one of its 10 per-thread tile
    buffers (24.6 MB) untouched, but every block worker is busy. Each worker also
    holds its block buffer (2–5 MB) and a `LibRaw` instance (~0.75 MB).
  - The overhead is bounded per worker, not per pixel.
- **Every Bayer fixture is byte-identical to before**, at both scales.

## Tests

- `synthetic_xtrans_decodes_are_deterministic` (runs in CI): a synthetic X-Trans DNG
  (`fixtures::chart_xtrans_dng`) decoded at half and full size, several times and
  with 1, 3 and all threads, must be byte-identical.
  - The chart carries deterministic per-photosite texture. On a flat chart, duplicate
    samples in a cell are equal, and the race would not show.
  - The DNG is 1700×988, so the block grid covers both a merged remainder and a
    pulled-back last block.
  - On the old code the test fails at both scales.
- `synthetic_xtrans_full_decode_matches_the_chart`: every 24×24 region of the full
  decode is compared with the scene (worst mean error 0.015, limit 0.03). Writing
  blocks back 6 px off gives 0.128.
- `fujifilm_xtrans_decodes_are_deterministic`: the X-T3 fixture is decoded twice at
  preview and full scale. It skips when the git-ignored file is absent.

## Consequences

- Repeated decodes of a Fuji file are reproducible, so its previews, exports and
  golden images no longer change from run to run.
- Full-size X-Trans output differs slightly from LibRaw's single-threaded output at
  block seams, by design (see above). It no longer depends on the machine's core
  count.
- The shim now includes a small C++ file and links the C++ standard library.
- **Revisit when we build our own LibRaw** (ADR 0003): the fix belongs upstream.
  Ordering `copy_bayer` and giving `xtrans_interpolate` read-only context would
  remove the need for the subclass. Report both races to LibRaw.
