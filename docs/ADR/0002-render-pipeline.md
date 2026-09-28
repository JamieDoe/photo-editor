# ADR 0002: Render pipeline — recipe, plan, backends, fused CPU execution

- Status: Accepted (Phase 0)
- Date: 2026-09-28

## Context

The renderer must support CPU and (optionally) GPU execution, multiple quality levels,
cancellation, and additional stages without rewrites. It must not allocate full-size
buffers per stage or re-process full resolution while sliders move.

## Decision

1. **Three layers:** `EditRecipe` (persisted, versioned) → `RenderPlan` (resolved,
   backend-agnostic stage list) → `RenderBackend` (executes). Backends never read the
   recipe; recipes never mention backends.
2. **Storage `u16`, processing `f32`.** Images are stored as scene-linear 16-bit RGB;
   each render converts row chunks to `f32` in reusable scratch buffers.
3. **Fused, chunked CPU execution.** All current stages are point operations, compiled
   into fused kernels (merged gains, LUT curves) and run per ~64K-pixel chunk in
   parallel. Only the output buffer is allocated per render.
4. **Multi-resolution previews from a reduced decode.** Previews use a pyramid built
   from a LibRaw half-size decode; full resolution is decoded only for export.
5. **Version and verify.** `RENDERER_VERSION` is part of every cache key; golden
   images catch unintended output changes.

## Consequences

- Stage cost is dominated by memory traffic, not arithmetic: on 1.5 MP, conversion +
  output encoding costs ~1.7 ms and all four adjustments add ~1.5 ms (PERFORMANCE.md).
- Neighbourhood operations will require splitting fused segments and tiled execution
  with aprons; the plan format already allows this.
- Detail previews are capped at half sensor resolution until a full-resolution viewer
  path exists.

## Alternatives considered

- *Stage-at-a-time full-frame buffers*: simplest, but N× memory traffic and allocation.
- *`f32` storage*: 2× memory for no precision benefit on sources.
- *A general node graph now*: premature; a linear plan covers Phase 0-4 needs and can
  evolve into a graph when masks/local adjustments need branching.
