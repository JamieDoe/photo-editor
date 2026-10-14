#!/bin/sh
# Builds the native libraries the app ships (ADR 0086) from pinned sources, for the
# app's minimum macOS, into target/native/macos-arm64, and readies them for the app
# bundle in target/native/bundle (see `stage`):
#
#   libjpeg-turbo 3.2.0 (BSD-style/IJG/zlib): libturbojpeg for the app's JPEG encoding,
#                                              libjpeg for LibRaw's lossy DNG support
#   LLVM OpenMP 21.1.8 (Apache-2.0 with LLVM    libomp, which LibRaw decodes in parallel with
#   exception):                                 (Apple's compiler ships no OpenMP runtime)
#   LibRaw 0.22.2 (LGPL-2.1 or CDDL-1.0):      libraw_r, the RAW decoder (ADR 0003),
#                                              linked dynamically (LGPL route)
#
# LibRaw is built without Little CMS (the app does its own colour management), from its
# plain makefile. Homebrew's builds
# can't be shipped: they target the macOS they were built on and need Homebrew's paths.
#
# Needs: curl, shasum, tar, make, the Xcode command line tools (clang), and cmake.
# Usage: scripts/build-native-macos.sh        (does nothing if already built)

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/target/native-src"
PREFIX="$ROOT/target/native/macos-arm64"
# Keep in step with bundle.macOS.minimumSystemVersion and .cargo/config.toml.
MIN_MACOS=13.0
ARCH=arm64

LIBRAW=LibRaw-0.22.2
LIBRAW_URL=https://www.libraw.org/data/$LIBRAW.tar.gz
LIBRAW_SHA256=de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa
TURBO=libjpeg-turbo-3.2.0
TURBO_URL=https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/3.2.0/$TURBO.tar.gz
TURBO_SHA256=6f30092cef9fb839779646608f4ee14ae3cbac989c47fa05e841b0841f09878e
LLVM_VERSION=21.1.8
LLVM_RELEASE=https://github.com/llvm/llvm-project/releases/download/llvmorg-$LLVM_VERSION
OPENMP_SHA256=856b023748b41ac7b2c83fd8e9f765ff48a4df2fe6777d2811ef7c7ed8f2f977
LLVM_CMAKE_SHA256=85735f20fd8c81ecb0a09abb0c267018475420e93b65050cc5b7634eab744de9

STAGE="$ROOT/target/native/bundle"
LIBS="libraw_r.25.dylib libjpeg.62.dylib libturbojpeg.0.dylib libomp.dylib"

# The libraries as the app bundle carries them, in target/native/bundle: named to load
# from its Frameworks folder (@rpath, which the app binary points there), and signed.
# They exist before the app is compiled, as Tauri copies them then.
stage() {
  rm -rf "$STAGE"
  mkdir -p "$STAGE"
  for lib in $LIBS; do
    cp -L "$PREFIX/lib/$lib" "$STAGE/$lib"
    chmod u+w "$STAGE/$lib"
    install_name_tool -id "@rpath/$lib" "$STAGE/$lib" 2>/dev/null
    for dep in $(otool -L "$STAGE/$lib" | tail -n +2 | awk '{print $1}' | grep "^$PREFIX/" || true); do
      install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$STAGE/$lib" 2>/dev/null
    done
  done
  codesign --force --sign - "$STAGE"/*.dylib 2>/dev/null
}

if [ -f "$PREFIX/lib/libraw_r.25.dylib" ] && [ -f "$PREFIX/lib/libomp.dylib" ]; then
  [ -f "$STAGE/libraw_r.25.dylib" ] || stage
  echo "native libraries already built in $PREFIX"
  exit 0
fi
command -v cmake >/dev/null || { echo "cmake is needed: brew install cmake" >&2; exit 1; }

mkdir -p "$SRC" "$PREFIX"

# Downloads $2 to $SRC (as the URL names it) unless there, checks its SHA-256 ($3),
# and unpacks it into $SRC/$1.
fetch() {
  archive="$SRC/$(basename "$2")"
  [ -f "$archive" ] || curl -sSfL --max-time 300 -o "$archive" "$2"
  echo "$3  $archive" | shasum -a 256 -c - >/dev/null || {
    echo "checksum mismatch for $archive" >&2
    exit 1
  }
  rm -rf "${SRC:?}/$1"
  mkdir -p "$SRC/$1"
  tar xf "$archive" -C "$SRC/$1" --strip-components 1
}

FLAGS="-O2 -arch $ARCH -mmacosx-version-min=$MIN_MACOS"

fetch "$TURBO" "$TURBO_URL" "$TURBO_SHA256"
cmake -S "$SRC/$TURBO" -B "$SRC/$TURBO/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DCMAKE_INSTALL_LIBDIR=lib \
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MIN_MACOS" \
  -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
  -DCMAKE_INSTALL_NAME_DIR="$PREFIX/lib" \
  -DENABLE_STATIC=OFF \
  -DWITH_TURBOJPEG=ON >/dev/null
cmake --build "$SRC/$TURBO/build" --parallel >/dev/null
cmake --install "$SRC/$TURBO/build" >/dev/null

# OpenMP's runtime, built on its own: its sources beside LLVM's build modules, as in
# LLVM's tree.
fetch llvm/openmp "$LLVM_RELEASE/openmp-$LLVM_VERSION.src.tar.xz" "$OPENMP_SHA256"
fetch llvm/cmake "$LLVM_RELEASE/cmake-$LLVM_VERSION.src.tar.xz" "$LLVM_CMAKE_SHA256"
cmake -S "$SRC/llvm/openmp" -B "$SRC/llvm/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DCMAKE_INSTALL_LIBDIR=lib \
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MIN_MACOS" \
  -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
  -DCMAKE_INSTALL_NAME_DIR="$PREFIX/lib" \
  -DLIBOMP_INSTALL_ALIASES=OFF \
  -DOPENMP_ENABLE_LIBOMPTARGET=OFF \
  -DLIBOMP_OMPD_SUPPORT=OFF \
  -DOPENMP_ENABLE_OMPT_TOOLS=OFF >/dev/null
cmake --build "$SRC/llvm/build" --parallel >/dev/null
cmake --install "$SRC/llvm/build" >/dev/null

fetch "$LIBRAW" "$LIBRAW_URL" "$LIBRAW_SHA256"
# LibRaw's plain makefile (its configure script wants pkg-config): the thread-safe
# library's objects, with OpenMP, zlib (deflate DNG) and libjpeg (lossy DNG), linked
# here into a shared library named as LibRaw's own build names it. Apple's compiler
# takes OpenMP through -Xclang, with the runtime built above.
(
  cd "$SRC/$LIBRAW"
  make -f Makefile.dist -j"$(sysctl -n hw.ncpu)" lib/libraw_r.a \
    CC=clang CXX=clang++ \
    CFLAGS="$FLAGS -I. -w -Xclang -fopenmp -DUSE_ZLIB -DUSE_JPEG -I$PREFIX/include" >/dev/null
  clang++ -dynamiclib -arch $ARCH -mmacosx-version-min=$MIN_MACOS \
    -o "$PREFIX/lib/libraw_r.25.dylib" \
    -Wl,-force_load,lib/libraw_r.a \
    -L"$PREFIX/lib" -ljpeg -lomp -lz \
    -install_name "$PREFIX/lib/libraw_r.25.dylib" \
    -compatibility_version 26 -current_version 26
  ln -sf libraw_r.25.dylib "$PREFIX/lib/libraw_r.dylib"
  mkdir -p "$PREFIX/include/libraw"
  cp libraw/*.h "$PREFIX/include/libraw/"
)

# The licences, for checking against apps/desktop/src-tauri/licenses (which the app
# ships; LGPL-2.1 asks for its notice) when a version changes.
mkdir -p "$PREFIX/licenses"
cp "$SRC/$LIBRAW/LICENSE.LGPL" "$SRC/$LIBRAW/LICENSE.CDDL" "$SRC/$LIBRAW/COPYRIGHT" "$PREFIX/licenses/" 2>/dev/null || true
cp "$SRC/$TURBO/LICENSE.md" "$PREFIX/licenses/libjpeg-turbo-LICENSE.md"
cp "$SRC/llvm/openmp/LICENSE.TXT" "$PREFIX/licenses/LLVM-OpenMP-LICENSE.TXT"
stage
# Their build scripts chose a library when they last ran: run them again.
(cd "$ROOT" && cargo clean -q -p raw -p jpeg-turbo) || true
echo "built into $PREFIX"
