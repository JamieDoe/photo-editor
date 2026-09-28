# Architecture

Status: **Phase 0 prototype** (RAW/rendering architecture spike). This document
describes the system as it exists today. Product intent lives in `PRODUCT.md`;
rendering detail in `RENDERING.md`; measurements in `PERFORMANCE.md`; decisions in
`ADR/`.

## 1. Shape of the system

```text
apps/desktop (Tauri 2)
├── src/                React + TypeScript: controls, display, interaction only
│   ├── ipc/            typed command wrappers + generated payload types
│   └── features/       editor (viewer, sliders, preview scheduler), selftest
└── src-tauri/          thin command layer: IPC payloads <-> engine calls
        │
        ▼
crates/app-core         Engine: open / render_preview / export / close
   ├── jobs             lanes, priority, supersession, cooperative cancellation
   ├── cache            bounded LRU (byte budget), stable render keys
   ├── raw              Decoder trait + registry: LibRaw (RAW), zune-jpeg (JPEG)
   ├── renderer         EditRecipe -> RenderPlan -> RenderBackend (CPU)
   ├── export           encode + atomic write, never over the source
   ├── jpeg-turbo       libjpeg-turbo binding: JPEG encode, DCT-scaled decode
   └── image-core       LinearImage, OutputImage, Pyramid, colour maths, Cancellation

crates/fixtures         synthetic, copyright-free DNG/JPEG/linear test images
crates/bench            baseline benchmark harness
crates/gpu-spike        wgpu evaluation (not linked into the app; see ADR 0005)
```

Dependency direction is strictly downward. `image-core` depends on nothing in the
workspace; `renderer` and `raw` never know about each other, jobs, caches or Tauri;
`app-core` composes them; the Tauri crate only adapts IPC to `app-core`.

## 2. Responsibilities

| Layer | Owns | Must not |
|---|---|---|
| React (`apps/desktop/src`) | UI state, controls, when to request previews, blitting frames to a canvas | decode, process or resample pixels; touch the filesystem |
| Tauri commands (`src-tauri`) | IPC payload conversion, native dialogs, event emission | do heavy work on the main thread; expose engine internals |
| `app-core` | orchestration: file identity, open images, preview cache, job submission, export flow | depend on Tauri or UI concepts |
| `renderer` | recipe schema/versioning, render plan, CPU backend | do I/O; know which decoder produced the pixels |
| `raw` | converting files into `LinearImage` | apply edits |
| `jobs` | scheduling, cancellation, lane isolation | know what a job does |

## 3. Data flow

### Open

```text
UI "Open…" ─► open_image_dialog(onPreview channel) (Rust shows native dialog)
           ─► Engine::open_with_preview(path)   [interactive lane, supersedes previous open]
                SourceIdentity (size + mtime + head/tail fingerprint)
                embedded camera preview (≥ 1024px, DCT-scaled)  ──► channel ──► shown at ~15–75 ms
                DecoderRegistry.decode(AtLeast 1600px)   // LibRaw half-size for RAW
                Pyramid::build (2x box, down to >= 256px)
           ◄─ ImageSummary (dims, levels, timings)   ──► first real render replaces it
```

The embedded preview is a display-only placeholder (ADR 0007). The UI never lets it
replace a real render, and never lets frames of the previous image replace it.

Full resolution is **not** decoded on open. Previews come from the pyramid built on
a reduced-resolution decode; full resolution is decoded only for export.

### Edit (slider drag)

```text
slider change ─► setRecipe ─► PreviewScheduler (≤ 1 request per animation frame)
  ─► render_preview {imageId, recipe, quality, targetLongEdge}
       Engine: choose pyramid level for quality/target
               cache lookup (source id + recipe hash + size + renderer version)
               hit  ─► respond immediately, cancel in-flight viewer render
               miss ─► job on interactive lane, supersede key "viewer-preview"
                        (cancels the previous render) ─► CpuRenderer ─► cache insert
  ◄─ binary frame: 20-byte header + RGBA8 pixels
  ─► putImageData on a canvas (no pixel processing in JS)
... 180 ms after the last change: one "detail" render at viewport resolution
```

### Export

```text
UI "Export…" ─► export_image (Rust shows save dialog, validates destination)
  ─► Engine::export [background lane, bounded compute pool]
       full decode ─► render (RGB8) ─► JPEG encode (libjpeg-turbo) ─► atomic write (tmp + rename)
  ◄─ returns job id immediately; progress/finished/failed arrive as events
```

## 4. Typed IPC

- Payload types are Rust structs with `ts-rs` derives; TypeScript is generated into
  `apps/desktop/src/ipc/generated/` (`npm run bindings`). The recipe type and the
  adjustment ranges come from `renderer`, so UI sliders are built from engine specs,
  not hard-coded ranges.
- Preview frames bypass JSON: `render_preview` returns a `tauri::ipc::Response` with a
  fixed 20-byte little-endian header (`src-tauri/src/ipc.rs`, mirrored in
  `src/ipc/frame.ts`) followed by RGBA8 pixels. The UI wraps the buffer in an
  `ImageData` view without copying. The open commands stream the embedded preview in
  the same format over a `tauri::ipc::Channel` (flag bit 1), before they return.
- Errors cross IPC as `{ kind, message }`. `message` is photographer-facing; technical
  detail is logged on the Rust side. `kind: "cancelled"` means superseded and is
  silently ignored by the UI.
- The webview is granted only `core:default`. Native dialogs are opened from Rust, so
  the frontend never receives dialog or filesystem permissions.
- `open_image_path` accepts a path (needed for drag-and-drop and the self-test); it is
  read-only. Writing to a caller-supplied path is refused unless the app was launched
  in self-test mode.

### Development aids

- `PE_SELF_TEST=<image>`: the app drives its own UI through real IPC (open, simulated
  drag, before/after cache check, export while dragging), prints a JSON report and
  exits. Works with release builds and `tauri dev`.
- `http://localhost:1420/dev/mock.html` (while `npm run dev` runs): the UI with IPC
  mocked by `@tauri-apps/api/mocks`, for layout work without Rust. Not bundled.

## 5. Threads

| Thread | Work |
|---|---|
| Main / UI (webview) | React, canvas blit. Never waits on rendering. |
| Tauri async runtime | IPC command futures; waits on jobs via the blocking pool |
| `jobs-Interactive-0` | open, previews (uses rayon global pool: all cores) |
| `jobs-Background-0` | export (runs inside a rayon pool of `cores/2` threads) |
| rayon workers | data-parallel row chunks inside renders |

LibRaw is built with OpenMP by Homebrew. Each decode caps OpenMP at the calling
lane's compute-pool size (`DecodeOptions::max_threads`), so background decodes stay
within half the machine.

## 6. Non-destructive guarantees

- Decoders only read source files. No code path opens a source for writing.
- `export::validate_destination` rejects the source path (after canonicalisation) and
  wrong extensions; writes are atomic via a temporary file in the same directory.
- Integration tests assert source bytes and mtime are unchanged after export.

## 7. Persistence

Phase 0 persists nothing. There is no SQLite: recipes live in UI state for the session
only. `EditRecipe` is already versioned and serialisable (`to_json`/`from_json` with
migration hook) so Phase 2 can store it without changing its shape.

## 8. Known limitations (Phase 0)

See `PERFORMANCE.md` for measured consequences.

- One viewer: a single supersede key for previews; multi-view needs per-view keys.
- Full-resolution export holds the whole image in memory (no tiling).
- LibRaw's OpenMP threads are capped per decode via `DecodeOptions::max_threads`
  on macOS/Linux; Windows has no cap yet.
- Export peak memory is dominated by LibRaw's working set (17–25 bytes/pixel); see
  PERFORMANCE.md §7.4.
- JPEG EXIF orientation and embedded ICC profiles are ignored; exports carry no
  ICC profile or metadata.
- Windows: LibRaw is opened with a narrow-character path (non-ASCII paths will fail);
  the LibRaw DLL is not bundled.
- The recipe is not persisted and there is no undo history yet.
