//! `bench --look`: how our default render's tones compare with the camera's own JPEG.
//!
//! For each camera file, the embedded JPEG and our render (default recipe) are
//! reduced to Rec.709 luminance. Framing differs slightly (cameras correct lens
//! distortion in their JPEG), so pixels are not compared one to one: instead the two
//! luminance distributions are matched percentile by percentile. Each pair (our linear
//! value, the camera's linear value) at the same percentile samples the camera's tone
//! curve relative to ours. Used to design the default base look (ADR 0022).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use app_core::{EditRecipe, Engine, EngineConfig, PreviewQuality, PreviewRequest};
use image_core::OutputImage;
use image_core::color::srgb_to_linear;
use serde_json::{Value, json};

pub const PERCENTILES: [f32; 11] = [
    1.0, 5.0, 10.0, 25.0, 40.0, 50.0, 60.0, 75.0, 90.0, 95.0, 99.0,
];

pub fn run(files: &[PathBuf], recipe: EditRecipe) -> Value {
    let rows: Vec<Value> = files.iter().map(|f| compare(f, recipe.clone())).collect();
    json!({ "percentiles": PERCENTILES, "files": rows })
}

fn compare(path: &PathBuf, recipe: EditRecipe) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let engine = Engine::new(EngineConfig::default());
    let embedded: Arc<Mutex<Option<Arc<OutputImage>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&embedded);
    let summary = match engine
        .open_with_preview(path, move |f| *slot.lock().expect("slot") = Some(f.image))
        .wait()
    {
        Ok(s) => s,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    let Some(camera) = embedded.lock().expect("slot").take() else {
        return json!({ "file": name, "error": "no embedded preview" });
    };
    let ours = engine
        .render_preview(PreviewRequest {
            image: summary.id,
            recipe,
            quality: PreviewQuality::Interactive,
            target_long_edge: 1024,
            window: None,
        })
        .wait()
        .expect("render");
    let (mut a, mut b) = (luminance(&ours.image), luminance(&camera));
    a.sort_by(f32::total_cmp);
    b.sort_by(f32::total_cmp);
    let pairs: Vec<[f32; 2]> = PERCENTILES
        .iter()
        .map(|p| [percentile(&a, *p), percentile(&b, *p)])
        .collect();
    let to_srgb = |v: f32| image_core::color::linear_to_srgb(v.clamp(0.0, 1.0));
    let rms = (pairs
        .iter()
        .map(|[x, y]| (to_srgb(*x) - to_srgb(*y)).powi(2))
        .sum::<f32>()
        / pairs.len() as f32)
        .sqrt();
    json!({
        "file": name,
        // Tonal difference from the camera's JPEG, in sRGB units (0 = same tones).
        "rms_srgb": rms,
        "ours_mean": mean(&a),
        "camera_mean": mean(&b),
        // Stops the camera's median is above ours: the brightness gap at mid tones.
        "median_gap_ev": (percentile(&b, 50.0) / percentile(&a, 50.0).max(1e-6)).log2(),
        "pairs": pairs,
    })
}

/// Rec.709 luminance (linear) of every pixel of an 8-bit sRGB image.
fn luminance(img: &OutputImage) -> Vec<f32> {
    let ch = img.format().channels();
    img.data()
        .chunks_exact(ch)
        .map(|p| {
            let [r, g, b] = [p[0], p[1], p[2]].map(|v| srgb_to_linear(f32::from(v) / 255.0));
            0.2126 * r + 0.7152 * g + 0.0722 * b
        })
        .collect()
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    let i = ((p / 100.0) * (sorted.len() - 1) as f32).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}
