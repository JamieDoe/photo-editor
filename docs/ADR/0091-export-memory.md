# ADR 0091: Export memory

- Status: Accepted
- Date: 2026-10-14

## Context

PERFORMANCE §65 found full-size exports peaking at 0.9 GB for a 24 MP RAW and 2.5 GB
for 61 MP, against §8's 400 MB target and the 8 GB reference Mac (ADR 0090).

Measuring the export's physical footprint stage by stage (what macOS counts: Activity
Monitor's Memory column and memory pressure; `bench --export-memory`) showed four
causes:

1. **The allocator kept freed memory.** macOS's allocator keeps the pages of freed
   large blocks for reuse, and they count against the machine:
   - it held about 600 MB after a 24 MP export and 1.8 GB after a 61 MP one, until the
     app quit;
   - it made each export peak above its live data.

   `malloc_zone_pressure_relief` returned nothing on macOS 26, whose allocator ignores
   it. Only `MallocLargeCache=0` stops the keeping.
2. **Two steps copied the image while changing nothing:** output sharpening set to
   None, and conversion to sRGB at the same depth.
3. **A TIFF was built whole in memory before being written,** after a full-size copy
   of its 16-bit samples.
4. **An export's decode stayed alive while its framed (lens-corrected, cropped) copy
   was made, and that copy stayed in the renderer's cache afterwards:** 370 MB for a
   61 MP RAW.

## Decision

1. **`MallocLargeCache=0` for the app**, set in its bundle's `Info.plist` as
   `LSEnvironment` (`apps/desktop/src-tauri/Info.plist`, merged by Tauri), so it
   applies whenever macOS launches the app.
   - **The cost:** each new large buffer comes fresh from the system.
   - **On 4 threads:** interactive renders are about 10% slower (a heavy drag's round
     trip 20 → 22 ms, every frame still shown) and exports about 4% slower.
   - **Worth it:** that is less than the freed memory costs a machine with 8 GB.
   - **Development runs** (`tauri dev`, the bare binary) don't get it.
2. **`sharpen` and `colour::convert` take the image by value** and return it unchanged
   when there is nothing to do.
3. **A TIFF is encoded straight into the file** it is written to (`encode_to_file`, with
   `platform::fs::write_atomic_with` keeping the write atomic). Its 16-bit samples are
   read in place. The bytes are identical to before.
4. **Exports render with `CpuRenderer::render_once`.** It owns the decode, frees each
   full-size step (decode, retouched, framed) as soon as the next exists, and caches
   nothing. Its output equals a cached render's.

## Consequences

- **Every export now peaks at LibRaw's decode,** whatever the format, edit or lens
  correction (PERFORMANCE §66):
  - 396 MB for 24 MP, within §8's 400 MB (it was 637–901 MB);
  - 981 MB for 61 MP (it was 1.8–2.5 GB).
- **Memory returns to a few MB** after each export.
- **LibRaw's decode is the floor,** about 16 bytes a pixel: its raw buffer, its
  four-channel image and the converted copy it hands over. Lowering it would mean
  reading LibRaw's image directly, without that copy; that is a change for the decoder
  shim.
- **The benchmark reports physical footprint on macOS.** Resident size there includes
  the allocator's kept pages.
