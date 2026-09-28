//! Compiles the TurboJPEG shim and links libjpeg-turbo dynamically.
//! libjpeg-turbo is located via `JPEG_TURBO_DIR` or common Homebrew/system prefixes.
//! Crates that must build without it make this crate an optional dependency.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=JPEG_TURBO_DIR");
    println!("cargo:rerun-if-changed=shim/pe_turbojpeg.c");
    let prefix = find().unwrap_or_else(|| {
        panic!(
            "libjpeg-turbo not found. Install it (macOS: `brew install jpeg-turbo`), set \
             JPEG_TURBO_DIR, or disable the `turbojpeg` feature of the depending crate."
        )
    });
    cc::Build::new()
        .file("shim/pe_turbojpeg.c")
        .include(prefix.join("include"))
        .warnings(true)
        .compile("pe_turbojpeg");
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=dylib=turbojpeg");
}

fn find() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("JPEG_TURBO_DIR") {
        candidates.push(dir.into());
    }
    for p in [
        "/opt/homebrew/opt/jpeg-turbo",
        "/usr/local/opt/jpeg-turbo",
        "/opt/homebrew",
        "/usr/local",
        "/usr",
    ] {
        candidates.push(p.into());
    }
    candidates.into_iter().find(|p| has_header(p))
}

fn has_header(prefix: &Path) -> bool {
    prefix.join("include/turbojpeg.h").is_file()
}
