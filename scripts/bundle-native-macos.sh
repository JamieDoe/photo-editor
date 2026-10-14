#!/bin/sh
# The app binary readied for its bundle (ADR 0086); `npm run bundle` has Tauri run it
# before bundling (tauri.bundle.conf.json's beforeBundleCommand).
#
# The binary was linked against the libraries scripts/build-native-macos.sh built, by
# their paths in target/native/macos-arm64. It is changed to load them by name from the
# app's Frameworks folder (@rpath, which Tauri points there), where the bundle puts the
# copies that script readied in target/native/bundle. Then it checks that the binary and
# the libraries load nothing from outside the system and the app and need no newer macOS
# than the app's oldest, and signs the binary again (ad hoc; release signing comes
# later).
#
# Usage: scripts/bundle-native-macos.sh [app binary]   (default target/release/desktop)

set -eu

[ "$(uname)" = Darwin ] || exit 0

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX="$ROOT/target/native/macos-arm64"
STAGE="$ROOT/target/native/bundle"
BIN="${1:-$ROOT/target/release/desktop}"
# Keep in step with scripts/build-native-macos.sh and bundle.macOS.minimumSystemVersion.
MIN_MACOS=13.0

fail() {
  echo "bundle-native-macos: $*" >&2
  exit 1
}

# The libraries a file loads: the paths otool lists after the file's own line.
loads() {
  otool -L "$1" | tail -n +2 | awk '{print $1}'
}

[ -f "$BIN" ] || fail "no app binary at $BIN"
[ -f "$STAGE/libraw_r.25.dylib" ] || fail "the libraries aren't readied: run scripts/build-native-macos.sh"
if loads "$BIN" | grep -q -E '^/(opt/homebrew|usr/local)/'; then
  fail "the app links Homebrew's libraries, which can't be shipped. Run scripts/build-native-macos.sh, then build again."
fi

for dep in $(loads "$BIN" | grep "^$PREFIX/" || true); do
  install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$BIN" 2>/dev/null
done
otool -l "$BIN" | grep -q '@executable_path/../Frameworks' ||
  install_name_tool -add_rpath '@executable_path/../Frameworks' "$BIN" 2>/dev/null

# Nothing from outside the system and the app; nothing newer than the oldest macOS.
for f in "$BIN" "$STAGE"/*.dylib; do
  outside=$(loads "$f" | grep -v -E '^(/usr/lib/|/System/Library/|@rpath/)' || true)
  [ -z "$outside" ] || fail "$(basename "$f") loads $outside"
  minos=$(otool -l "$f" | awk '/LC_BUILD_VERSION/ { found = 1 } found && /minos/ { print $2; exit }')
  awk -v a="$minos" -v b="$MIN_MACOS" 'BEGIN {
    split(a, x, "."); split(b, y, ".")
    exit !(x[1] < y[1] || (x[1] == y[1] && x[2] <= y[2]))
  }' || fail "$(basename "$f") needs macOS $minos; the app supports $MIN_MACOS"
done

# Changing its load commands invalidated its signature.
codesign --force --sign - "$BIN" 2>/dev/null
echo "app binary ready to bundle with $STAGE"
