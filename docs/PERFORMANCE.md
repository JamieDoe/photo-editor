# Performance

Status: Phase 0 baseline. These numbers set a starting point and expose bottlenecks.
They are **not** product targets and make no comparison with other software.

## 1. How to measure

```bash
cargo run -p fixtures --release --bin gen-fixtures -- --large   # synthetic 24 MP DNG/JPEG
cargo run -p bench --release -- --iterations 7                   # every file in tests/fixtures/{synthetic,local}
cargo run -p gpu-spike --release --bin gpu-bench -- <file>       # CPU vs GPU (ADR 0005)
PE_SELF_TEST=<file> target/release/desktop                       # end-to-end through the real UI + IPC
```

- `bench` runs each file in a child process. It writes a markdown table to stdout and
  JSON to `bench-results/phase0-<timestamp>.json`. Each measurement is the median of
  N timed runs after one warm-up. Recipes vary slightly between iterations so the
  preview cache never serves a timed render.
- App-like peak memory comes from a **separate** child that does exactly what the app
  does (open → interactive + detail preview → export) and reads `ru_maxrss` after each
  step.
- The self-test drives the real React UI, using the same actions as the sliders. It
  makes one slider change per animation frame, runs an export while dragging again,
  and reports UI-side timings (request → frame displayed, animation-frame gaps).

## 2. Environment and caveats

- Apple M1 Max, 10 cores, 32 GB, macOS, Metal. LibRaw 0.22.2 (Homebrew, OpenMP).
  Release build.
- **This is a high-end machine.** Low-end hardware (4-core laptops, integrated GPUs)
  has not been measured yet; see §7.
- The machine was a shared developer workstation with ambient load (load average 6–8
  from other apps and simulators). CPU-bound full-resolution work varied up to ~2×
  between runs. The tables therefore show the **median of three runs** with the
  (min–max) range where it spread by more than 5%.
- Test files: six CC0 camera samples from raw.pixls.us, plus synthetic files
  (`tests/fixtures/README.md`).

## 3. Results: open and preview (ms)

"Preview decode" is the reduced-resolution decode used on open (LibRaw `half_size`).
Render times are Rust-side through the engine (job dispatch included); sizes are the
pyramid level chosen for each quality.

| File | MP | Preview decode | Pyramid | Thumbnail | Interactive | Detail |
|---|---|---|---|---|---|---|
| chart.dng (synthetic) | 1.0 | 38.8 (38.2–48.5) | 0.5 | 0.5 (300x200) | 1.6 (1200x800) | 1.6 (1200x800) |
| chart.jpg (synthetic) | 1.0 | 2.4 (2.4–2.9) | 0.5 | 0.5 (300x200) | 1.4 (1200x800) | 1.5 (1200x800) |
| Canon EOS R6 CR3 | 20.2 | 88.8 (87.6–103.8) | 1.8 | 0.6 (343x229) | 1.8 (1374x917) | 5.4 (5.2–6.8) (2748x1835) |
| Fujifilm X-T3 RAF | 26.0 | 191.9 (171.2–334.8) | 2.3 | 0.6 (390x260) | 2.6 (2.0–3.5) (1561x1042) | 9.2 (6.7–17.0) (3123x2085) |
| Nikon Z 6 NEF | 24.5 | 321.9 (318.1–379.5) | 2.3 | 0.6 (379x252) | 2.3 (1516x1010) | 7.1 (6.5–13.6) (3032x2020) |
| Ricoh GR III DNG | 24.2 | 399.4 (399.0–470.6) | 2.0 | 0.6 (376x251) | 2.0 (1505x1006) | 6.9 (6.3–15.5) (3010x2012) |
| Sony A7 III ARW | 24.2 | 145.9 (129.1–150.7) | 2.2 | 0.7 (376x251) | 2.1 (1.9–3.3) (1506x1006) | 7.4 (6.4–17.9) (3012x2012) |
| Sony A7R IV ARW | 61.0 | 335.0 (301.2–353.3) | 5.1 | 0.6 (299x199) | 1.6 (1196x797) | 16.8 (14.1–37.5) (4784x3188) |
| synthetic 24 MP DNG | 24.0 | 108.7 (107.6–127.5) | 2.1 | 0.6 (375x250) | 2.0 (2.0–3.1) (1500x1000) | 6.8 (6.7–19.0) (3000x2000) |
| synthetic 24 MP JPEG | 24.0 | 55.8 (54.7–65.1) | 2.0 | 0.6 (375x250) | 1.9 (1.8–3.8) (1500x1000) | 6.8 (6.1–15.3) (3000x2000) |

## 4. Results: full resolution and export (ms)

Export = full decode → render (RGB8) → JPEG q92 4:4:4 encode → atomic write.

| File | Full decode | Full render | Export total | Export: decode / render / encode / write |
|---|---|---|---|---|
| Canon EOS R6 CR3 | 388 (363–416) | 20.7 (18.6–31.6) | 651 (565–720) | 407 / 33.6 / 199 / 8.7 |
| Fujifilm X-T3 RAF | 2253 (2057–4380) | 26.4 (24.4–43.9) | 2644 (2349–4867) | 2324 / 43.4 / 268 / 9.7 |
| Nikon Z 6 NEF | 738 (627–936) | 25.7 (22.8–40.4) | 1011 (872–1310) | 729 / 43.4 / 229 / 8.5 |
| Ricoh GR III DNG | 840 (706–1099) | 26.1 (22.7–53.1) | 1164 (1000–1331) | 826 / 41.4 / 286 / 10.1 |
| Sony A7 III ARW | 503 (432–792) | 26.0 (24.5–50.4) | 854 (741–1137) | 494 / 40.9 / 310 / 11.2 |
| Sony A7R IV ARW (61 MP) | 1393 (1168–2084) | 61.6 (54.5–128.5) | 2083 (1819–2696) | 1358 / 99.2 / 601 / 11.3 |
| synthetic 24 MP DNG | 411 (377–646) | 25.6 (22.3–43.0) | 646 (566–871) | 409 / 38.9 / 190 / 7.4 |
| synthetic 24 MP JPEG | 63 (54–78) | 25.0 (21.9–35.0) | 297 (252–326) | 61 / 38.9 / 189 / 7.8 |

## 5. Results: responsiveness and memory

| File | Render cancel latency p50 / max (ms) | Interactive preview p50: idle → during export (ms) | App peak RSS (MB): open / +previews / +export |
|---|---|---|---|
| Canon EOS R6 CR3 | 0.7 / 2.4 | 1.9 → 2.9 | 127 / 141 / 478 |
| Fujifilm X-T3 RAF | 0.8 / 1.1 | 2.1 → **8.4** (now 3.7, §7.2) | 163 / 180 / 762 (now ~670) |
| Nikon Z 6 NEF | 0.6 / 0.8 | 2.1 → 2.2 | 152 / 169 / 644 |
| Ricoh GR III DNG | 0.5 / 1.5 | 2.0 → 2.4 | 152 / 169 / 645 |
| Sony A7 III ARW | 0.6 / 1.2 | 2.1 → 2.4 | 151 / 168 / 647 |
| Sony A7R IV ARW (61 MP) | 0.5 / 0.7 | 1.4 → 1.5 | 371 / 385 / **1321** |
| synthetic 24 MP JPEG | 0.6 / 0.9 | 2.0 → 2.2 | 268 / 310 / 311 |

Cancel latency is measured on a full-resolution render: time from setting the cancel
flag to the render returning.

### Per-stage cost at interactive resolution (cumulative, ms, median of 3 runs)

| File | Level | no stages (u16→f32 + sRGB encode) | + WB | + exposure | + contrast | + saturation |
|---|---|---|---|---|---|---|
| Nikon Z 6 NEF | 1516x1010 | 1.0 | 1.0 | 1.3 | 2.0 | 2.1 |
| Sony A7 III ARW | 1506x1006 | 1.0 | 1.1 | 1.0 | 1.8 | 1.9 |
| Sony A7R IV ARW | 1196x797 | 0.7 | 0.7 | 0.7 | 1.2 | 1.4 |
| synthetic 24 MP DNG | 1500x1000 | 0.9 | 0.9 | 0.9 | 1.7 | 1.9 |

Gains are essentially free once fused. The contrast LUT costs ~0.8 ms per 1.5 MP.
The base conversion and encode are about half the cost.

### End to end through the UI (self-test, `PE_SELF_TEST`)

A 120-step simulated drag at 60 Hz, then 90 more steps while an export runs.
Round trip = slider request → frame received in JS (IPC + queue + render + transfer).

The window must stay visible for the whole run. macOS stops delivering animation
frames to a window that is covered, on another Space or on a sleeping display, and the
editor paces its rendering on those frames. The self-test then fails within 2 s with
"animation frames stopped (page visibility: hidden)" rather than hanging (before
Phase 2 milestone 5 it hung).

| File | First frame after open | Drag: frames shown | Round trip p50 / p95 / max | During export: shown, p50 / p95 / max | UI frame gap p50 / max (idle baseline) | Detail frame: render → round trip |
|---|---|---|---|---|---|---|
| Nikon Z 6 | 367 ms (1516x1010) | 119/120 | 4 / 5 / 5 ms | 89/90, 4 / 10 / 18 ms | 17 / 18 ms (17 / 21) | 5.3 → 21 ms (3032x2020) |
| Sony A7R IV | 367 ms (1196x797) | 119/120 | 4 / 4 / 9 ms | 81/90, 3 / 10 / 31 ms | 17 / 18 ms (17 / 21) | 4.0 → 16 ms (2392x1594) |
| Fujifilm X-T3 | 230 ms (1561x1042) | 119/120 | 5 / 5 / 6 ms | **70/90**, 9 / 22 / 47 ms → after §7.2: **89/90**, 5 / 7 / 11 ms | 17 / 18 ms (17 / 21) | 4.5 → 18 ms (3123x2085) |

Animation-frame gaps during a drag match the idle baseline, so rendering adds no
measurable UI-thread cost. The webview throttles to 30 Hz when the window is not
frontmost; both baseline and drag then read 33 ms. Superseded renders (cancelled
before completion) were observed in real use: 19 of 220 requests on the Fuji, 8 on
the 61 MP Sony.

## 6. GPU vs CPU (ADR 0005)

Same plan and pixels (max difference 1 code). ms, median of 10 runs after warm-up.

| Source | Size | CPU | GPU incl. upload + readback | GPU resident (readback only) |
|---|---|---|---|---|
| Nikon Z 6 (quiet run) | 24.5 MP | 22.5 | 50.1 | 18.3 |
| Nikon Z 6 (quiet run) | 6.1 MP | 6.0 | 12.2 | 4.5 |
| Nikon Z 6 (quiet run) | 1.5 MP | 1.8 | 3.2 | 1.4 |
| Sony A7R IV (quiet run) | 61 MP | 55.5 | 127.6 | 45.2 |
| Nikon Z 6 (loaded run) | 24.5 MP | 72.9 | 55.8 | 18.0 |

## 7. Findings: architectural bottlenecks

1. **RAW decode dominates everything the user waits for.**
   - Open: 90–400 ms (half-size); export: 0.4–2.3 s (full). Rendering is 10–100×
     cheaper.
   - Lossless-compressed NEF and DNG unpack is single-threaded in LibRaw.
   - X-Trans full demosaic takes ~2.3 s.
   - Mitigations: embedded JPEG preview while decoding (**done**, §9 and ADR 0007);
     evaluate RawSpeed or
     rawler for unpack (ADR 0003); cache decoded preview pyramids on disk.
2. **LibRaw's OpenMP threads escaped the bounded background pool** (fixed). A Fuji
   export raised interactive latency ~4× (2.1 → 8.4 ms) and dropped frames while
   dragging (70/90 shown).
   - Fix: decoders take `DecodeOptions::max_threads`. The engine passes the calling
     lane's pool size, and the LibRaw shim caps OpenMP with `omp_set_num_threads`
     (looked up at runtime, so it is a no-op without OpenMP).
   - Result (3 bench runs + 2 self-tests): interactive p50 during a Fuji export is
     **3.7 ms** (max 4.8); **89/90** drag steps shown (p95 6–7 ms); export peak memory
     762 → ~670 MB (fewer per-thread demosaic buffers).
   - Trade-off: background X-Trans decode takes 3.6 s instead of 2.3 s. Opening on
     the interactive lane still uses all cores. This maps to the "background
     processing intensity" setting in PRODUCT.md.
3. **JPEG encoding was 20–30% of export** (160–515 ms, single-threaded). **Fixed**
   with libjpeg-turbo: 2.4–2.6× faster, byte-identical sizes (§9, ADR 0006).
4. **Export peak memory is ~26 bytes/pixel** (24 MP → ~640 MB; 61 MP → 1.3 GB), and
   most of it is **inside LibRaw**.
   - `bench --decode-peak` measures a lone full decode in a fresh process: 17–18 B/px
     for Bayer files (Nikon 451 MB, 61 MP Sony 1.05 GB) and 24.5 B/px for X-Trans.
     Our decoded result is 6 B/px.
   - LibRaw holds its raw buffer, a 4-channel working image and demosaic scratch at
     the same time. Removing our copy of its output would not lower the peak (the copy
     happens after LibRaw frees its buffers), so that refactor was not done.
   - Real fixes are architectural (Phase 3): own the demosaic so decoding can be tiled
     or streamed (e.g. RawSpeed-style unpack + our demosaic), and stream the render
     into a strip-based encoder.
5. **Large frames are IPC-bound, not render-bound.** A 6 MP detail frame renders in
   ~5 ms but takes ~16 ms to reach the canvas (24 MB RGBA through IPC plus
   `putImageData`). Interactive frames (1.5 MP) cost ~3 ms of overhead. Options:
   - cap detail at viewport size exactly (not the next pyramid level up);
   - send only the visible region when zoomed;
   - evaluate `createImageBitmap` or WebGL texture upload versus `putImageData`;
   - measure WebView2 on Windows before deciding.
6. **Preview cache admission** (fixed). Every interactive frame used to be cached
   (~6 MB each), so a 40-frame drag evicted earlier settled renders. Interactive frames
   now go to a separate pool of 1/8 of the budget; thumbnail and detail renders go to
   the settled pool, which a drag cannot evict. Covered by
   `long_drag_does_not_evict_settled_renders`.
7. **What is not a bottleneck on this machine:** pyramid build (2–5 ms), interactive
   render (~2 ms), cancellation (< 1 ms p50, ≤ 4.4 ms max), thumbnail render
   (< 1 ms).

## 8. Suggested Phase 1 targets (to confirm on low-end hardware)

| Metric | Phase 0 (M1 Max) | Proposed target (reference low-end laptop) |
|---|---|---|
| Interactive preview round trip p95 | 4–5 ms | ≤ 16 ms |
| Frames shown during drag | 119/120 | ≥ 95% at display rate |
| Interactive p95 while exporting | 10–22 ms | ≤ 2× idle |
| Time to first visible image after open | 230–370 ms → 25–74 ms with embedded preview (§9); since ADR 0020 the editor shows only renders: **116–345 ms** | ≤ 600 ms rendered |
| 24 MP export | 0.65–1.2 s | ≤ 3 s |
| Editing memory, 24 MP open | ~170 MB | ≤ 300 MB |
| Export peak memory, 24 MP | ~645 MB | ≤ 400 MB |

A reference low-end machine (e.g. 4-core x86 laptop, 8 GB RAM, integrated GPU,
Windows) should be chosen and added to this document before targets are finalised.

## 9. After Phase 0 hardening: embedded preview and libjpeg-turbo

Measured after the baseline above. Single `bench` run (5 iterations) plus in-app
self-tests; same machine and caveats as §2.

### Time to first visible image (in app; ADR 0007)

Embedded preview decoded with libjpeg-turbo DCT scaling. Two self-test runs each;
"first open" is the first open after launch, "re-open" the same file once the app has
settled.

| File | Embedded preview shown (first open / re-open) | First render shown |
|---|---|---|
| Canon EOS R6 CR3 | 35–37 / 14–15 ms (1620x1080) | 146–166 ms |
| Nikon Z 6 NEF | 39 / 16 ms (1620x1080) | 389 ms |
| Sony A7 III ARW | 25–47 / 19 ms (1616x1080) | 205 ms |
| Sony A7R IV ARW (61 MP) | 29–40 / 14 ms (1616x1080) | 387 ms |
| Fujifilm X-T3 RAF | 50–61 / 34–35 ms (1104x736) | 307 ms |
| Ricoh GR III DNG | 60–74 / 46–48 ms (1500x1000) | 540–563 ms |
| synthetic chart DNG | — (none embedded) | 77–81 ms |

Warm extraction in isolation is 9–13 ms for ~1620 px previews, 31 ms (Fuji) and 40 ms
(Ricoh) for near-full-size ones. Full-size-only embedded JPEGs are bounded by entropy
decoding: a real 24 MP JPEG decodes in 103.5 ms at 1/1 and still 72.5 ms at 1/8. Peak
memory after open is ~166–173 MB for 24–26 MP files (+10–14 MB for the preview).

### JPEG encode, full resolution, q92 4:4:4 (ADR 0006)

| File | jpeg-encoder | libjpeg-turbo | Size |
|---|---|---|---|
| Canon EOS R6 CR3 (20 MP) | 167.7 ms | 70.8 ms | 3.6 MB (identical) |
| Nikon Z 6 NEF (24.5 MP) | 194.5 ms | 80.6 ms | 3.2 MB (identical) |
| Sony A7 III ARW (24.2 MP) | 262.1 ms | 108.5 ms | 11.8 MB (identical) |
| Sony A7R IV ARW (61 MP) | 515.3 ms | 206.9 ms | 12.1 MB (identical) |
| Fujifilm X-T3 RAF (26 MP) | 225.1 ms | 94.0 ms | 4.9 MB (identical) |
| Ricoh GR III DNG (24.2 MP) | 242.9 ms | 99.7 ms | 8.7 MB (identical) |

Export totals now (decode / render / encode / write, ms): Nikon 806 / 36 / 81 / 8;
61 MP Sony 1457 / 85 / 210 / 11. Decode is now ~80% of export for Bayer files.

## 10. Library indexing (Phase 2; ADRs 0013, 0014)

`cargo run -p bench --release -- --index-scale 10000` generates 10,000 distinct 70 KB
files in 100 folders. `--index-links 10000` indexes 10,000 hard links to the six real
camera samples, so it parses real RAW headers. Both use warm OS cache and an on-disk
catalogue.

| Pass (10,000 files) | Distinct synthetic files | Real RAW headers (hard links) |
|---|---|---|
| First index (record + read details) | 817 ms | 1,864 ms (details: 978 ms) |
| Rescan, nothing changed | 74 ms | 101 ms |
| Rescan, 1% changed | 88 ms | — |

- The directory walk takes ~9 ms, and the catalogue including WAL is ~11 MB.
- Reading details from real RAW headers costs ~0.25–0.4 ms per file warm (~2 ms for
  a first read); JPEG EXIF ~0.13 ms. In parallel that's ~1 s per 10,000 photos.
- The first `--index-links` run exposed quadratic move detection with many identical
  files: **36.8 s**. It's fixed by migration 3 plus bounded candidates; see ADR 0014.
- That index costs rescans ~25 ms per 10,000 files (A/B: 50 → 76 ms), a deliberate
  trade for a linear worst case. In the synthetic first index, reading details takes
  218 ms: every file is tried and rejected as not a RAW file.
- In the app, indexing the 8 camera samples through IPC takes 2–8 ms (4–5 ms with
  details), and a rescan under 1 ms.
- Not yet measured: a cold OS cache, spinning disks and network drives. The first
  index reads 128 KB per new file (fingerprint) plus the file headers.

## 11. Library thumbnails (Phase 2; ADR 0015)

`cargo run -p bench --release -- --thumbnails` covers each fixture file and a
"screenful" of 48 thumbnails requested at once: hard links to the camera files, cold
cache, warm OS cache.

| File | Made from | Cold | Cached | Size |
|---|---|---|---|---|
| Nikon Z 6 NEF | embedded preview | 5.2 ms | 0.03 ms | 30 KB |
| Canon EOS R6 CR3 | embedded preview | 11.7 ms | 0.03 ms | 40 KB |
| Sony A7R IV ARW (61 MP) | embedded preview | 12.6 ms | 0.03 ms | 30 KB |
| Sony A7 III ARW | embedded preview | 16.7 ms | 0.03 ms | 56 KB |
| Fujifilm X-T3 RAF | embedded preview | 34.4 ms | 0.03 ms | 34 KB |
| Ricoh GR III DNG | embedded preview | 43.7 ms | 0.03 ms | 36 KB |
| synthetic 24 MP JPEG | DCT-scaled decode | 20.7 ms | 0.03 ms | 15 KB |
| synthetic 24 MP DNG | decode + render (no preview) | 182 ms | 0.03 ms | 15 KB |

| Screenful (48) | First | All | Rate |
|---|---|---|---|
| Browse lane, 3 workers (default on 10 cores) | 12 ms | 341 ms | 141/s |
| 1 worker (low-end stand-in) | 12 ms | 995 ms | 48/s |
| 3 workers while indexing 3,000 files | 11 ms | 344 ms | 139/s |

- **In the app** (self-test, real IPC): the 8 fixture photos take ~230 ms together,
  dominated by the 182 ms synthetic DNG. A cached thumbnail's round trip is ~1 ms.
- **Indexing no longer delays thumbnails:** they run on their own lane, and the
  concurrent index pass finished normally (538 ms).
- **Pre-generation after indexing** (ADR 0015 §7): 300 thumbnails (camera-file hard
  links) at idle priority on the single background worker take 6.2 s (48/s), so about
  3.5 min per 10,000 photos. An on-screen thumbnail requested mid-batch still takes
  11.5 ms, because it runs on its own lane.
- **Edited photos** (ADR 0019): their thumbnails come from a reduced decode plus
  render, 175–470 ms each (once, then cached), against 5–40 ms from the embedded
  preview.
- **Worth investigating:** the Fuji and Ricoh cases are 3–8x slower than Nikon. That
  is likely LibRaw's container parsing or a larger embedded JPEG. Not yet profiled.
- **Not yet measured:**
  - a cold OS cache (the embedded preview is read from the RAW file);
  - network drives;
  - a real low-end machine;
  - UI frame times while 48 thumbnails decode in the webview.

