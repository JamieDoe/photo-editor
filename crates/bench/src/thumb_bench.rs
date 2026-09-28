//! Library thumbnails: `bench --thumbnails`.
//!
//! Per file: cold generation time (no cache), cache-hit time, output size and how
//! the thumbnail was made. Then a "screenful": 48 thumbnails requested at once from
//! hard links to the camera files (cold cache), with the default browse lane, with
//! one worker (low-end machine), and while a library index runs on the background
//! lane. Files are warm in the OS cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use app_core::{Catalogue, Engine, EngineConfig, ThumbnailSource};
use serde_json::{Value, json};

const SCREENFUL: usize = 48;

pub fn run(files: &[PathBuf], camera_files: &[PathBuf]) -> Value {
    let per_file: Vec<Value> = files.iter().map(|f| file(f)).collect();
    let screenful = if camera_files.is_empty() {
        Value::Null
    } else {
        let default_workers = EngineConfig::default().jobs.browse.workers;
        json!({
            "thumbnails": SCREENFUL,
            "default_workers": default_workers,
            "default": screen(camera_files, None, false),
            "one_worker": screen(camera_files, Some(1), false),
            "while_indexing": screen(camera_files, None, true),
        })
    };
    json!({ "files": per_file, "screenful": screenful })
}

fn file(path: &Path) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let path = path.canonicalize().expect("fixture path");

    // Cold: no cache, so every request generates.
    let engine = Engine::new(EngineConfig::default());
    let mut cold = Vec::new();
    let mut last = None;
    for _ in 0..5 {
        let t = Instant::now();
        let thumb = engine.thumbnail(path.clone()).wait();
        cold.push(ms(t));
        last = Some(thumb);
    }
    let thumb = match last.expect("ran") {
        Ok(t) => t,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };

    // Warm: served from the disk cache.
    let dir = fixtures::TempDir::new("thumb-bench");
    let engine = Engine::new(EngineConfig {
        thumbnail_cache_dir: Some(dir.path().join("thumbs")),
        ..EngineConfig::default()
    });
    engine.thumbnail(path.clone()).wait().expect("first");
    let mut hits = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let hit = engine.thumbnail(path.clone()).wait().expect("hit");
        hits.push(ms(t));
        assert_eq!(hit.source, ThumbnailSource::Cache);
    }
    json!({
        "file": name,
        "source": format!("{:?}", thumb.source),
        "cold_ms_median": median(&mut cold),
        "cached_ms_median": median(&mut hits),
        "jpeg_kb": thumb.jpeg.len() as f64 / 1024.0,
    })
}

/// Requests a screenful of thumbnails at once (distinct hard links, so nothing is
/// cached) and waits for all of them.
fn screen(camera_files: &[PathBuf], workers: Option<usize>, while_indexing: bool) -> Value {
    let dir = fixtures::TempDir::new("thumb-screen");
    let links = make_links(dir.path(), "Screen", camera_files, SCREENFUL);
    let mut config = EngineConfig::default();
    if let Some(w) = workers {
        config.jobs.browse.workers = w;
        config.jobs.browse.compute_threads = Some(w);
    }
    let engine = Engine::new(config);

    let index = while_indexing.then(|| {
        let root = dir.path().join("Library");
        make_links(&root, "Index", camera_files, 3000);
        let catalogue = Arc::new(Catalogue::open_in_memory().expect("catalogue"));
        let root = root.canonicalize().expect("root");
        let handle = engine.index_folder(catalogue, root, |_| {});
        // Let the index pass get going before the screenful is requested.
        std::thread::sleep(std::time::Duration::from_millis(100));
        handle
    });

    let t = Instant::now();
    let handles: Vec<_> = links.into_iter().map(|p| engine.thumbnail(p)).collect();
    let mut first_ms = None;
    for h in handles {
        h.wait().expect("thumbnail");
        first_ms.get_or_insert_with(|| ms(t));
    }
    let total_ms = ms(t);
    // The index pass started 100 ms before the screenful; if it ends well after
    // `all_ms`, the thumbnails were made while it ran.
    let index_ms = index.map(|h| h.wait().expect("index").total_ms);
    json!({
        "first_ms": first_ms,
        "all_ms": total_ms,
        "per_second": SCREENFUL as f64 / (total_ms / 1000.0),
        "index_total_ms": index_ms,
    })
}

fn make_links(dir: &Path, prefix: &str, sources: &[PathBuf], n: usize) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("mkdir");
    (0..n)
        .map(|i| {
            let src = &sources[i % sources.len()];
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("raw");
            let dest = dir.join(format!("{prefix}_{i:05}.{ext}"));
            std::fs::hard_link(src, &dest).expect("hard link (same volume as the fixtures)");
            dest.canonicalize().expect("link")
        })
        .collect()
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
