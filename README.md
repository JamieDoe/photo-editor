# Photo Editor

A local-first, non-destructive RAW photo editor, currently at **Phase 1** (desktop
shell) on top of the Phase 0 rendering prototype. Product intent is in
[`docs/PRODUCT.md`](docs/PRODUCT.md). The system as built is described in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/RENDERING.md`](docs/RENDERING.md)
and [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md). Decisions are recorded in
[`docs/ADR/`](docs/ADR).

The UI is still intentionally minimal: the catalogue, thumbnails and full editing
tools come in later phases.

## Prerequisites (macOS)

```bash
brew install rustup libraw jpeg-turbo
rustup default stable
```

Homebrew installs rustup keg-only, so add `/opt/homebrew/opt/rustup/bin` to your
`PATH`. You also need Node.js 20+ and npm. Set `LIBRAW_DIR` if LibRaw lives somewhere
other than a Homebrew or system prefix (likewise `JPEG_TURBO_DIR` for libjpeg-turbo).
Building `export` with `--no-default-features` uses the pure-Rust JPEG encoder instead.

## Run the app

```bash
cd apps/desktop
npm install
npx tauri dev                    # development (Vite + debug Rust)
npx tauri build --bundles app    # release .app in target/release/bundle/macos/
```

The app opens in **Library**: choose a folder, browse its subfolders and photos, and
click a photo to open it in **Edit**. There you can adjust exposure, contrast,
temperature and saturation, and export a JPEG. **Settings** holds the theme, preview
memory, background processing and export quality.

Files the app keeps (macOS):

- Settings and window state: `~/Library/Application Support/dev.photoeditor.prototype/`
- Logs: `~/Library/Logs/dev.photoeditor.prototype/photo-editor.log`. Error messages
  show a reference such as `E-M2P8J-1` that matches a line in this file.

## Test

```bash
cargo test                                   # all Rust unit, integration and golden tests
cargo test -p gpu-spike --release            # CPU/GPU parity (skips without a GPU)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                             # licences/advisories (cargo install cargo-deny)
cd apps/desktop && npm test && npm run typecheck
```

End-to-end check through the real UI and IPC (opens, drags, exports, prints a JSON
report, then exits):

```bash
PE_SELF_TEST=/path/to/photo.nef target/release/desktop
```

## Benchmark

```bash
cargo run -p fixtures --release --bin gen-fixtures -- --large   # synthetic 24 MP files
cargo run -p bench --release -- --iterations 7                   # markdown + bench-results/*.json
cargo run -p gpu-spike --release --bin gpu-bench -- [file]       # CPU vs wgpu comparison
```

## Regenerate the TypeScript IPC types

```bash
cd apps/desktop && npm run bindings
```

## Layout

```text
apps/desktop/        Tauri 2 shell (src-tauri) + React/TypeScript UI (src)
crates/image-core    internal image types, pyramid, colour maths
crates/raw           Decoder trait: LibRaw (C shim) + JPEG
crates/renderer      EditRecipe -> RenderPlan -> CPU backend
crates/jobs          background job lanes, priority, supersession, cancellation
crates/cache         byte-budgeted LRU, render keys
crates/export        JPEG encode, safe atomic writes
crates/jpeg-turbo    libjpeg-turbo binding (encode + DCT-scaled decode)
crates/app-core      Engine: open / preview / export
crates/settings      typed, versioned settings and their store
crates/folders       folder listings and the folder access scope
crates/platform      OS-level helpers (atomic writes)
crates/fixtures      synthetic copyright-free test images
crates/bench         benchmark harness
crates/gpu-spike     wgpu evaluation (not used by the app)
tests/fixtures/      synthetic fixtures, golden images, local (ignored) photos
```

## Third-party notices

LibRaw is used under LGPL-2.1 (dynamically linked; see ADR 0003). libjpeg-turbo is used
under its BSD-3-Clause/IJG/zlib licences (dynamically linked; see ADR 0006). This
software is based in part on the work of the Independent JPEG Group.
