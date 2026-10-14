# ADR 0092: A quick X-Trans demosaic for zoom

- Status: Accepted
- Date: 2026-10-14

## Context

On the 4-thread stand-in for the slowest supported Mac (ADR 0090), a Fujifilm X-Trans
RAW took about 7 s to decode at full size; other cameras' RAWs took 1–2 s. Exports
and zooming to 100% both decode at full size, in the background lane, which a
4-thread machine gives 2 threads.

Profiling a full decode: about 90% of the active time is LibRaw's X-Trans demosaic,
Markesteijn with three passes, which the shim runs block by block (ADR 0061). The blocks
keep the threads busy (24 a phase for a 26 MP sensor). The cost is the three passes.

One pass, measured on the X-T3 at full size:

| Threads | Three passes | One pass |
|---|---|---|
| 4 | 3,684 ms | 1,992 ms |
| 2 | 6,828 ms | 3,623 ms |

**The difference**, in 8-bit sRGB at a viewer's exposure:
- mean 0.22 levels;
- 0.94% of pixels differ by more than 2 levels, and 0.009% by more than 8;
- the largest difference is 32.

The crop where the two differ most (printed halftone texture, where demosaic artefacts
show first) looks the same side by side.

## Decision

1. **`DecodeOptions::quick_demosaic`** asks for a one-pass X-Trans demosaic.
   - **How it reaches LibRaw:** the shim's `pe_raw_decode` takes it as a flag
     (`PE_RAW_QUICK_DEMOSAIC`), and `pe_libraw_quick_demosaic` sets it on the
     instance.
   - **Determinism kept:** the block grid is unchanged, so output is still identical
     for any thread count.
   - **Other sensors** are unaffected.
2. **The editor's full-resolution decode uses it.** That decode backs zooming to 100%
   (ADR 0070) and removals' fills. The view there is a moment's look at detail, and
   gets there twice as fast.
3. **Full-size exports keep three passes:** they are where image quality is the point.
   Exports at a smaller size decode at half size, which has no demosaic at all.

## Consequences

- **Zooming to 100% on a Fujifilm X-T3, 4-thread stand-in:** 7.0 → 3.9 s.
- **Full-size Fujifilm exports are unchanged:** about 7 s on the stand-in, 3.7 s with
  4 threads to themselves.
- **Zoom and export can differ slightly:** at 100% the editor shows the one-pass
  demosaic, and a full-size export has the three-pass one. The difference is within
  the figures above.
- **A removal's fill** is made from the one-pass decode, then applied to the export's.
