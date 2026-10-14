# ADR 0087: Third-party notices in the app

- Status: Accepted
- Date: 2026-10-14

## Context

The app is built from about 420 third-party libraries:
- three native libraries (ADR 0086);
- 269 Rust crates;
- the Rust standard library and the crates it is built from;
- four JavaScript packages and two fonts.

Nearly all are under licences (MIT, BSD, Apache-2.0, Unicode, OFL, MPL, LGPL) that
require their copyright notices and licence texts to go with every copy. LibRaw's LGPL
also asks that people be told they may replace it. ADR 0003 and ADR 0086 left "a
third-party notices page in the app" to do before release. `cargo deny` already holds
the Rust crates to the licence policy, but nothing collected the texts, and the
JavaScript packages had no check.

## Decision

1. **A generated list.** `scripts/third-party-notices.mjs` (Node, no dependencies)
   writes `third-party-notices.json`: groups of libraries, each with its version,
   declared licence, source address and licence texts. Identical texts are stored once,
   which brings about 2.2 MB of texts down to about 0.8 MB.
   - **Rust crates:** the `desktop` crate's normal dependencies for macOS on Apple
     Silicon, followed through `cargo metadata`. Build and dev dependencies aren't
     shipped, so they're left out.
   - **Each crate's own licence files.** For the 20 crates that ship none, the
     standard text of the licence they declare is used (`scripts/licence-texts`): MIT
     with a copyright line naming the crate's authors, or MPL-2.0. `alloc-stdlib` takes
     the text of `alloc-no-stdlib`, which comes from the same repository.
   - **The Rust standard library:** read from `COPYRIGHT-library.html`, the notices the
     Rust toolchain ships for it, covering its own files and the crates it is built
     from.
   - **JavaScript packages and fonts:** those `package-lock.json` doesn't mark as
     development only. Their licences must be in an allow list (MIT, ISC, Apache-2.0,
     BSD, 0BSD, Zlib, OFL-1.1), or the script fails, as `cargo deny` does for crates.
   - **Native libraries:** the versions come from `scripts/build-native-macos.sh`, and
     the texts from `apps/desktop/src-tauri/licenses`, which the bundle also carries.
     LibRaw's entry explains that it is a separate, replaceable library and how to sign
     the app again after replacing it.
   - **The script fails** when a library has no licence text.
2. **Written on every build, not committed.** `npm run dev` and `npm run build` write it
   to `apps/desktop/public/` (git-ignored), and Vite puts it in the web assets.
   - Every build, the bundle included, carries the current list; nothing can be
     forgotten or fall out of date.
   - It runs in half a second.
3. **In the app: Settings → About.** The About group shows the version and a
   "Third-party notices" row.
   - **The notices screen:** a filter and the groups, each library a row (name,
     version, licence) that opens to its note, source address and texts.
   - **Loading:** the list is fetched only when the screen opens. Texts are only laid
     out for opened rows.
   - **Without a list:** the screen says the notices are written when the app is built.
4. **Checked by the self-test:**
   - there are more than 100 libraries;
   - LibRaw has its note;
   - every library has non-empty texts.

## Consequences

- Bundled apps carry complete notices.
- A new dependency appears in the list automatically.
- A JavaScript package under a licence that isn't allowed stops the build.
- **For the legal review (ADR 0003):**
  - whether pointing to LibRaw's published source archive is enough for LGPL-2.1 §6,
    or whether the source must be offered from our own download location;
  - whether the MPL-2.0 crates' notices (source addresses) meet MPL §3.2.
- **Windows** needs its own crate list (`--filter-platform` for the Windows target) and
  its native libraries, as part of the Windows milestone.
