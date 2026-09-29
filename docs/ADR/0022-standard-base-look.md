# ADR 0022: A camera-like Standard base look, fitted to real cameras

- Status: Accepted (Phase 3, milestone 1)
- Date: 2026-09-29

## Context

Our unedited render applied no tone curve: LibRaw's scene-linear output went straight
through the sRGB encoding. Next to a camera's own JPEG it looked dark and flat. Once
the editor stopped showing the camera's JPEG (ADR 0020), that gap was the first
thing a photographer saw. A raw converter is expected to start from a pleasing
default, as Lightroom's default profile or a camera's picture style does.

Constraints:

- **Existing edits keep their look.** CLAUDE.md §13: rendering changes must not
  silently change existing edits.
- **Measure, don't guess.** Tuning by eye on one monitor and one photo would not
  generalise.

## Decision

1. **Measure.** `bench --look` compares, per camera file, the luminance distribution
   of our render with the camera's embedded JPEG, percentile by percentile. The
   framing differs slightly (cameras correct lens distortion in their JPEG), so the
   comparison is not pixel for pixel. Each percentile pair samples the camera's tone
   curve relative to ours.
2. **Fit one curve** to all six sample cameras (Canon R6, Fuji X-T3, Nikon Z 6,
   Ricoh GR III, Sony A7 III and A7R IV), in sRGB units:
   - a 1.25 EV lift, then a log-logistic S-curve (slope 1.65, pivot 0.55), normalised
     so sensor white stays white;
   - it keeps a toe (deep shadows stay deep) and a shoulder (highlights roll off
     instead of clipping).
3. **Where it sits:** `Stage::BaseCurve`, a per-channel point operation, runs after
   white balance, exposure and contrast (all scene-referred) and before saturation.
   Exposure and contrast pushes therefore still roll off through the shoulder.
   - The CPU backend runs it as a lookup table.
   - The GPU spike implements it in WGSL, and the parity test passes.
4. **Recipe version 2** adds `look: standard | flat`, defaulting to `standard`.
   - Version-1 recipes (every edit saved before) migrate to `flat` and render
     exactly as before. The flat-look golden images are unchanged to the byte.
   - A recipe is an identity (no saved edit) only with the default look.
   - Edit shows **Look: Standard / Flat** at the top of the panel, so an old edit can
     move to Standard without losing its adjustments.
5. **`RENDERER_VERSION` becomes 2,** so cached previews and rendered thumbnails
   refresh.

## Measurements (`bench --look`, M1 Max)

Tonal difference from each camera's JPEG (RMS over 11 percentiles, sRGB units; 0 means
identical tones):

| Camera | Flat (before) | Standard |
|---|---|---|
| Canon EOS R6 | 0.207 | 0.072 |
| Fujifilm X-T3 | 0.094 | 0.083 |
| Nikon Z 6 | 0.204 | 0.046 |
| Ricoh GR III | 0.105 | 0.052 |
| Sony A7 III | 0.176 | 0.018 |
| Sony A7R IV | 0.023 | 0.045 |
| **Mean** | **0.135** | **0.053** |

- The midtone brightness gap went from +0.3–1.6 EV to within ±0.7 EV of every camera.
- The A7R IV sample is mostly clipped sky, which is already equal in both. There the
  gentler shoulder costs a little.

## Consequences

- Unedited photos look close to the camera's JPEG in tone. Library thumbnails of
  unedited photos, which come from embedded JPEGs, now nearly match Edit.
- **Still to do:**
  - One curve for every camera. Brands differ by about ±0.7 EV at midtones; per-camera
    baseline exposure or profiles would close that.
  - Camera JPEGs remain slightly more saturated; that belongs to colour profiles.
  - Framing differences need lens correction (Phase 5).
- Changing the Standard look later means a new look (such as `standard2`), never
  editing this one, so edits keep their appearance.
