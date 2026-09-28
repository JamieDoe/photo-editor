# ADR 0006: libjpeg-turbo for JPEG export, pure-Rust encoder as fallback

- Status: Accepted
- Date: 2026-09-28

## Context

Phase 0 measurements showed JPEG encoding at 20–30% of export time: 160–515 ms at
20–61 MP with the pure-Rust `jpeg-encoder`, which is single-threaded and not
SIMD-optimised.

## Decision

- `export::encode` uses **libjpeg-turbo** (TurboJPEG 3 API) when the `turbojpeg` cargo
  feature is on (default). The binding lives in a small shared crate,
  `crates/jpeg-turbo`, whose C shim takes its constants from the real `turbojpeg.h`,
  so Rust hard-codes no enum values. The same crate provides DCT-scaled decoding for
  embedded RAW previews (ADR 0007).
- `jpeg-encoder` remains as the **portable fallback** (feature off) and as a reference.
  `export::encode_with(image, format, JpegEncoder)` selects explicitly for benchmarks
  and parity tests.
- Output settings are unchanged: quality 92, 4:4:4 chroma, standard Huffman tables.
- The library is linked dynamically and found via `JPEG_TURBO_DIR` or the Homebrew and
  system prefixes, the same mechanism as LibRaw (ADR 0003).

## Measurements (`bench`, M1 Max; full resolution, q92 4:4:4)

| Source | jpeg-encoder | libjpeg-turbo | Speed-up | File size |
|---|---|---|---|---|
| Nikon Z 6, 24.5 MP | 194.5 ms | 80.6 ms | 2.4× | identical |
| Sony A7 III, 24.2 MP | 262.1 ms | 108.5 ms | 2.4× | identical |
| Sony A7R IV, 61 MP | 515.3 ms | 206.9 ms | 2.5× | identical |
| synthetic 24 MP | 159.4 ms | 60.5 ms | 2.6× | identical |

Fidelity: both encoders exceed 38 dB PSNR against the source in
`all_encoders_round_trip_the_chart_with_similar_fidelity`, for RGB and RGBA input.

## Licensing and distribution

- libjpeg-turbo: BSD-3-Clause (TurboJPEG API), IJG and zlib licences, all permissive.
  The IJG notice is already in the README. The same library is already a transitive
  dependency of Homebrew's LibRaw.
- Distribution needs a bundled `libturbojpeg` per platform, alongside LibRaw. Builds
  without it (feature off) fall back automatically.

## Consequences

- Encoding is no longer a significant share of export for Bayer RAWs; decode dominates
  (PERFORMANCE.md).
- There is one more native library to bundle and keep patched.
- libjpeg-turbo's DCT-scaled decoding is now used for embedded previews (ADR 0007).
- `app-core`'s `turbojpeg` feature controls both `raw` and `export`. Engine info reports
  which encoder and decoder are active, so a build that silently falls back is visible.

## Alternatives considered

- *Parallel strip encoding with `jpeg-encoder`* (restart markers + stitched entropy
  segments): pure Rust, but depends on encoder internals and is fragile. It could be
  layered on libjpeg-turbo later if export encode matters again.
- *mozjpeg*: better compression, but slower; export speed was the goal.
