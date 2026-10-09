//! Phase 0 benchmark harness.
//!
//! Usage (always build with --release):
//!
//!     cargo run -p bench --release -- [--iterations N] [--out DIR] [FILE|DIR ...]
//!
//! With no paths, benchmarks `tests/fixtures/synthetic` and `tests/fixtures/local`.
//! Each file runs in a child process so peak memory (max RSS) is per file. Results
//! are printed as a markdown table and written as JSON for comparison over time.

mod auto_bench;
mod dust_bench;
mod estimate_bench;
mod index_bench;
mod look;
mod measure;
mod remove_bench;
mod report;
mod segment_bench;
mod thumb_bench;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iterations = 5usize;
    let mut out_dir = workspace_root().join("bench-results");
    let mut single: Option<PathBuf> = None;
    let mut memory: Option<PathBuf> = None;
    let mut decode_peak: Option<PathBuf> = None;
    let mut index_scale: Option<usize> = None;
    let mut index_links: Option<usize> = None;
    let mut thumbnails = false;
    let mut look = false;
    let mut args_flat = false;
    let mut inputs = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--iterations" => {
                iterations = it.next().and_then(|v| v.parse().ok()).unwrap_or(iterations)
            }
            "--out" => out_dir = it.next().map(PathBuf::from).unwrap_or(out_dir),
            "--single" => single = it.next().map(PathBuf::from),
            "--memory" => memory = it.next().map(PathBuf::from),
            "--decode-peak" => decode_peak = it.next().map(PathBuf::from),
            "--index-scale" => index_scale = it.next().and_then(|v| v.parse().ok()),
            "--index-links" => index_links = it.next().and_then(|v| v.parse().ok()),
            "--thumbnails" => thumbnails = true,
            "--look" => look = true,
            "--segment" => {
                let out = it.next().map(PathBuf::from);
                let result = segment_bench::run(&camera_files(), out.as_deref());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("serialisable result")
                );
                return;
            }
            "--auto" => {
                let out = it.next().map(PathBuf::from);
                let result = auto_bench::run(&camera_files(), out.as_deref());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("serialisable result")
                );
                return;
            }
            "--dust" => {
                let result = dust_bench::run(&camera_files());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("serialisable result")
                );
                return;
            }
            "--estimate" => {
                let result = estimate_bench::run(&camera_files());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("serialisable result")
                );
                return;
            }
            "--remove" => {
                let result = remove_bench::run(&camera_files());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("serialisable result")
                );
                return;
            }
            "--flat" => args_flat = true,
            "-h" | "--help" => {
                println!("bench [--iterations N] [--out DIR] [FILE|DIR ...]");
                return;
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }

    if look {
        let recipe = if args_flat {
            app_core::EditRecipe {
                look: app_core::Look::Flat,
                ..app_core::EditRecipe::default()
            }
        } else {
            app_core::EditRecipe::default()
        };
        let result = look::run(&camera_files(), recipe);
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serialisable result")
        );
        return;
    }
    if thumbnails {
        let fixtures = workspace_root().join("tests/fixtures");
        let files = collect_files(&[fixtures.join("synthetic"), fixtures.join("local")]);
        let result = thumb_bench::run(&files, &camera_files());
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serialisable result")
        );
        return;
    }
    if let Some(n) = index_links {
        let result = index_bench::run_links(n, &camera_files());
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serialisable result")
        );
        return;
    }
    if let Some(n) = index_scale {
        // Library indexing at scale (see index_bench.rs).
        let result = index_bench::run(n);
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serialisable result")
        );
        return;
    }
    if let Some(file) = decode_peak {
        println!(
            "{}",
            serde_json::to_string(&measure::decode_peak(&file)).expect("serialisable result")
        );
        return;
    }
    if let Some(file) = memory {
        // Child mode: app-like peak memory for one file.
        println!(
            "{}",
            serde_json::to_string(&measure::memory(&file)).expect("serialisable result")
        );
        return;
    }
    if let Some(file) = single {
        // Child mode: measure one file, print JSON.
        let result = measure::file(&file, iterations);
        println!(
            "{}",
            serde_json::to_string(&result).expect("serialisable result")
        );
        return;
    }

    if cfg!(debug_assertions) {
        eprintln!("warning: debug build; numbers are not representative. Use --release.");
    }
    if inputs.is_empty() {
        let fixtures = workspace_root().join("tests/fixtures");
        inputs = vec![fixtures.join("synthetic"), fixtures.join("local")];
    }
    let files = collect_files(&inputs);
    if files.is_empty() {
        eprintln!("no supported images found; see tests/fixtures/README.md");
        std::process::exit(1);
    }

    let exe = std::env::current_exe().expect("current exe");
    let mut results = Vec::new();
    for file in &files {
        eprintln!("benchmarking {} ...", file.display());
        let file_arg = file.display().to_string();
        let timing = run_child(
            &exe,
            &[
                "--single",
                &file_arg,
                "--iterations",
                &iterations.to_string(),
            ],
        );
        let memory = run_child(&exe, &["--memory", &file_arg]);
        let decode_peak = run_child(&exe, &["--decode-peak", &file_arg]);
        match (timing, memory) {
            (Some(mut v), memory) => {
                v["app_memory"] = memory.unwrap_or(Value::Null);
                v["full_decode_memory"] = decode_peak.unwrap_or(Value::Null);
                results.push(v);
            }
            (None, _) => {
                results.push(serde_json::json!({ "file": file_arg, "error": "child failed" }))
            }
        }
    }

    let report = report::build(&results, iterations);
    println!("{}", report::markdown(&report));
    std::fs::create_dir_all(&out_dir).expect("create output dir");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = out_dir.join(format!("phase0-{stamp}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&report).expect("json"))
        .expect("write results");
    eprintln!("wrote {}", path.display());
}

fn run_child(exe: &Path, args: &[&str]) -> Option<Value> {
    let output = Command::new(exe).args(args).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .lines()
        .last()
        .and_then(|l| serde_json::from_str::<Value>(l).ok());
    if parsed.is_none() {
        eprintln!(
            "  child {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parsed
}

/// Real camera RAW files in `tests/fixtures/local` (git-ignored; may be empty).
fn camera_files() -> Vec<PathBuf> {
    let local = workspace_root().join("tests/fixtures/local");
    let registry = raw::DecoderRegistry::with_defaults();
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&local)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    sources.retain(|p| {
        registry
            .decoder_for(p)
            .is_some_and(|d| d.name() == "libraw")
            && !p.to_string_lossy().contains("synthetic")
    });
    sources.sort();
    sources
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn collect_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let registry = raw::DecoderRegistry::with_defaults();
    let supported = |p: &Path| registry.decoder_for(p).is_some();
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(input)
                .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
                .unwrap_or_default();
            entries.sort();
            files.extend(entries.into_iter().filter(|p| p.is_file() && supported(p)));
        } else if input.is_file() && supported(input) {
            files.push(input.clone());
        }
    }
    files
}
