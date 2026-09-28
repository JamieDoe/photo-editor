# ADR 0001: Workspace boundaries and typed IPC

- Status: Accepted (Phase 0)
- Date: 2026-09-28

## Context

Phase 0 validates Tauri ↔ Rust communication and the rendering foundation. The product
requires that the UI never processes images, that IPC is deliberate and typed, and that
domain models are not duplicated between Rust and TypeScript.

## Decision

1. **Cargo workspace of small crates** with one-way dependencies:
   `image-core` ← `raw`, `renderer`, `jobs`, `cache`, `export` ← `app-core` ← `desktop`.
   The desktop crate is a thin adapter; `app-core` has no Tauri dependency, so the
   benchmark harness and tests drive exactly the same engine as the app.
2. **Engine API returns job handles.** Every engine operation is non-blocking and
   returns `JobHandle`; the Tauri layer awaits handles on Tauri's blocking pool.
3. **Generated TypeScript types.** IPC payloads derive `ts_rs::TS`; bindings are
   generated into `apps/desktop/src/ipc/generated/` and committed. `EditRecipe` and
   `AdjustmentSpec` come from the renderer crate (feature `ts`), so slider ranges and
   the recipe shape have a single source of truth.
4. **Binary preview frames.** `render_preview` returns raw bytes (20-byte header +
   RGBA8). JSON encoding of multi-megabyte pixel arrays is avoided entirely.
5. **Native dialogs are opened from Rust.** The webview gets only `core:default`.

## Consequences

- Adding an IPC field is a Rust change + `npm run bindings`; TypeScript fails to
  compile if the UI is out of date.
- The frame header is the one hand-mirrored layout (Rust ↔ `frame.ts`); it is covered
  by TS unit tests and the end-to-end self-test.
- Measured IPC round trip for a 1.5 MP RGBA frame is ~5 ms including render
  (PERFORMANCE.md), so the webview is not a bottleneck at interactive resolution.

## Alternatives considered

- *tauri-specta*: typed commands as well as types, but pre-1.0 and larger; ts-rs is
  sufficient while the command set is small.
- *JPEG/PNG-encoded frames*: smaller transfers but adds encode/decode latency and
  lossy artefacts to every interactive frame; revisit only if IPC bandwidth becomes
  the bottleneck on Windows (WebView2) measurements.
- *Custom URI protocol serving frames*: viable later for `<img>`-based display and
  browser caching; unnecessary now.
