//! Compiles the LibRaw C shim and links the thread-safe LibRaw (`libraw_r`).
//!
//! LibRaw is located via `LIBRAW_DIR` (containing `include/` and `lib/`), falling back
//! to common Homebrew/system prefixes. Linking is dynamic; see ADR 0003.

#[cfg(feature = "libraw")]
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=LIBRAW_DIR");
    println!("cargo:rerun-if-changed=shim/pe_libraw.c");
    println!("cargo:rerun-if-changed=shim/pe_libraw.h");
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
