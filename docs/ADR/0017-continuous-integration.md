# ADR 0017: Continuous integration on GitHub Actions

- Status: Accepted
- Date: 2026-09-29

## Context

Until now every check (format, clippy, tests, `cargo deny`, frontend) was run by hand
on one Mac. The product targets macOS, Windows and Linux. Nothing verified that the
core builds there, or stopped a pull request that breaks a check.

## Decision

`.github/workflows/ci.yml` runs on pushes to `main` and on every pull request:

| Job | Runner | Checks |
|---|---|---|
| macOS (full product) | macos-15 (Apple Silicon), LibRaw and libjpeg-turbo from Homebrew | frontend build; `cargo fmt --check`; clippy `-D warnings` for the whole workspace and the no-native-library configurations; all tests; the JPEG-only engine tests; generated TypeScript bindings are current |
| Core on Linux / Windows | ubuntu-24.04, windows-2025 | clippy and tests of the portable crates with `--no-default-features` (no LibRaw, no libjpeg-turbo) |
| Frontend | ubuntu-24.04 | `npm ci`, typecheck, vitest |
| Licences and advisories | ubuntu-24.04 | `cargo deny check` against `deny.toml` |

- **Pinned toolchain.** `rust-toolchain.toml` pins Rust 1.98.1 for local builds and
  CI alike, since clippy's lints change between releases. Upgrading is a deliberate
  one-line change.
- **Pinned actions.** Third-party actions are pinned to commit SHAs, with the release
  in a comment. The workflow token is read-only.
- **Why the full product is macOS only for now:**
  - Ubuntu 24.04 ships libjpeg-turbo 2.1, and our shim uses the TurboJPEG 3 API.
  - Windows has known LibRaw gaps: the DLL isn't bundled, and paths use narrow
    characters (ARCHITECTURE.md, limitations).
  - Building those libraries in CI is follow-up work, for when Windows and Linux
    builds of the app are due. Until then, the portable job still catches
    platform-specific breakage in the catalogue, jobs, cache, renderer and folder
    code.
- **Camera fixtures:** the real RAW files are git-ignored, so the tests that use them
  skip in CI. Those tests and the in-app self-test (it needs a visible window) remain
  local checks.

## Consequences

- A pull request shows whether it keeps every automated check green on the product's
  platforms.
- macOS minutes cost more than Linux minutes. The full-product job is the one
  expensive job, and caching (`Swatinem/rust-cache`) keeps reruns short.
