# ADR 0003: LibRaw as the initial RAW decoder, behind a trait, via a C shim

- Status: Accepted for Phase 0; licensing/distribution to be confirmed before release
- Date: 2026-09-28

## Context

RAW support must sit behind an abstraction. LibRaw is the specified first candidate.
The application core is proprietary, so licensing of native libraries must be explicit.

## Decision

1. **`raw::Decoder` trait + `DecoderRegistry`.** Decoders produce `LinearImage` +
   `SourceInfo`. Two implementations: `LibRawDecoder` (camera RAW) and `JpegDecoder`
   (zune-jpeg, pure Rust).
2. **C shim (`crates/raw/shim/pe_libraw.c`).** Rust never touches `libraw_data_t`
   (its layout changes between LibRaw versions). The shim exposes plain structs and
   five functions. No bindgen dependency.
3. **Output contract:** 16-bit linear, sRGB primaries, as-shot white balance,
   `no_auto_bright`, highlight clip. Tone and display encoding are ours.
4. **Preview decodes use `half_size`** when the half-resolution result still has a
   long edge ≥ 1600 px.
5. **Cancellation** via LibRaw's progress callback (checked at stage boundaries).
6. **Thread budget:** `DecodeOptions::max_threads` caps LibRaw's OpenMP regions via
   `omp_set_num_threads`, resolved with `dlsym` at runtime (no link-time OpenMP
   dependency; a no-op for non-OpenMP builds). Windows is not implemented yet.
7. **Dynamic linking to the thread-safe `libraw_r`**, found via `LIBRAW_DIR` or common
   prefixes. LibRaw is a default cargo feature; building without it leaves JPEG-only.

## Licensing

LibRaw 0.22 is dual-licensed **LGPL-2.1 OR CDDL-1.0**. For a proprietary app:

- LGPL-2.1 is compatible when LibRaw is dynamically linked, users can replace the
  library, and the licence/notice is shipped. The current build links dynamically.
- CDDL-1.0 would also permit static linking (file-level copyleft: modifications to
  LibRaw files must be published).
- Homebrew's LibRaw links `libomp` (Apache-2.0 with LLVM exception), `little-cms2`
  (MIT) and `jpeg-turbo` (IJG/BSD/zlib). A distributable build should bundle a LibRaw
  we compile ourselves with a known feature set.

**Action before distribution:** legal confirmation of the licence route, and a
bundled LibRaw build per platform (macOS `.app` Frameworks, Windows DLL).

Rust dependencies are checked with `cargo deny check` (`deny.toml`: permissive
licences only). Tauri itself brings in five MPL-2.0 crates (`cssparser`,
`cssparser-macros`, `selectors`, `dtoa-short`, `option-ext`). They are allowed as
named exceptions pending the same legal review; any new MPL/GPL dependency fails the
check. Bundled npm runtime dependencies are MIT/Apache-2.0.

Other new dependencies: `zune-jpeg` (MIT/Apache-2.0/Zlib), `jpeg-encoder`
((MIT OR Apache-2.0) AND IJG, which requires the IJG acknowledgement in docs),
`rayon`, `serde`, `serde_json` (MIT/Apache-2.0), `ts-rs` (MIT), `cc` (build only).

## Measured consequences (PERFORMANCE.md)

- All six CC0 camera samples (Nikon Z6 NEF, Canon R6 CR3, Sony A7 III ARW, Sony A7R IV
  61 MP ARW, Fuji X-T3 RAF, Ricoh GR III DNG) decode correctly.
- Decode dominates open/export. Unpack is single-threaded for Huffman/lossless-JPEG
  formats (NEF, DNG), and X-Trans full demosaic is slow (~2.3 s).
- A full LibRaw decode peaks at 17–25 bytes/pixel of working memory, which bounds how
  low export memory can go while LibRaw does the demosaic.

## Alternatives / follow-ups

- **RawSpeed** (LGPL-2.1, C++): faster unpack for many formats; evaluate behind the same
  trait with the benchmark harness.
- **rawler** (pure Rust, LGPL-2.1): removes the C toolchain; evaluate coverage/speed.
- ~~Embedded preview extraction for an instant first frame~~: done, see ADR 0007.
