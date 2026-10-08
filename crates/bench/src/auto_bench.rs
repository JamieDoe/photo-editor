//! `bench --auto [DIR]`: Auto tone (ADR 0071) on each camera file, as the editor runs
//! it (from the open photo's small sample), with its settings and time. With a
//! directory, each photo before and after is written there as one JPEG, side by side.

use std::path::{Path, PathBuf};
use std::time::Instant;

use app_core::{EditRecipe, Engine, EngineConfig, PreviewQuality, PreviewRequest};
use image_core::{OutputImage, PixelFormat};
use serde_json::{Value, json};

pub fn run(files: &[PathBuf], out: Option<&Path>) -> Value {
    let rows: Vec<Value> = files.iter().map(|f| one(f, out)).collect();
    json!({ "files": rows })
}

fn one(path: &Path, out: Option<&Path>) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let engine = Engine::new(EngineConfig::default());
    let summary = match engine.open(path).wait() {
        Ok(s) => s,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    let base = EditRecipe::default();
    // The first run warms caches; the median of three is reported.
    let mut times = Vec::new();
    let mut tone = None;
    for _ in 0..3 {
        let t = Instant::now();
        tone = Some(
            engine
                .auto_tone(summary.id, &base)
                .wait()
                .expect("auto tone"),
        );
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let tone = tone.expect("ran");
    if let Some(dir) = out {
        let render = |recipe: EditRecipe| {
            engine
                .render_preview(PreviewRequest {
                    image: summary.id,
                    recipe,
                    quality: PreviewQuality::Detail,
                    target_long_edge: 900,
                    window: None,
                })
                .wait()
                .expect("render")
                .image
        };
        let pair = side_by_side(&render(base.clone()), &render(tone.apply(&base)));
        let bytes = export::encode_in(
            &pair,
            export::ExportFormat::Jpeg { quality: 85 },
            export::colour::ExportColourSpace::Srgb,
            None,
        )
        .expect("encode");
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::write(dir.join(format!("auto-{stem}.jpg")), bytes).expect("write");
    }
    json!({
        "file": name,
        "ms": (times[1] * 10.0).round() / 10.0,
        "tone": tone,
    })
}

/// Two RGBA frames of one height as one RGB image, a gap between them.
fn side_by_side(a: &OutputImage, b: &OutputImage) -> OutputImage {
    const GAP: u32 = 8;
    let h = a.height().min(b.height());
    let w = a.width() + GAP + b.width();
    let mut data = vec![20u8; (w * h * 3) as usize];
    for (img, x0) in [(a, 0), (b, a.width() + GAP)] {
        let ch = img.format().channels();
        for y in 0..h {
            for x in 0..img.width() {
                let s = ((y * img.width() + x) as usize) * ch;
                let d = ((y * w + x0 + x) * 3) as usize;
                data[d..d + 3].copy_from_slice(&img.data()[s..s + 3]);
            }
        }
    }
    OutputImage::from_raw(w, h, PixelFormat::Rgb8, data).expect("sized")
}
