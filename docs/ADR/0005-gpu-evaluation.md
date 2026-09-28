# ADR 0005: GPU (wgpu) evaluation, CPU stays the production path for now

- Status: Accepted (Phase 0). Revisit in Phase 8 with low-end hardware data.
- Date: 2026-09-28

## Context

CLAUDE.md requires CPU and GPU rendering to coexist, with GPU never mandatory and no
GPU work adopted without benchmarks. `wgpu` is the preferred API to evaluate.

## What was built

`crates/gpu-spike` (excluded from the default build; the app does not link it):

- `GpuRenderer` implements the same `renderer::RenderBackend` trait as `CpuRenderer`
  and consumes the same `RenderPlan`: one fused WGSL compute pass (gains → contrast →
  saturation → sRGB encode), with `u16` source samples packed two per word.
- `ResidentSource` keeps a pyramid level in GPU memory across renders, the way a GPU
  preview path would during a slider drag.
- `tests/parity.rs`: GPU output matches CPU within 2 codes (measured max: 1) for
  identity and strong edits; the test skips cleanly when no adapter exists.
- `gpu-bench`: CPU vs GPU, with and without upload, at three pyramid levels.

## Findings (Apple M1 Max, Metal; full tables in PERFORMANCE.md)

1. **CPU and GPU coexist behind one abstraction.** No renderer or engine change was
   needed to add a GPU backend, and results agree within 1 code value.
2. **At interactive resolution the GPU has nothing to win.** A 1.5 MP preview costs
   ~2 ms on CPU, ~3.7 ms on GPU including upload and readback, and ~1.5 ms with a
   resident source. All are far below a 16 ms frame.
3. **Upload dominates GPU cost.** At 24 MP: ~55 ms with upload versus ~18 ms resident.
   Any GPU path must keep pyramid levels resident on the GPU.
4. **Readback is structural in a webview UI.** Frames must return to the CPU and cross
   IPC to be shown. Only a native presentation surface (wgpu surface composited
   with/under the webview) avoids it, and Tauri does not provide that out of the box.
5. **GPU latency is more stable under CPU contention.** Across runs on a loaded
   machine, CPU full-resolution renders varied ~2× while resident GPU times stayed
   within ~5%.
6. **Point operations are memory-bound.** The fused CPU kernel already runs near
   memory bandwidth. GPU advantages should be larger for neighbourhood work (blur,
   noise reduction, masks, local contrast), which does not exist yet.

## Decision

- The CPU backend remains the only production backend through Phase 1.
- `RenderBackend` + `RenderPlan` is the integration point for a future GPU backend. The
  spike demonstrates it is sufficient.
- GPU work resumes in Phase 8 (per PRODUCT.md), starting with neighbourhood stages and
  resident pyramid levels, and only where benchmarks on **low-end** hardware
  (Intel/AMD integrated GPUs, 4-core CPUs) show a benefit.
- Before then, investigate native presentation from Rust into the Tauri window
  (e.g. a wgpu surface on a child native view), since it decides whether GPU output
  can skip readback and IPC.

## Consequences

- No GPU driver, adapter or shader-compilation risk in the Phase 0/1 app.
- `wgpu` (MIT/Apache-2.0) stays out of the app's dependency tree and binary for now.
- The spike's code is disposable, but its benchmark harness and parity test should be
  kept and extended when GPU work resumes.
