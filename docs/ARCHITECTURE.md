# Architecture

Status: **Phase 2 in progress** (catalogue, background indexing, photo details,
thumbnails, ratings and flags, saved edits) on top of the Phase 1 desktop shell. This document describes the system as it exists
today. Product intent lives in `PRODUCT.md`;
rendering detail in `RENDERING.md`; measurements in `PERFORMANCE.md`; decisions in
`ADR/`.

## 1. Shape of the system

```text
apps/desktop (Tauri 2)
├── src/                React + TypeScript: controls, display, interaction only
│   ├── app/            shell: modes, error boundary, error handling
│   ├── components/     shared UI (error banner, quit dialog)
│   ├── ipc/            typed command wrappers + generated payload types
│   ├── lib/            small pure helpers (formatting, clipboard)
│   └── features/       library, editor, settings, selftest
└── src-tauri/          thin command layer: IPC payloads <-> engine calls
    ├── commands/       images, export, library, settings, system, selftest
    ├── state.rs        AppState: engine, settings, folder access, running exports
    ├── lifecycle.rs    quit/close handling, shutdown
    └── logging.rs      log sink, panic hook, error references
        │
        ▼
crates/settings         typed, versioned settings; crash-safe JSON store
crates/catalogue        SQLite catalogue: library folders, photos, files, identity
crates/folders          folder listings and recursive walks; FolderAccess scope
crates/platform         OS-level helpers (atomic file writes)
crates/app-core         Engine: open / render_preview / export / close
   ├── jobs             lanes (interactive, browse, background), priority,
   │                    supersession, cooperative cancellation
   ├── cache            bounded LRU (byte budget), stable render keys, disk cache
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
| Tauri commands (`src-tauri`) | IPC payload conversion, native dialogs, event emission, folder access checks, lifecycle | do heavy work on the main thread; expose engine internals |
| `settings` | preference schema, validation, persistence | contain catalogue data or UI state |
| `folders` | listing and walking folders, deciding which paths are granted | read photo contents |
| `catalogue` | library state in SQLite, file identity, migrations | hold photographs; be the only copy of anything (it is rebuildable) |
| `app-core` | orchestration: file identity, open images, preview cache, job submission, export flow | depend on Tauri or UI concepts |
| `renderer` | recipe schema/versioning, render plan, CPU backend | do I/O; know which decoder produced the pixels |
| `raw` | converting files into `LinearImage` | apply edits |
| `jobs` | scheduling, cancellation, lane isolation | know what a job does |

Visual design: design tokens in `apps/desktop/src/styles.css` and bundled Geist fonts,
following the app design (ADR 0016). New screens reuse the tokens and components.

## 3. Data flow

### Browse (Library)

```text
UI "Choose folder…" ─► choose_folder (Rust shows native folder dialog)
      ─► FolderAccess.grant(folder)   // canonical path; the only way to add access
      ─► settings.recent_folders += folder
      ─► folders::list_folder (subfolders + supported photos, natural order)
UI clicks a subfolder / breadcrumb ─► list_folder(path)  // must be inside a grant
UI clicks a photo ─► open_image_path(path)               // must be inside a grant
```

### Index (Library, background)

```text
choose_folder / Refresh ─► index_library_folder(path) ─► granted root containing path
  ─► Engine::index_folder [background lane, Priority::Indexing, supersedes same root]
       walk_photos (no symlinks, hidden skipped)
       per batch of 256: stat ─► touch_unchanged (fast path, no reads)
                         fingerprint the rest (parallel) ─► record_files (one transaction)
       finish_scan: files not seen ─► missing (only for complete passes)
       read details for the queue (new/changed photos): headers only, parallel,
         batches of 256 ─► set_details (one transaction)          [stage ReadingDetails]
  ◄─ library://index events: progress (stage, n of total), finished (counts) or failed
```

### Thumbnails (Library)

```text
row scrolls into view ─► library_thumbnail(path)       // must be inside a grant
  ─► disk cache hit (path + size + mtime + versions) ─► JPEG bytes, ~1 ms
  ─► miss: Engine::thumbnail [browse lane, VisibleThumbnail, supersedes same path]
       display_preview (embedded RAW preview / reduced JPEG decode)
         or reduced decode + default render ─► fit to 512 px ─► JPEG ─► disk cache
row scrolls away first ─► cancel_thumbnail(path) (queued job skipped)
index finished ─► pregenerate_thumbnails(root, present files)
  ─► one job per photo [background lane, Priority::Idle]; cached ones skipped;
     a new index of the same root cancels the previous batch
```

The Library's grid and list are virtualised: only rows on or near the screen are
mounted. Mounting requests a thumbnail and unmounting cancels it, so the work
follows what is visible.

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
- **Folder access scope** (ADR 0010). The webview can list folders and open photos
  only inside folders the user picked in the native dialog. Those grants are
  remembered as the default and recent folders, and settings updates from the UI
  cannot add folders. Paths are checked after canonicalisation, so `..` and
  symlinks cannot escape. Writing to a caller-supplied path is refused unless the
  app was launched in self-test mode.
- Errors carry a reference (e.g. `E-M2P8J-1`) that also appears in the log line with
  the technical detail (ADR 0008).

### Development aids

- `PE_SELF_TEST=<image>`: the app drives its own UI through real IPC (open, simulated
  drag, before/after cache check, export while dragging), prints a JSON report and
  exits. Works with release builds and `tauri dev`.
- `http://localhost:1420/dev/mock.html` (while `npm run dev` runs): the UI with IPC
  mocked by `@tauri-apps/api/mocks`, for layout work without Rust. Not bundled.

## 5. Settings, logging and lifecycle

- **Settings** (ADR 0009): `settings.json` in the OS config directory (macOS
  `~/Library/Application Support/dev.photoeditor.prototype/`). Loaded at start-up
  with defaults for anything missing; a corrupt file is moved aside. Saved
  atomically, debounced in the UI.
  - Applied live: theme, preview cache size, export quality.
  - Applied on next launch: background intensity.
- **Logging** (ADR 0008): `photo-editor.log` in the OS log directory (macOS
  `~/Library/Logs/dev.photoeditor.prototype/`), rotated at 5 MB × 5. It includes
  UI errors, panics, opens, exports and settings changes. Nothing leaves the machine.
- **Lifecycle** (ADR 0011):
  - One instance: a second launch focuses the existing window.
  - Window size and position are restored.
  - Closing or quitting during an export asks for confirmation. Confirming cancels
    the export and waits up to 5 s, so nothing half-written is left.

## 6. Threads

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

## 7. Non-destructive guarantees

- Decoders only read source files. No code path opens a source for writing.
- `export::validate_destination` rejects the source path (after canonicalisation) and
  wrong extensions; writes are atomic via a temporary file in the same directory.
- Integration tests assert source bytes and mtime are unchanged after export.

## 8. Persistence

- Persisted:
  - settings (including default and recent folders), window state, logs;
  - the catalogue (`catalogue.sqlite` in the OS app-data directory; ADRs 0012–0014).
    It holds library folders, photos, their files and photo details (camera, lens,
    capture time, exposure, dimensions, GPS). It is rebuildable by re-indexing, and a
    corrupt file is moved aside and rebuilt.
- Not persisted yet: edit recipes live in UI state for the session (a later Phase 2
  milestone). `EditRecipe` is already versioned and serialisable.

## 9. Known limitations

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
- Edits are saved per photo (ADR 0019), but there is no undo history yet (Phase 7).
- Library: folders are listed from the filesystem, one level at a time, joined with
  details and marks from the catalogue. The library-wide Picks / Rated / Rejected
  collections come from the catalogue. There is no sort, search or multi-select yet.
- Ratings, flags and edits cannot be rebuilt from the files. A catalogue reset loses
  them (the old file is kept), and backups are the next milestone (ADR 0018, 0019).
- A granted folder that is later moved is not followed; the user chooses it again.
- Recent folders cannot be removed from the list yet.
- Background intensity changes need a restart (thread pools are created at start-up).
