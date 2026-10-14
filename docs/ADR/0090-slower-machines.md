# ADR 0090: Keeping up on slower machines

- Status: Accepted
- Date: 2026-10-14

## Context

The app has been measured on an M1 Max (8 performance cores, 2 efficiency cores).
PERFORMANCE §8's targets were "to confirm on low-end hardware", and since Phase 0 the
editor has gained heavy controls: clarity, texture, noise reduction, dehaze, grain, the
colour mixer. The renderer is CPU only (no GPU path is used), so a slower Mac differs
mainly in how many cores do the work.

## Decision

1. **The reference machine** is the slowest Mac the app supports (ADR 0086): an M1
   MacBook Air, 8 GB, macOS 13.
   - **Stood in for here** by limiting the app to 4 threads (`PE_SELF_TEST_CPUS=4` for
     the self-test, `--cpus 4` for the benchmark): every pool uses at most that many.
     That is the M1's four performance cores with no help from its efficiency cores,
     so the figures err on the slow side.
   - **Thermal throttling** (the Air has no fan) and the M1's narrower memory bandwidth
     aren't modelled; long exports there will be somewhat slower than measured.
   - **The app sizes its job lanes from the core count** (`JobSystemConfig::for_cores`),
     so a smaller machine gets proportionally smaller lanes.
2. **Drags keep up by rendering smaller** while they can't.
   - **The problem:** a heavy edit took 45–58 ms per interactive frame with 4 threads,
     so a drag showed half its frames or fewer.
   - **What triggers it:** the preview scheduler marks a request as *catching up* when
     a newer change was already waiting when the last render returned. A single
     change, an undo or a click never is.
   - **What it does:** for those requests the engine renders from the next pyramid
     level down (a quarter of the pixels) when the running cost estimate (ms per
     megapixel, updated by every viewer render) predicts the normal level would take
     over 14 ms.
   - **Without flicker:** it only goes back up once the larger level would take under
     80% of that.
   - **Limits:** at most one level down, so a drag is never less than half as sharp.
     The detail render that follows every drag is full sharpness, as before.
3. **Edge-aware maps are built in parallel.**
   - **The maps:** the tone, dehaze and colour-noise maps (guided filters on small
     fixed-size maps), rebuilt when a control they depend on moves.
   - **The cost:** they were built on one thread, about half of a small frame's time
     with 4 threads.
   - **The change:** their passes now run line by line and side by side. The sums are
     taken in the same order, so results are bit-identical and the golden images are
     unchanged.

## Consequences

- **With 4 threads,** a heavy drag shows every frame instead of about half
  (PERFORMANCE §65).
- **With all cores,** the same drag is about a quarter faster.
- **Light edits** still drag at full preview sharpness everywhere.
- **Memory is the remaining gap for 8 GB Macs** (PERFORMANCE §65):
  - full-size exports peak at about 0.9 GB for a 24 MP RAW and 2.5 GB for 61 MP;
  - exports sized for the web decode at half size and need far less;
  - lowering the full-size peak needs the lens correction made in bands rather than as
    a full-size copy, and the detail stage's full-size plane removed. That is its own
    change.
- **The per-pixel pass** (already parallel) is now about two thirds of a frame. Making
  it cheaper (SIMD, or the GPU) is the next step if drags need to be faster still.
