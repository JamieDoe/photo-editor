//! Library indexing at scale: `bench --index-scale N`.
//!
//! Generates N distinct files (70 KB each, 100 per folder) in a temporary directory,
//! then indexes them into an on-disk catalogue: first pass, unchanged rescan, and a
//! rescan after 1% of the files changed. Files are warm in the OS cache (a cold-cache
//! run needs `sudo purge` on macOS and is not automated).

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use app_core::{Catalogue, Engine, EngineConfig, IndexSummary};
use serde_json::{Value, json};

const FILE_BYTES: usize = 70 * 1024;

pub fn run(n: usize) -> Value {
    let dir = fixtures::TempDir::new("index-bench");
    let root = dir.path().join("Library");
    let t = Instant::now();
    for i in 0..n {
        write_file(
            &root.join(format!("{:04}/DSC_{i:06}.NEF", i / 100)),
            i as u64,
            0,
        );
    }
    let generate_ms = ms(t);
    let root = root.canonicalize().expect("root");

    let catalogue =
        Arc::new(Catalogue::open(&dir.path().join("catalogue.sqlite")).expect("catalogue"));
    let engine = Engine::new(EngineConfig::default());
    let index = || -> IndexSummary {
        engine
            .index_folder(Arc::clone(&catalogue), root.clone(), |_| {})
            .wait()
            .expect("index")
    };

    let first = index();
    let rescan = index();
    // Change 1% of the files (new content, new mtime).
    for i in (0..n).step_by(100) {
        write_file(
            &root.join(format!("{:04}/DSC_{i:06}.NEF", i / 100)),
            i as u64,
            1,
        );
    }
    let changed = index();

    let db_bytes: u64 = std::fs::read_dir(dir.path())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("catalogue"))
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0);
    json!({
        "files": n,
        "file_kb": FILE_BYTES / 1024,
        "generate_ms": generate_ms,
        "first": summary(&first),
        "rescan_unchanged": summary(&rescan),
        "rescan_1pct_changed": summary(&changed),
        "catalogue_mb": db_bytes as f64 / 1e6,
    })
}

fn summary(s: &IndexSummary) -> Value {
    json!({
        "total_ms": s.total_ms,
        "walk_ms": s.walk_ms,
        "found": s.found,
        "new": s.new,
        "changed": s.changed,
        "unchanged": s.unchanged,
        "missing": s.missing,
        "files_per_sec": s.found as f64 / (s.total_ms / 1000.0),
    })
}

fn write_file(path: &Path, seed: u64, generation: u64) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ generation.wrapping_add(1);
    let mut bytes = Vec::with_capacity(FILE_BYTES);
    while bytes.len() < FILE_BYTES {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(FILE_BYTES);
    let mut f = std::fs::File::create(path).expect("create");
    f.write_all(&bytes).expect("write");
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
