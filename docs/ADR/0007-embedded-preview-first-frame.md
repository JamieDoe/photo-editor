# ADR 0007: Show the embedded camera preview while the RAW decodes

- Status: Accepted
- Date: 2026-09-28

## Context

Opening a RAW blocked the first visible image on the half-size decode: 230–370 ms
(up to 560 ms for DNG). PERFORMANCE.md proposed ≤ 150 ms to the first visible image.
Almost every RAW file embeds a camera-rendered JPEG preview.

## Decision

1. **Decoder capability.** `Decoder::embedded_preview(path, min_long_edge, cancel)`,
   default `Ok(None)`. `LibRawDecoder`:
   - reads LibRaw's thumbnail list and extracts the smallest JPEG preview whose long
     edge is ≥ `min_long_edge` (1024 px), or the largest if none is;
   - decodes it with libjpeg-turbo using **DCT-domain scaling**: the smallest
     power-of-two scale (1, 1/2, 1/4, 1/8) whose long edge is still ≥ the target, so a
     full-size embedded JPEG is never decoded at full size. Power-of-two only, so the
     output matches what repeated 2× downsampling kept before (a 1620 px preview stays
     1620 px, not 3/4 = 1215 px);
   - applies the main image's orientation (dcraw `flip` semantics), without unpacking
     the raw data;
   - falls back to a full-size zune-jpeg decode plus box downscale when built without
     the `turbojpeg` feature.
2. **Part of the open job.** `Engine::open_with_preview(path, on_preview)` extracts the
   preview first, calls `on_preview` from the job thread, then decodes as before. A
   missing or broken embedded preview is logged and never fails the open.
   `ImageSummary::embedded_preview_ms` records the extraction time.
3. **Streamed over IPC before the command returns.** The open commands take a
   `tauri::ipc::Channel<Response>` and send the preview in the standard binary frame
   format with flag bit 1 (`FRAME_FLAG_EMBEDDED`).
4. **Display-only placeholder.** It is never cached as a render, never edited, and is
   labelled in the stats panel. The UI enforces the ordering:
   - Previews from an older open, or arriving after the open was adopted, are ignored.
     (The channel message and the command response travel different paths.)
   - While an open is pending, late frames of the previous image are dropped so they
     cannot replace the new preview.
   - If the open fails after the preview was shown, the previous image is re-rendered.

## Measurements

### Initial version (full-size zune-jpeg decode)

End-to-end in the app (`PE_SELF_TEST`, cold file, request → shown on screen):

| File | Embedded preview shown | First render shown |
|---|---|---|
| Canon EOS R6 CR3 | 42 ms | 166 ms |
| Nikon Z 6 NEF | 44 ms | 407 ms |
| Sony A7 III ARW | 49 ms | 205 ms |
| Sony A7R IV ARW (61 MP) | 41 ms | 387 ms |
| Fujifilm X-T3 RAF | 107 ms | 308 ms |
| Ricoh GR III DNG | 143 ms | 563 ms |

Warm extraction (`bench`): 10–16 ms where the camera embeds a ~1620 px preview. It is
68–98 ms for Fuji and Ricoh, which embed only near-full-size JPEGs that are decoded in
full and then downscaled.

### With DCT-scaled decoding (libjpeg-turbo, shared `jpeg-turbo` crate)

In the app, two self-test runs each (cold = first open after launch, re-open = same file
once the app has settled):

| File | Embedded JPEG | Shown, first open | Shown, re-open | Before (first open) |
|---|---|---|---|---|
| Fujifilm X-T3 RAF | 4416x2944 → 1104x736 | 50–61 ms | 34–35 ms | 101–107 ms |
| Ricoh GR III DNG | 6000x4000 → 1500x1000 | 60–74 ms | 46–48 ms | 124–143 ms |
| Nikon Z 6 NEF | 1620x1080 | 39 ms | 16 ms | 43–45 ms |
| Canon EOS R6 CR3 | 1620x1080 | 35–37 ms | 14–15 ms | 22–42 ms |
| Sony A7 III ARW | 1616x1080 | 25–47 ms | 19 ms | 49 ms |
| Sony A7R IV ARW | 1616x1080 | 29–40 ms | 14 ms | 41 ms |

Warm extraction in isolation: Fuji 68 → 31 ms, Ricoh 98 → 40 ms; small previews
9–13 ms. Peak memory after open for Fuji/Ricoh drops from ~198–210 MB to ~166–173 MB,
because the full-size image is never allocated. That is now within ~10–14 MB of a
file without an embedded preview.

**Why not ~15 ms:** DCT scaling removes most of the inverse DCT and colour
conversion, but not entropy (Huffman) decoding, which must read every coefficient.
On a real 24 MP q92 4:4:4 JPEG, decode takes 103.5 ms at 1/1 and still 72.5 ms at 1/8.
For full-size embedded previews, ~30–40 ms is therefore the floor of this approach.
Going lower would need restart-marker-parallel decoding, and only some cameras write
restart markers.

**Build-wiring lesson:** the first app build after this change still used zune-jpeg.
`app-core` depends on `raw` without default features and didn't forward the new
`turbojpeg` feature, and the benchmark (default features) hid it. Engine info now
reports the active JPEG encoder and embedded-preview decoder, shown in the stats panel
and in every self-test report, and `app-core`'s `turbojpeg` feature controls both
`raw` and `export`.
## Consequences

- Every sample now shows an image within the proposed ≤ 150 ms target.
- The placeholder shows the camera's rendering, so its look differs from ours and
  visibly swaps on the first render. This is acceptable for a placeholder and is how
  other raw editors behave.
- The open job's first render is delayed by the extraction time (10–40 ms warm).
- Peak memory after open rises by ~10–14 MB for every camera sample.
- libjpeg-turbo is now used for decoding as well as encoding. Both go through one
  crate (`crates/jpeg-turbo`) that finds and links the library once.
