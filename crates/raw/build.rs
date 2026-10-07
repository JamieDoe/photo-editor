//! Compiles the LibRaw shim (C, plus a C++ LibRaw subclass) and links the thread-safe
//! LibRaw (`libraw_r`).
//!
//! LibRaw is located via `LIBRAW_DIR` (containing `include/` and `lib/`), falling back
//! to common Homebrew/system prefixes. Linking is dynamic; see ADR 0003.

#[cfg(feature = "libraw")]
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=LIBRAW_DIR");
    println!("cargo:rerun-if-changed=shim/pe_libraw.c");
    println!("cargo:rerun-if-changed=shim/pe_libraw.h");
    println!("cargo:rerun-if-changed=shim/pe_libraw_xtrans.cpp");
    println!("cargo:rerun-if-changed=shim/pe_libraw_xtrans.h");
    #[cfg(feature = "libraw")]
    build_shim();
}

#[cfg(feature = "libraw")]
fn build_shim() {
    let prefix = find_libraw().unwrap_or_else(|| {
        panic!(
            "LibRaw not found. Install it (macOS: `brew install libraw`) or set LIBRAW_DIR \
             to a prefix containing include/libraw/libraw.h and lib/. To build without \
             camera RAW support, disable the `libraw` feature."
        )
    });

    cc::Build::new()
        .file("shim/pe_libraw.c")
        .include(prefix.join("include"))
        .warnings(true)
        .compile("pe_libraw");
    // Compiled after the C shim, which references it (link order matters for GNU ld).
    // `cpp(true)` also links the C++ standard library.
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("shim/pe_libraw_xtrans.cpp")
        .include(prefix.join("include"))
        .warnings(true)
        .compile("pe_libraw_xtrans");

    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=dylib=raw_r");
}

#[cfg(feature = "libraw")]
fn find_libraw() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("LIBRAW_DIR") {
        candidates.push(dir.into());
    }
    for p in [
        "/opt/homebrew/opt/libraw",
        "/usr/local/opt/libraw",
        "/opt/homebrew",
        "/usr/local",
        "/usr",
    ] {
        candidates.push(p.into());
    }
    candidates.into_iter().find(|p| has_libraw(p))
}

#[cfg(feature = "libraw")]
fn has_libraw(prefix: &Path) -> bool {
    prefix.join("include/libraw/libraw.h").is_file()
}
