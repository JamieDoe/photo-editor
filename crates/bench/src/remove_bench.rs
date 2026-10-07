//! `bench --remove`: the Remove tool's fill (ADR 0066) on each camera file, at the
//! interactive preview's size and at full size, for three typical removals: a small
//! object, a wire across the frame, and a person-sized area.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image_core::{LinearImage, NeverCancel};
use raw::{DecodeOptions, DecodeScale, DecoderRegistry};
use renderer::masks::brush::Stroke;
use renderer::remove::{Removal, remove};
use serde_json::{Value, json};

pub fn run(files: &[PathBuf]) -> Value {
    let rows: Vec<Value> = files.iter().map(PathBuf::as_path).map(one).collect();
    json!({ "files": rows })
}

/// The removals timed, in photo fractions (the brush size a fraction of the diagonal).
fn cases() -> Vec<(&'static str, Removal)> {
    let removal = |size: f32, points: &[[f32; 2]]| Removal {
        strokes: vec![Stroke {
            erase: false,
            size,
            feather: 10.0,
            flow: 100.0,
            points: points.to_vec(),
        }],
    };
    vec![
        // A bird or a sign: a dab 1 % of the diagonal across.
        ("small", removal(0.005, &[[0.55, 0.3]])),
        // A power line across the whole frame.
        (
            "wire",
            removal(0.0015, &[[0.0, 0.22], [0.5, 0.26], [1.0, 0.24]]),
        ),
        // A person: a tall stroke 8 % of the diagonal wide.
        ("person", removal(0.04, &[[0.3, 0.45], [0.3, 0.75]])),
    ]
}

fn one(path: &Path) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let decoders = DecoderRegistry::with_defaults();
    let decode = |scale| {
        decoders
            .decode(path, DecodeOptions::new(scale), &NeverCancel)
            .map(|d| d.image)
    };
    let (preview, full) = match (
        decode(DecodeScale::AtLeast(2048)),
        decode(DecodeScale::Full),
    ) {
        (Ok(p), Ok(f)) => (p, f),
        (Err(e), _) | (_, Err(e)) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    let timed = |image: &LinearImage, removal: &Removal| {
        let mut times: Vec<f64> = (0..3)
            .map(|_| {
                let t = Instant::now();
                let out = remove(image, std::slice::from_ref(removal), &NeverCancel);
                assert!(out.is_ok());
                t.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(f64::total_cmp);
        (times[1] * 10.0).round() / 10.0
    };
    let cases: Vec<Value> = cases()
        .iter()
        .map(|(case, removal)| {
            json!({
                "case": case,
                "preview_ms": timed(&preview, removal),
                "full_ms": timed(&full, removal),
            })
        })
        .collect();
    json!({
        "file": name,
        "preview": format!("{}x{}", preview.width(), preview.height()),
        "full": format!("{}x{}", full.width(), full.height()),
        "cases": cases,
    })
}
