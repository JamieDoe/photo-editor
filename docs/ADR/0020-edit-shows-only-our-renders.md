# ADR 0020: The editor shows only our own renders, never the camera's JPEG

- Status: Accepted (supersedes ADR 0007 for the editor)
- Date: 2026-09-29

## Context

ADR 0007 showed the camera's embedded JPEG while a RAW decoded, so something appeared
within ~30 ms. In use, the switch to our render was jarring.
- **Different tone.** The camera applies its own tone curve and dynamic-range
  processing, and our default render has neither yet, so the photo flashed from
  punchy to flat.
- **Different framing.** Cameras correct lens distortion and crop slightly in their
  JPEG, so the picture also jumped in scale.
- **Wrong for edits.** For an edited photo the camera JPEG never shows the edit.

A fade would have blended two differently framed images.

## Decision (chosen by the product owner)

- **Our renders only.** `Engine::open` no longer extracts the embedded preview, and
  the open commands no longer stream it. The editor shows only renders of the RAW
  data, with the photo's saved edit.
- **While loading,** the current photo stays on screen, dimmed, with a small
  "Loading…" label, until the new photo's first render arrives: 116–345 ms on the
  M1 Max, depending on the camera.
- **One box per photo.** It is fitted by the photo's full size, so the quick
  interactive render and the sharper detail render occupy exactly the same box. The
  self-test checks this.
- `Engine::open_with_preview` and the decoders' `embedded_preview` remain. Library
  thumbnails use the embedded preview (ADR 0015), because they're small and fast to
  build.

## Consequences

- **No flash in the editor:** there is no change of tone, framing or edit state when
  a photo opens.
- **Slower first image:** it appears at about 0.1–0.35 s instead of 0.03 s. Stepping
  through photos shows the previous one dimmed for that time.
- **Thumbnails still use the camera's look** for unedited photos. Once our default
  look is closer to the camera's (a base tone curve, planned after backups), they
  will be consistent.
- A disk cache of our own rendered previews would make revisiting a photo instant,
  as Lightroom's standard previews do. It is noted as follow-up work.
