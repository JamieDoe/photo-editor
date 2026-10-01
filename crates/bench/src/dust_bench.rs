//! `bench --dust`: sensor-dust detection (ADR 0058) on each camera file, as the editor
//! runs it: how many spots it proposes, where, and how long it takes. For finding
//! false alarms in real photos as much as for timing.

use std::path::PathBuf;
use std::time::Instant;

use app_core::{Engine, EngineConfig};
use serde_json::{Value, json};

pub fn run(files: &[PathBuf]) -> Value {
    let rows: Vec<Value> = files.iter().map(one).collect();
    json!({ "files": rows })
}

fn one(path: &PathBuf) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let engine = Engine::new(EngineConfig::default());
    let summary = match engine.open(path).wait() {
        Ok(s) => s,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    // The first run includes nothing cached; time a few.
    let mut times = Vec::new();
    let mut spots = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        spots = engine
            .find_dust(summary.id, Vec::new())
            .wait()
            .unwrap_or_default();
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    json!({
        "file": name,
        "found": spots.len(),
        "ms": (times[1] * 10.0).round() / 10.0,
        "spots": spots.iter().take(12).map(|s| json!({
            "x": (s.x * 1000.0).round() / 1000.0,
            "y": (s.y * 1000.0).round() / 1000.0,
            "radius": (s.radius * 10000.0).round() / 10000.0,
        })).collect::<Vec<_>>(),
    })
}
