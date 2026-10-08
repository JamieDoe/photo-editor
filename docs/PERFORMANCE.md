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
     for Bayer files (Nikon 451 MB, 61 MP Sony 1.05 GB) and 24.5 B/px for X-Trans
     (28.1 B/px at 10 threads since the deterministic X-Trans demosaic, §43).
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

## 12. Default look vs camera JPEGs (ADR 0022)

`cargo run -p bench --release -- --look` compares, per camera file, our default render's
luminance distribution with the camera's embedded JPEG (11 percentiles; RMS in sRGB
units). `--flat` measures the old flat look.

| | Canon R6 | Fuji X-T3 | Nikon Z 6 | Ricoh GR III | Sony A7 III | Sony A7R IV | Mean |
|---|---|---|---|---|---|---|---|
| Flat (recipe v1) | 0.207 | 0.094 | 0.204 | 0.105 | 0.176 | 0.023 | 0.135 |
| Standard (v2 default) | 0.072 | 0.083 | 0.046 | 0.052 | 0.018 | 0.045 | 0.053 |

The base curve is one lookup-table pass (the same cost as contrast). Its effect on
interactive render time is within noise.

## 13. Highlights / Shadows / Whites / Blacks (ADR 0023)

Tone stage on the 1516×1010 interactive level (bench recipe, development Mac, noisy,
±10 %):

| Version | Stage cost | Full render |
|---|---|---|
| First: per-pixel `exp2`, map rebuilt every render | +6.6 ms | 9.4 ms |
| Lookup-table gains, per-row map interpolation | +4.0 ms | — |
| Map cached while it cannot change | +2.0 ms | 4.5 ms |

- **Building the map:** about 2.8 ms at this size. It is rebuilt when exposure or
  white balance change.
- **Low-end hardware:** not yet measured.

## 14. Temperature / Tint / Vibrance (ADR 0024)

Stage costs on the 1516×1010 interactive level (Nikon Z 6, bench recipe, three runs):

| Cumulative stages | ms |
|---|---|
| none (u16→f32 + encode) | 0.89 – 1.09 |
| + white balance (Temperature and Tint) | 0.94 – 1.12 |
| + … base curve | 4.36 – 4.37 |
| + vibrance | 4.77 – 4.83 |
| + saturation (whole recipe) | 4.89 – 5.13 |

- **White balance:** merged into the channel gains, so Tint costs nothing extra.
- **Vibrance:** about +0.45 ms.
- **Release self-test:** it changes Temperature, Tint and Vibrance on every drag
  frame.
  - Median render: 3.2 ms (Nikon), 2.8 ms (Canon), 3.3 ms (Fuji).
  - Preview: 30 fps on all three.

## 15. Colour mixer (ADR 0025)

Mixer stage on the 1516×1010 interactive level (Nikon Z 6, three runs):

| Version | One band edited | All eight bands |
|---|---|---|
| First | +3.5 to +3.9 ms | not measured |
| Untouched hues skip the work; no `rem_euclid` | +1.48 to +1.55 ms | +3.0 to +3.9 ms |

- **Whole bench recipe** (every Light and Colour control plus one mixer band): about
  6.4 ms.
- **Release self-test** (mixer, Temperature, Tint and Vibrance dragged together):
  median render 4.9–5.8 ms, 30 fps on Nikon, Canon and Fuji.
- **Low-end hardware:** not yet measured. If it needs it, a branch-free SIMD version is
  the next step.

## 16. Texture and Clarity (ADR 0026)

Nikon Z 6, bench recipe (texture 20, clarity 25), three runs:

| Measure | First version | Taller chunks + gain lookup table |
|---|---|---|
| Detail stage, 1516×1010 | +3.18 – 3.21 ms | +3.10 – 3.15 ms |
| Full-resolution render | 202 – 224 ms | 150 – 158 ms |
| Export render | 357 – 360 ms | 274 – 275 ms |

- **Without the stage:** full resolution was about 70 ms.
- **Where the full-resolution cost comes from:** the blur's overlap rows. Chunks of 10
  rows with 18 rows of reach either side meant computing ~4.6× the rows. Chunks are
  now at least 8 × radius rows tall.
- **Release self-test worst case** (exposure and every slider change each frame, so
  the surroundings map is rebuilt too):
  - median render 9.7–12.3 ms, 95th percentile 13–19 ms;
  - 30 fps preview.
- **Next step if needed:** cache the blurred plane between frames.

## 17. Sharpening (ADR 0027)

Default recipe on the 1516×1010 interactive level (Nikon Z 6, medians of 40 renders):

| Default recipe | ms |
|---|---|
| Without sharpening | 1.6 |
| Sharpening 40, first version (two box-blur passes and plane copies) | 4.7 – 5.5 |
| Sharpening 40, final (3×3 blur per row, Texture/Clarity work skipped) | 2.9 – 3.0 |

Bench recipe (Texture 20, Clarity 25, Sharpening 40):

- Detail stage: +3.4 to +3.9 ms; total 10.4 ms.
- Full-resolution render: 163–174 ms; export render: about 295 ms.

Release self-test, all controls dragged together:

- median render 10.3–12.3 ms, 95th percentile 12.5–14.4 ms;
- 30 fps.

## 18. Dehaze and the shared scene map (ADR 0028)

Nikon Z 6, 1516×1010 interactive, bench recipe (every Light, Colour and Detail control
set, Dehaze 20), three runs:

| Measure | ms |
|---|---|
| Dehaze stage | +0.7 – 1.2 |
| Detail stage (with dehazed luminance) | +5.6 – 6.4 (was 3.4 – 3.9) |
| Whole render, maps cached | 13.3 – 14.4 (was 10.4) |
| Full-resolution render | 215 – 220 |
| Export render | 366 – 371 |

Self-test worst case (exposure and everything change every frame):

| Version | Median | 95th percentile |
|---|---|---|
| First | 18.5 – 21.2 ms | 25 – 26 ms |
| Gain-free map cached, rescaled per exposure/WB | 16.1 – 18.3 ms | 24 – 26 ms |

- **Preview:** 28–30 fps throughout.
- **Why caching helps:** averaging is linear, so a white-balance or exposure change
  only rescales the 256-px map instead of re-reading the image.

## 19. Noise reduction (ADR 0030)

Nikon Z 6, `main` and this branch alternated three times each, bench recipe with
Noise reduction 30, load average 7–13:

| Measure | `main` | With noise reduction |
|---|---|---|
| Interactive, every control set | 11.8 ms | 15.3 – 16.7 ms |
| Full-resolution render | 199 – 204 ms | 308 – 314 ms |
| Export render | 321 – 323 ms | 488 – 569 ms |
| Peak memory | 1483 – 1485 MB | 1549 – 1586 MB |

Getting there:

- **First version:** colour filtered per chunk at full resolution. +26 ms
  interactive, ~900 ms at full resolution, 2.2 GB peak.
- **Box blur:** the horizontal pass (1.3 ns/px, a dependent running sum) became
  shifted-row sums for small radii and eight rows at once for large ones.
- **Colour:** one 512-cell map per render, cached. It ignores exposure when there is
  no dehaze.
- **Memory:** kernel buffers now per render instead of per thread (−650 MB peak).
- **Profiling tool:** macOS `sample` on a render loop.

Preview scheduler: one interactive render in flight. Before, a drag whose renders
took longer than a frame showed no frames at all.

## 20. Vignette and Grain (ADR 0031)

Nikon Z 6, 1516×1010, default recipe, median of 60 renders, three runs (load average
about 10):

| Recipe | ms |
|---|---|
| Default | 3.2 |
| + Vignette −40 | 4.2 |
| + Grain 40 | 6.6 – 6.8 |

- **Grain as a precomputed 4 MB noise tile:** measured slower (+6 ms, cache traffic),
  so it stays procedural.
- **Grain's per-pixel `exp2`:** replaced by the stop-gain lookup table.

## 21. Crop and straighten (ADR 0032)

Nikon Z 6, 1516×1010, median of 40, three runs:

| Case | ms |
|---|---|
| Default recipe | 3.2 |
| Straightened + cropped, another control dragged (framed image cached) | 3.2 |
| Straighten dragged (bilinear resample every frame) | 8.3 |

Crops choose a larger pyramid level so the kept part stays sharp. Those renders cost
in proportion to the pixels kept.

## 22. Auto level (ADR 0033)

Measured on a preview level of about 1000 px:

- about 33–39 ms each on the samples (the `auto_level` estimate alone);
- 28–52 ms through the release app.

It runs once per click, as an interactive job.

## 23. Perspective (ADR 0034)

Resample of a 1516×1010 frame, median of 40, three rounds, machine load 20–30:

| Case | ms |
|---|---|
| Straighten only (bilinear) | 5.5–6.2 |
| Perspective and straighten (homography, bilinear) | 5.7–6.9 |

- The projective divide costs little next to the bilinear fetches.
- Fitting the crop by binary search takes about 2 µs.
- Release self-test, Nikon Z 6: a perspective render at the fitted crop's level
  (5542×3692 kept) took 12.4 ms.

## 24. Remove chromatic aberration (ADR 0035)

Measuring runs once, when the switch is turned on. Scratch probe, release build,
machine load 5–7:

| Source | Level measured | ms |
|---|---|---|
| Canon R6, 20 MP | 2748×1835 | 233 |
| Nikon Z 6, 24 MP | 3032×2020 | 252 |
| Sony a7 III, 24 MP | 3012×2012 | 285 |
| Fuji X-T3, 26 MP | 3123×2085 | 274 |
| Sony a7R IV, 61 MP | 4784×3188 | 790 |

- The app measures on the level nearest 2000 px or above. On the 61 MP file that is
  2392 px: 178 ms in the release self-test (Nikon: 252 ms).
- Framing resample at the interactive level (about 1500 px), median of 30:
  - plain copy: 0.3 ms;
  - with the correction: 4–7 ms (bilinear, three sample points per pixel).
  - It runs only when the framing changes. The framed image is cached while other
    controls are dragged.
- Found at full size (largest of red and blue at the corners):
  - Fuji X-T3: 7.2 px;
  - Sony a7R IV: 6.1 px;
  - Nikon: 1.7 px;
  - Sony a7 III: 1.5 px;
  - Canon: 1.0 px;
  - Ricoh GR III: 0.7 px;
  - synthetic chart: 0.0 px.


## 25. Histogram (ADR 0036)

`renderer::Histogram::of` on an RGBA8 frame, median of 50, three rounds, machine
load about 4:

| Frame | ms |
|---|---|
| 1516×1010 (interactive) | 0.37–0.52 |
| 3032×2020 (detail) | 1.15–1.20 |

- It is computed in the render job (and on cache hits), not on the UI thread.
- Each frame gains 4 KB.
- Release self-test, Nikon Z 6: all 162 viewer frames carried a histogram that counted
  every pixel. The interactive render median was 30 ms, as before (§22).

## 26. Tone curve (ADR 0037)

A full render of a 1516×1010 frame (default look, no sharpening), median of 40, three
rounds, machine load about 7:

| Case | ms |
|---|---|
| No curve | 1.75–2.23 |
| With a four-point curve | 2.64–2.93 |

- The cost is the 4096-entry lookup table built for each render, plus one lookup per
  channel per pixel.
- Release self-test, Nikon Z 6: a render with a curve took 5.6 ms. The frame's mean
  luminance rose from 155 to 192.

## 27. Red, green and blue curves (ADR 0038)

Full render of a 1516×1010 frame (default look, no sharpening), median of 40, three
rounds, machine load about 9:

| Case | ms |
|---|---|
| No curve | 1.68–1.88 |
| RGB curve | 3.19–3.31 |
| RGB curve and all three channel curves | 3.25–3.38 |

- The channels compose with the RGB curve into one table per channel, about 0.1 ms
  more.
- Without channel curves the single shared table is kept.
- These RGB figures are higher than §26's (2.6–2.9 ms) because the machine was
  busier.

## 28. Masks (ADR 0040)

A full render of a 1516×1010 frame (default look), median of 40, three rounds, machine
load about 8:

| Case | ms |
|---|---|
| No masks | 2.93–3.79 |
| One linear gradient: Exposure and Warmth | 5.06–5.19 |
| Three linear gradients: Exposure and Warmth | 5.56–5.81 |
| One linear gradient: Clarity | 5.23–5.63 |

- Most of a gain mask's cost is three `exp2` per pixel.
- Coverage (a dot product and a smoothstep per mask) is cheap. A lookup-table `exp2`
  would halve the cost if it matters.
- Release self-test, Nikon Z 6: a render with a mask took 6.3–6.8 ms.

## 29. Radial masks (ADR 0041)

A full render of a 1516×1010 frame (default look), median of 40, three rounds, machine
load about 7–8. Each mask has Exposure and Warmth:

| Case | ms |
|---|---|
| No masks | 2.79–2.96 |
| One linear gradient | 4.86–4.89 |
| One radial gradient, inverted | 5.43–5.49 (one 8.39 under load) |

- A radial's elliptical distance adds a square root per pixel, about 0.6 ms over a
  linear gradient.
- Release self-test, Nikon Z 6: an inverted radial darkens the frame's top by 44.6
  levels and leaves the middle unchanged.

## 30. Brush masks (ADR 0042)

Rasterising strokes into the 2048 × 1365 coverage map of a 3:2 frame, median of 10,
three rounds, machine load about 7. Strokes are wavy, 200 points each:

| Case | Before optimising | After |
|---|---|---|
| One stroke, small (0.02 of the diagonal) | 6.1–7.7 ms | 1.2–2.2 ms |
| One stroke, large (0.08) | 63.5 ms | 7.2–8.2 ms |
| Ten strokes, medium (0.04) | 180 ms | 21.8–23.0 ms |
| Painting an 11th stroke over ten cached ones, per update | — | 2.4 ms median, 2.9 ms max |

- The optimisations are path simplification, parallel rows testing only nearby
  segments, and incremental painting over the cached map of the earlier strokes.
- Reading the map during a render is one bilinear sample per pixel per mask.
- Release self-test, Nikon Z 6: the first render with a brush mask, rasterising
  included, took 10–27 ms depending on load.
- The app's frame times while painting through the UI (40 pointer moves, two runs,
  load about 8):

  | Stroke | SVG, blurred (first) | SVG, stepped rings (second) | Canvas (now) |
  |---|---|---|---|
  | Painting | p50 25 ms, p95 36 ms | p50 17 ms, p95 21–34 ms | p50 17 ms, p95 23–24 ms |
  | Erasing | p50 49 ms, p95 59 ms | p50 17 ms, p95 17–37 ms | p50 17 ms, p95 17–18 ms |
  | A large brush after 1, 2 and 3 erase strokes | — | p50 44, 83 and 186 ms | p50 17 ms each (p95 ≤ 22 ms) |

  Each erase stroke nested everything before it in another SVG mask, which the web view
  redraws on every update. The canvas cuts erases out directly (`destination-out`) and
  keeps finished strokes in a cached layer, so the cost no longer grows with the strokes.

## 31. Masks of several shapes (ADR 0043)

A full render of a 1516×1010 frame (default look), median of 40, three rounds, machine
load about 6–7, on battery. The mask has Exposure and Warmth:

| Case | ms |
|---|---|
| No masks | 3.05–3.29 |
| A linear gradient | 5.32–5.48 |
| The linear, minus a radial | 6.18–6.35 |
| The same at density 50 | 6.18–6.31 |
| The linear, intersected with a radial, plus a brush stroke | 7.34–7.55 |

- Each further shape adds about 0.7–1.2 ms; density is one multiply per pixel.
- Release self-test, Nikon Z 6: the combined mask (a linear minus a hard radial)
  rendered in 6.7–7.5 ms. The photo inside the circle is unchanged and beside it 42.2
  levels darker.
- Painting through the UI now draws the tint through the mask's combined canvas. In
  Low Power Mode the web view ran at 30 fps (idle frame gap p50 33 ms), and painting
  matched it: p50 33 ms, p95 34–35 ms for all nine large strokes, erases included.
  Photo round trip p50 16 ms, render p50 11 ms.
- Out of Low Power Mode (two runs, load about 3–5): idle frame gap p50 17 ms; all nine
  large strokes p50 17 ms, p95 18–19 ms, max 21 ms (60 fps). Photo round trip p50
  14–15 ms, render p50 9.6–10.5 ms.

## 32. Preset previews (ADR 0046)

Release self-test, Nikon Z 6, machine load 6–8: each preset preview (thumbnail
quality, 256 px, the `presets` render slot) takes 2–5 ms from request to frame. The six
built-in presets take about 22 ms in all. Previews are redrawn only when the photo's
own settings (exposure, geometry, masks) change, 300 ms after the change stops, and
never while a look slider moves.

## 33. Batch edits (ADR 0049)

Merging a copied edit into each photo's saved edit and saving it (`paste_onto`,
release, machine load 6–8), 1,000 photos, three rounds:

| Catalogue | ms per 1,000 photos |
|---|---|
| In memory | 18–19 |
| On disk | 32 |

Release self-test, Nikon Z 6 folder: syncing onto 7 photos took 1–7 ms through the
command. A batch needs no progress bar; the thumbnails re-render afterwards, as each
is shown.

## 34. The export queue (ADR 0050)

Release self-test, Nikon Z 6 fixture folder, machine load 6–8:

| Export | Time per photo |
|---|---|
| 1350 px long edge (decoded at reduced scale, rendered, shrunk in linear light) | ~300 ms |
| Full size (§4 and §7: decode / render / encode / write 806 / 36 / 81 / 8 ms) | ~930 ms |

- Photos export one after another: a full-size export holds about 26 bytes a pixel, so
  running several at once would multiply peak memory.
- A sized export decodes at the smallest scale that fills its long edge after the crop,
  which cuts both time and memory.
- Cancelling at the first progress event stopped a full-size run before its first
  photo finished.

## 35. Colour grading (ADR 0052)

A full render of a 1516×1010 frame (default look) with shadows and highlights graded,
median of 40, three rounds, machine load 5–7. An ungraded render takes 3.3–3.5 ms.

| Approach | Graded render | Added |
|---|---|---|
| Oklab round trip per pixel | 14.3–14.5 ms | +11 ms |
| Shift tabulated over lightness, Oklab per pixel | 9.1–9.2 ms | +5.9 ms |
| As above, with a fast cube root | 8.6–8.8 ms | +5.3 ms |
| A grey's grading tabulated: gain plus tint (kept) | 4.7–5.1 ms | +1.4–1.7 ms |

Release self-test, Nikon Z 6: a split-toned black and white frame rendered in 7.3 ms.

## 36. Calibration (ADR 0053)

A full render of a 1516×1010 synthetic chart (default look, no sharpening), median of
40, three rounds, machine load 5.6–7.9. An uncalibrated render takes 1.74–1.84 ms.

| Setting | Render | Added |
|---|---|---|
| Primaries (one 3×3 matrix) | 2.72–2.82 ms | about +1.0 ms |
| Primaries and Shadow Tint (colour grading's table, kept) | 5.16–5.26 ms | about +2.4 ms more |
| Primaries and Shadow Tint, tint tabulated over √luminance | 4.69–5.42 ms | about +2.0 ms more (not kept: within noise, more code) |

Release self-test, Nikon Z 6: a calibrated frame (primaries) rendered in 5.0 ms.

## 37. Heal and clone spots (ADR 0054)

Retouching a source, synthetic content, five heal spots of radius 0.02 (of the long
edge), median of 9, machine load 5.3–7.6:

| Source | Five heal spots | Of which the copy | Finding a source |
|---|---|---|---|
| 1516×1010 (preview level) | 2.25 ms | 0.29 ms | 0.047 ms |
| 6064×4040 (24 MP export) | 12.2 ms | 3.5 ms | 0.050 ms |

The retouched source is cached, so this is paid once per change to the spots, not per
frame. Release self-test, Nikon Z 6, preview level:

| Frame | Render |
|---|---|
| Without spots (exposure +0.3) | 4.3 ms |
| First render with one spot (retouches) | 5.8 ms |
| With the spot cached, exposure +0.3 | 4.4 ms |

## 38. Search and Recently imported (ADR 0056)

In-memory catalogue of 20,000 photos (paths like `Library/2019/Trip 42/DSC_01234.NEF`,
with camera, lens and capture time), release build, median of 7:

| Query | Results | Time |
|---|---|---|
| "trip 42" | 797 | 41.8 ms |
| "z 6 2019" | 667 | 23.5 ms |
| "september 12" | 195 | 22.7 ms |
| "nothing-here" | 0 | 21.4 ms |
| "dsc_01" | 1,000 | 22.4 ms |
| Recently imported (all 20,000) | 20,000 | 27.8 ms |
| Collection counts (with recent) | — | 5.0 ms |

A search runs on a blocking thread 180 ms after typing pauses, and only the newest
query's results are shown. Its cost is a `LIKE` scan, about 1–2 µs per photo, so a
100,000-photo library would take about 0.1–0.2 s. A full-text index is the next step if
that becomes noticeable.

## 39. TIFF and PNG export (ADR 0057)

Full size (6064×4040), release build, machine load 7–10. The encodes use a synthetic
photo-like image of smooth gradients with slight noise; real photos compress less, so
their files are larger and slower.

| Step | Time | Size |
|---|---|---|
| Render, 8-bit output (default look) | 23 ms | — |
| Render, 16-bit output (exact sRGB curve) | 56 ms | — |
| Encode JPEG q85 (libjpeg-turbo) | 64 ms | 0.7 MB |
| Encode PNG (8-bit, balanced Deflate) | 862 ms | 3.1 MB |
| Encode TIFF (16-bit, Deflate, horizontal predictor) | 542 ms | 2.3 MB |

Release self-test, Nikon Z 6, 1,350 px through the export queue (decode included):

| Format | File size | Time |
|---|---|---|
| JPEG | 267 KB | — |
| PNG | 1.5 MB | 0.62 s |
| 16-bit TIFF | 5.9 MB | 0.56 s |

The JPEG time is not measured separately in this step.

All of this runs on the background export lane; the editor stays interactive.

## 40. Finding sensor dust (ADR 0058)

`bench --dust`: each camera file opened in the engine, then dust found on the preview
level of about 2000 px. Median of 3, release build, machine load 7–10.

| File | Found | Before parallel blurs | After |
|---|---|---|---|
| Canon EOS R6 (CR3) | 0 | 152 ms | 51 ms |
| Fujifilm X-T3 (RAF) | 0 | 256 ms | 44 ms |
| Nikon Z 6 (NEF) | 0 | 194 ms | 43 ms |
| Ricoh GR III (DNG) | 1 | 201 ms | 54 ms |
| Sony A7 III (ARW) | 0 | 208 ms | 54–66 ms |
| Sony A7R IV (ARW) | 0 | 110 ms | 31 ms |

Release self-test: Nikon Z 6, 53 ms; Ricoh GR III, 70 ms (with the call over IPC).
It runs once per photo when Retouch opens, on the interactive lane.

## 41. Output sharpening (ADR 0059)

Sharpening a 6064×4040 image on its own (release build, every core, median of 5,
machine load 7–8):

| Version | 8-bit Screen | 8-bit Matte | 16-bit Screen | 16-bit Matte |
|---|---|---|---|---|
| First (float copies of everything, ~1 GB at peak) | 82 ms | 98 ms | 86 ms | 101 ms |
| Kept (two float planes, ~190 MB, written in place) | 58 ms | 78 ms | 63 ms | 86 ms |

Release self-test, full-size export of the Nikon Z 6 on the export lane (its bounded
thread pool), Screen sharpening:

| Build | Render (incl. sharpening) | Total |
|---|---|---|
| First version | 342–469 ms | 1.5–1.8 s |
| Kept | 230 ms | 1.38 s |
| Before sharpening (ADR 0057) | 128–171 ms | 1.40 s |

## 42. Soft gamut compression (ADR 0060)

Per pixel over a 24 MP image (6064×4040), release build, every core, median of 5,
machine load about 5:

| Step | Time at 24 MP | At a 1516×1010 preview |
|---|---|---|
| Decode: Rec.2020 → sRGB with compression (every raw decode) | 67 ms | about 4 ms |
| Output compression (only plans with colour edits; most pixels exit at once) | 26 ms | about 1.7 ms |

For scale, a full-size raw decode takes about 1 s, so the conversion adds about 7%. A
half-size preview decode (6 MP) adds about 17 ms.

## 43. Deterministic X-Trans decoding (ADR 0061)

Fujifilm X-T3, release build, `main` and the fix run alternately (median of 3–9
decodes, machine load 3–13). "Differing" is the share of pixels that changed between
two decodes of the same file.

| Decode | Threads | Before | After | Differing, before → after |
|---|---|---|---|---|
| Preview (3123×2085) | 10 | 193 ms | 189 ms | 3.6–4.1% → 0 |
| Preview | 5 | 313 ms | 304 ms | |
| Full (6246×4170) | 10 | 2304 ms | 2351 ms | up to 0.06% → 0 |
| Full | 5 | 4.15 s | 3.66 s | |

Peak memory of a lone full decode (`bench --decode-peak`): 640 → 734 MB at 10
threads. It is unchanged at 5 threads (the export lane) and at 2 threads. Every Bayer
fixture decodes byte-identically to before.

## 44. Export colour space (ADR 0062)

Release bench, full-size JPEG (q92) of the Nikon Z 6 (24.5 MP), median of 3, no
sharpening, on a loaded machine:

| Colour space | Render (incl. conversion) | Total | File |
|---|---|---|---|
| sRGB (8-bit render) | 766 ms | 1.83 s | 3.72 MB |
| Display P3 (16-bit render, converted) | 989 ms | 2.03 s | 3.47 MB |
| Adobe RGB (16-bit render, converted) | 957 ms | 2.00 s | 3.43 MB |

The wider spaces cost about 0.2 s per full-size export: the 16-bit render and one pass
of decode, matrix and encode per pixel across every core. The files are slightly
smaller because sRGB colours sit further from the edges of the wider spaces, so the
same image uses a narrower range of values. `cargo run -p bench --release` reports
these as `export_colour_spaces`.

## 45. Export metadata (ADR 0063)

Reading a source's header for its capture facts, release build, mean of 20 reads after
one warm-up:

| File | Read |
|---|---|
| Nikon Z 6 (NEF) | 0.26 ms |
| Canon EOS R6 (CR3) | 0.24 ms |
| Ricoh GR III (DNG) | 0.22 ms |

Writing the EXIF block is a few hundred bytes of serialisation. Both are negligible
next to an export (1–2 s at full size). The block adds about 270 bytes to a JPEG
without a location.

## 46. Library sorting (ADR 0064)

The Library sorts in the UI (vitest, Node on the dev Mac), 20,000 photos with mixed
capture times, names and ratings:

| Order | Time |
|---|---|
| Capture time | 39 ms |
| File name (natural, `Intl.Collator`) | 58 ms |
| Rating, then capture time | 33 ms |

A sort runs when the view, the order or a photo's marks change, not on scrolling.
The work grows as n log n, so typical folders of a few hundred photos cost far less.

## 47. All photos listing (ADR 0065)

Release bench (`bench --index-scale 20000`: 20,000 synthetic files, indexed into an
on-disk catalogue), two runs:

| Step | Time |
|---|---|
| `collection(All)` query, with details and marks | 26.1–26.8 ms |
| Grant filter by canonicalising each path (before) | 233.7–238.5 ms |
| Grant filter by prefix on the stored canonical path (now) | 3.4–3.5 ms |

The prefix check is about 70× faster and touches no files. Reads of listed files are
still fully checked.

## 48. Remove: content-aware fill (ADR 0066)

`bench --remove`: the fill alone on each camera file, at the preview size the
interactive view uses (a half-size decode) and at full size. Release build, every core,
median of 3, two runs. The removals, in photo fractions:

- **Small:** a dab 1 % of the diagonal across, a bird or a sign.
- **Wire:** a 0.3 %-wide stroke across the whole frame.
- **Person:** a stroke 8 % of the diagonal wide and 30 % of the height long.

| File | Small, preview / full | Wire, preview / full | Person, preview / full |
|---|---|---|---|
| Canon EOS R6 (20 MP) | 12 / 23 ms | 166 / 423–428 ms | 258–262 ms / 1.12–1.16 s |
| Fujifilm X-T3 (26 MP) | 13–14 / 26 ms | 185–200 / 485–516 ms | 371–372 ms / 1.55–1.57 s |
| Nikon Z 6 (24 MP) | 13–14 / 25–27 ms | 192 / 471–475 ms | 348–368 ms / 1.49–1.54 s |
| Ricoh GR III (24 MP) | 13–14 / 25 ms | 181–235 / 472–485 ms | 306–309 ms / 1.34 s |
| Sony A7 III (24 MP) | 13 / 24–25 ms | 190–191 / 510–520 ms | 312–314 ms / 1.37 s |
| Sony A7R IV (61 MP) | 25 / 56 ms | 388–397 ms / 1.06–1.07 s | 806–842 ms / 3.83–3.85 s |

The cost follows the hole's area and the context around it, not the photo's size: a
small object is the same few milliseconds anywhere. The fill is cached with the
retouched source, so it is paid once per change to the removals, not on every slider
drag. While it runs (on the render's background lane) it can be cancelled.

**Tuning:**
- **A thin wire** is coarse enough at full size to need no pyramid. Its single level
  got the coarse level's 10 rounds at first; 5 brought the wire preview from 233–294 ms
  to 166–235 ms with the same result in the tests.
- **Starting from the edge inward** (ADR 0066) cost the same as the smooth first guess
  it replaced.
- **Matching the fill's tone to its edges** (ADR 0066) costs 5–10 % once it is
  limited to the hole and its ring. For example, person-sized fills take 0.28–0.41 s
  on the preview and 1.20–1.66 s at full size on the 20–26 MP files, and 0.88–0.91 s /
  4.06–4.15 s on the 61 MP file. Over the whole work region it had cost about 0.5 s
  more.

## 49. Export size estimate (ADR 0068)

`bench --estimate`: each camera file exported for real and estimated from a ~1,024 px
sample, with the fitted model. Error = estimate / actual − 1, over the six files
(median, range):

| Format | 1,350 px | 2,048 px | Full size |
|---|---|---|---|
| JPEG 85 | +0.9 % (−6.9…+11.9) | −0.3 % (−3.0…+3.0) | +3.4 % (−10.3…+26.0) |
| JPEG 95 | +0.3 % (−2.3…+5.8) | −3.8 % (−7.0…+5.0) | −2.0 % (−17.1…+17.1) |
| PNG | +1.4 % (−1.4…+3.6) | −1.9 % (−3.4…+1.3) | −0.5 % (−5.9…+14.3) |
| 16-bit TIFF | +0.1 % (−0.1…+0.4) | −0.3 % (−0.8…+1.0) | −0.1 % (−1.4…+3.8) |

Before fitting, scaling the sample's bytes by pixel count alone overestimated full-size
JPEGs by up to 83 %.

**Speed:** an estimate takes 57 ms for the Nikon Z 6 (release self-test, through IPC).
It renders a ~1,024 px level and encodes it once, and runs only when the dialog's
choices settle.
