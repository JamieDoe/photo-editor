# ADR 0086: The native libraries the app ships (macOS)

- Status: Accepted
- Date: 2026-10-14

## Context

The app linked LibRaw and libjpeg-turbo from Homebrew (`/opt/homebrew`), and Homebrew's
LibRaw also loads Homebrew's OpenMP runtime, libjpeg and Little CMS. That works on the
development Mac only:
- **On another Mac:** without those Homebrew packages, the app doesn't start.
- **Homebrew's builds target the macOS they were built on:** here macOS 26 (`minos
  26.0`), Apple Silicon only, although the app's own code runs on macOS 11 and later.

ADR 0003 left "a bundled LibRaw build per platform" and the licence route to settle
before distribution.

## Decision

1. **Oldest macOS 13 (Ventura), Apple Silicon** for the first release.
   - It covers nearly every Mac sold since 2020 on a supported macOS.
   - Intel would need a universal build, an assembler for libjpeg-turbo's Intel code,
     and an Intel Mac to test on; it can follow.
   - Subject and People masks already need macOS 14, checked at runtime (ADR 0074).
   - It is set in three places kept in step: `MACOSX_DEPLOYMENT_TARGET` (`.cargo/
     config.toml`), the native build script, and `bundle.macOS.minimumSystemVersion`.
2. **The libraries are built from pinned sources** (`scripts/build-native-macos.sh`,
   archives checked by SHA-256) for that target, into `target/native/macos-arm64`:

   | Library | Version | Licence | For |
   |---|---|---|---|
   | LibRaw (`libraw_r`) | 0.22.2 | LGPL-2.1 or CDDL-1.0 | RAW decoding (ADR 0003) |
   | libjpeg-turbo (`libturbojpeg`, `libjpeg`) | 3.2.0 | IJG, BSD-3-Clause, zlib | the app's JPEG encoding; LibRaw's lossy DNG |
   | LLVM OpenMP (`libomp`) | 21.1.8 | Apache-2.0 with LLVM exception | LibRaw's parallel decoding |

   - **OpenMP:** without it, decoding was up to five times slower (a Fuji X-T3 opened
     in 0.9 s instead of 0.19 s, and decoded in full in 11–13 s instead of 2–4 s).
     Apple's compiler ships no OpenMP runtime, so it is built too; LibRaw is compiled
     with `-Xclang -fopenmp` against it.
   - **Left out:** Little CMS (the app does its own colour management) and Jasper.
   - **LibRaw's plain makefile:** its configure script wants pkg-config, so the script
     compiles LibRaw's thread-safe objects and links them into `libraw_r.25.dylib`
     itself.
   - **Development keeps working:** the build scripts look in that folder first
     (`LIBRAW_DIR`, `JPEG_TURBO_DIR` in `.cargo/config.toml`) and fall back to Homebrew
     while it isn't built, so development and CI work either way. Building it clears
     the two crates that link them, so the next build picks them up.
3. **Linked dynamically, under the LGPL** (ADR 0003): LibRaw stays a separate,
   replaceable library in the app's `Frameworks` folder. The licence texts ship in
   `Resources/licenses` (`apps/desktop/src-tauri/licenses`).
4. **Bundling** (`npm run bundle`): the native build, then `tauri build` with
   `tauri.bundle.conf.json` merged in.
   - **The copies:** the native build readies copies in `target/native/bundle`, named
     to load from the app (`@rpath`), which Tauri puts in `Contents/Frameworks`.
   - **The binary:** before bundling, `scripts/bundle-native-macos.sh` changes it to
     load them by those names.
   - **Checks:** it refuses Homebrew paths, anything from outside the system and the
     app, and anything built for a newer macOS than 13.
   - **Why an overlay:** Tauri copies the listed libraries whenever the app compiles,
     so listing them in `tauri.conf.json` would make every build, and CI's clippy,
     need them.
   - **Signing:** the bundle is signed ad hoc (`signingIdentity: "-"`) without the
     hardened runtime. That runtime only loads libraries signed by the same team, and
     ad-hoc signatures have none. Developer ID signing and notarisation (with the
     hardened runtime) replace this once there is an Apple Developer account.
5. **The plain `tauri build`** stays a build for this Mac, for development and the
   self-test.

## How it was checked

- **Nothing loads from outside the app:** with `target/native` moved aside, the bundled
  app loaded all four libraries from its `Frameworks` folder (`DYLD_PRINT_LIBRARIES`)
  and passed the full self-test on the Nikon Z 6 NEF.
- **The Fujifilm X-T3 RAF:** it fails only the self-test's zoom check, whose half-size
  window is one pixel wider for that camera's size; the build before this change fails
  it the same way.
- **The bundle verifies** with `codesign --verify --deep --strict`.
- **Decoding** is as fast or faster than with Homebrew's LibRaw (PERFORMANCE §62).

## Consequences

- `npm run bundle` makes an app that runs on any Apple Silicon Mac with macOS 13 or
  later. Downloaded, it still needs "Open Anyway" in System Settings until it is
  notarised.
- **Still before release:**
  - legal confirmation of the LGPL route (ADR 0003);
  - Developer ID signing and notarisation;
  - the product's name and identifier.

  The third-party notices page, also listed here at first, is done (ADR 0087).
- **Windows** needs its own build of the same libraries (DLLs beside the executable),
  as part of the Windows milestone.
