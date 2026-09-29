use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use app_core::{
    EditRecipe, Engine, EngineConfig, ExportFormat, ExportRequest, ImageId, PreviewQuality,
    PreviewRequest,
};
use image_core::{NeverCancel, PixelFormat, Pyramid};
use raw::{DecodeOptions, DecodeScale, DecoderRegistry};
use renderer::{CpuRenderer, RenderBackend, RenderPlan, Stage};
use serde_json::{Value, json};

/// A representative edit: every stage active.
fn bench_recipe(i: usize) -> EditRecipe {
    // Vary exposure slightly so repeated renders never hit the preview cache. Callers
    // use disjoint seed ranges (< 10_000) so recipes never repeat or reach a clamp.
    EditRecipe {
        exposure: 0.35 + i as f32 * 1e-4,
        contrast: 25.0,
        highlights: -40.0,
        shadows: 35.0,
        whites: 10.0,
        blacks: -10.0,
        temperature: 15.0,
        saturation: 20.0,
        ..EditRecipe::default()
    }
}

pub fn file(path: &Path, iterations: usize) -> Value {
    let iterations = iterations.max(1);
    let file_bytes = std::fs::metadata(path).map_or(0, |m| m.len());
    let registry = DecoderRegistry::with_defaults();
    let config = EngineConfig::default();

    // --- Decode (preview resolution, as used when opening) ---
    let preview_opts = DecodeOptions::new(DecodeScale::AtLeast(config.preview_source_min_edge));
    let (decode_preview, decoded) = timed_n(iterations, || {
        registry.decode(path, preview_opts, &NeverCancel)
    });
    let decoded = match decoded {
        Ok(d) => d,
        Err(e) => return json!({ "file": name(path), "error": e.to_string() }),
    };
    let info = decoded.info.clone();

    // --- Embedded preview (shown while decoding) ---
    let (embedded_ms, embedded) = timed_n(iterations, || {
        registry
            .embedded_preview(path, config.embedded_preview_min_edge, &NeverCancel)
            .ok()
            .flatten()
    });
    let embedded = embedded.map(|p| {
        json!({
            "ms": embedded_ms,
            "embedded_size": format!("{}x{}", p.embedded_width, p.embedded_height),
            "shown_size": format!("{}x{}", p.image.width(), p.image.height()),
        })
    });

    // --- Pyramid ---
    let base = decoded.image;
    let (pyramid_ms, pyramid) = timed_n(iterations, || {
        Pyramid::build(base.clone(), config.limits.thumbnail_long_edge)
    });
    let levels: Vec<String> = pyramid
        .levels()
        .iter()
        .map(|l| format!("{}x{}", l.width(), l.height()))
        .collect();

    // --- Per-stage cost on the interactive-size level (renderer only, no job overhead) ---
    let interactive_level =
        &pyramid.levels()[pyramid.select_index(config.limits.interactive_max_long_edge * 85 / 100)];
    let full_plan = RenderPlan::from_recipe(&bench_recipe(0));
    let mut stage_costs = Vec::new();
    for n in 0..=full_plan.stages.len() {
        let plan = RenderPlan::new(full_plan.stages[..n].to_vec());
        let label = if n == 0 {
            "no stages (u16->f32 + sRGB encode)".to_owned()
        } else {
            format!("+{}", stage_name(&full_plan.stages[n - 1]))
        };
        let (t, _) = timed_n(iterations * 4, || {
            CpuRenderer
                .render(&plan, interactive_level, PixelFormat::Rgba8, &NeverCancel)
                .expect("render")
        });
        stage_costs.push(json!({ "stages": label, "ms": t }));
    }

    // --- Engine path: open + previews at each quality (includes job dispatch) ---
    let engine = Engine::new(config.clone());
    let t = Instant::now();
    let summary = engine.open(path).wait().expect("open");
    let open_ms = ms(t);
    let preview = |quality, target, i| {
        let t = Instant::now();
        let frame = engine
            .render_preview(PreviewRequest {
                image: summary.id,
                recipe: bench_recipe(i),
                quality,
                target_long_edge: target,
            })
            .wait()
            .expect("preview");
        (
            ms(t),
            frame.render_ms,
            format!("{}x{}", frame.image.width(), frame.image.height()),
        )
    };
    let mut quality_results = serde_json::Map::new();
    for (label, quality, target, seed) in [
        ("thumbnail", PreviewQuality::Thumbnail, 256, 0),
        ("interactive", PreviewQuality::Interactive, 1600, 1000),
        ("detail", PreviewQuality::Detail, 3200, 2000),
    ] {
        let runs: Vec<_> = (0..iterations * 4)
            .map(|i| preview(quality, target, seed + i))
            .collect();
        quality_results.insert(
            label.into(),
            json!({
                "size": runs[0].2,
                "render_ms": median(runs.iter().map(|r| r.1).collect()),
                "job_total_ms": median(runs.iter().map(|r| r.0).collect()),
            }),
        );
    }

    // --- Full decode ---
    let full_opts = DecodeOptions::new(DecodeScale::Full);
    let full_iters = iterations.min(3);
    let (decode_full, full) = timed_n(full_iters, || {
        registry
            .decode(path, full_opts, &NeverCancel)
            .expect("full decode")
    });

    // --- Full-resolution render + cancellation latency ---
    let (render_full, _) = timed_n(full_iters, || {
        CpuRenderer
            .render(&full_plan, &full.image, PixelFormat::Rgb8, &NeverCancel)
            .expect("render")
    });
    let cancel_latency = cancellation_latency(&full_plan, &full.image);
    let encoders = encoder_comparison(&full_plan, &full.image, full_iters);
    let full_dims = (full.image.width(), full.image.height());
    drop(full);

    // --- Export (decode full + render + encode + write), and interactive under load ---
    let out_dir = std::env::temp_dir().join(format!("pe-bench-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let export_req = |i: usize| ExportRequest {
        image: summary.id,
        recipe: bench_recipe(i),
        destination: out_dir.join(format!("export-{i}.jpg")),
        format: ExportFormat::Jpeg { quality: 92 },
    };
    let mut exports = Vec::new();
    for i in 0..full_iters {
        exports.push(engine.export(export_req(i), |_| {}).wait().expect("export"));
    }
    let under_load = interactive_under_export(&engine, summary.id, export_req(99));
    let _ = std::fs::remove_dir_all(&out_dir);

    json!({
        "file": name(path),
        "file_mb": file_bytes as f64 / 1e6,
        "decoder": info.decoder,
        "camera": format!("{} {}", info.make, info.model).trim(),
        "full_size": format!("{}x{}", full_dims.0, full_dims.1),
        "megapixels": f64::from(full_dims.0) * f64::from(full_dims.1) / 1e6,
        "preview_decode_size": levels.first(),
        "pyramid_levels": levels,
        "decode_preview_ms": decode_preview,
        "embedded_preview": embedded,
        "pyramid_ms": pyramid_ms,
        "open_total_ms": open_ms,
        "previews": quality_results,
        "stage_costs_interactive": stage_costs,
        "interactive_level": format!("{}x{}", interactive_level.width(), interactive_level.height()),
        "decode_full_ms": decode_full,
        "render_full_ms": render_full,
        "cancel_latency_ms": cancel_latency,
        "jpeg_encoders": encoders,
        "export": {
            "total_ms": median(exports.iter().map(|e| e.total_ms).collect()),
            "decode_ms": median(exports.iter().map(|e| e.decode_ms).collect()),
            "render_ms": median(exports.iter().map(|e| e.render_ms).collect()),
            "encode_ms": median(exports.iter().map(|e| e.encode_ms).collect()),
            "write_ms": median(exports.iter().map(|e| e.write_ms).collect()),
            "bytes": exports.first().map(|e| e.bytes),
        },
        "interactive_under_export": under_load,
        "bench_process_peak_rss_mb": peak_rss_mb(),
    })
}

/// Full-resolution JPEG (q92, 4:4:4) encode time and size for each available encoder.
fn encoder_comparison(
    plan: &RenderPlan,
    image: &image_core::LinearImage,
    iterations: usize,
) -> Value {
    use export::{ExportFormat, JpegEncoder};
    let rgb = CpuRenderer
        .render(plan, image, PixelFormat::Rgb8, &NeverCancel)
        .expect("render");
    let mut out = serde_json::Map::new();
    for encoder in [JpegEncoder::PureRust, JpegEncoder::Turbo] {
        let format = ExportFormat::Jpeg { quality: 92 };
        if export::encode_with(&rgb, format, encoder).is_err() {
            continue; // not compiled in
        }
        let (t, bytes) = timed_n(iterations, || {
            export::encode_with(&rgb, format, encoder).expect("encode")
        });
        out.insert(
            encoder.name().into(),
            json!({ "ms": t, "bytes": bytes.len() }),
        );
    }
    Value::Object(out)
}

/// Time from requesting cancellation to the render returning, on a full-size image.
fn cancellation_latency(plan: &RenderPlan, image: &image_core::LinearImage) -> Value {
    let mut samples = Vec::new();
    for delay_ms in [2u64, 2, 2, 5, 5, 5, 10, 10, 10] {
        let flag = Arc::new(AtomicBool::new(false));
        let setter = Arc::clone(&flag);
        let cancelled_at = Arc::new(std::sync::Mutex::new(None::<Instant>));
        let at = Arc::clone(&cancelled_at);
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            *at.lock().unwrap() = Some(Instant::now());
            setter.store(true, Ordering::Relaxed);
        });
        let result = CpuRenderer.render(plan, image, PixelFormat::Rgb8, flag.as_ref());
        let returned = Instant::now();
        t.join().unwrap();
        if result.is_err()
            && let Some(at) = *cancelled_at.lock().unwrap()
        {
            samples.push(returned.saturating_duration_since(at).as_secs_f64() * 1000.0);
        }
    }
    let max = samples.iter().copied().fold(f64::NAN, f64::max);
    json!({ "n": samples.len(), "p50": median(samples.clone()), "max": max, "samples": samples })
}

/// Interactive preview latency with and without a concurrent export.
fn interactive_under_export(engine: &Engine, id: ImageId, export: ExportRequest) -> Value {
    let run = |seed: usize| {
        let t = Instant::now();
        engine
            .render_preview(PreviewRequest {
                image: id,
                recipe: bench_recipe(seed),
                quality: PreviewQuality::Interactive,
                target_long_edge: 1600,
            })
            .wait()
            .expect("preview");
        ms(t)
    };
    let idle: Vec<f64> = (0..20).map(|i| run(5000 + i)).collect();
    let handle = engine.export(export, |_| {});
    let mut loaded = Vec::new();
    let mut i = 0;
    let started = Instant::now();
    // Sample while the export is (very likely) still running.
    while loaded.len() < 20 && started.elapsed() < Duration::from_secs(20) {
        loaded.push(run(6000 + i));
        i += 1;
    }
    let export_ms = handle.wait().map(|s| s.total_ms).unwrap_or(f64::NAN);
    json!({
        "idle_p50_ms": median(idle.clone()),
        "idle_max_ms": idle.iter().copied().fold(0.0, f64::max),
        "during_export_p50_ms": median(loaded.clone()),
        "during_export_max_ms": loaded.iter().copied().fold(0.0, f64::max),
        "export_ms": export_ms,
    })
}

/// App-like peak memory: a fresh engine opens the file, renders previews and exports,
/// exactly as the desktop app would. Run in its own process.
pub fn memory(path: &Path) -> Value {
    let engine = Engine::new(EngineConfig::default());
    let id = match engine.open(path).wait() {
        Ok(s) => s.id,
        Err(e) => return json!({ "file": name(path), "error": format!("{e:?}") }),
    };
    let after_open = peak_rss_mb();
    for (i, quality) in [PreviewQuality::Interactive, PreviewQuality::Detail]
        .into_iter()
        .enumerate()
    {
        let request = PreviewRequest {
            image: id,
            recipe: bench_recipe(i),
            quality,
            target_long_edge: 3200,
        };
        engine.render_preview(request).wait().expect("preview");
    }
    let after_previews = peak_rss_mb();
    let out = std::env::temp_dir().join(format!("pe-bench-mem-{}.jpg", std::process::id()));
    let export = ExportRequest {
        image: id,
        recipe: bench_recipe(0),
        destination: out.clone(),
        format: ExportFormat::Jpeg { quality: 92 },
    };
    engine.export(export, |_| {}).wait().expect("export");
    let _ = std::fs::remove_file(out);
    json!({
        "peak_after_open_mb": after_open,
        "peak_after_previews_mb": after_previews,
        "peak_after_export_mb": peak_rss_mb(),
        "open_images_mb": engine.open_image_bytes() as f64 / 1e6,
        "preview_cache_mb": engine.preview_cache_stats().used_bytes as f64 / 1e6,
    })
}

/// Peak memory of a lone full-resolution decode in a fresh process: isolates the
/// decoder's own working memory from the engine's.
pub fn decode_peak(path: &Path) -> Value {
    let before = peak_rss_mb();
    let decoded = DecoderRegistry::with_defaults().decode(
        path,
        DecodeOptions::new(DecodeScale::Full),
        &NeverCancel,
    );
    match decoded {
        Ok(d) => {
            let px = f64::from(d.image.width()) * f64::from(d.image.height());
            let peak = peak_rss_mb();
            json!({
                "baseline_mb": before,
                "peak_mb": peak,
                "result_mb": d.image.byte_size() as f64 / 1e6,
                "peak_bytes_per_pixel": (peak - before) * 1e6 / px,
            })
        }
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn stage_name(stage: &Stage) -> &'static str {
    stage.name()
}

fn timed_n<T>(n: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    // One warm-up run (page faults, LUT init), then the median of n timed runs.
    let mut last = f();
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        last = f();
        times.push(ms(t));
    }
    (median(times), last)
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn peak_rss_mb() -> f64 {
    // SAFETY: getrusage writes into the provided struct only.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let max = usage.ru_maxrss as f64;
    // macOS reports bytes, Linux kilobytes.
    if cfg!(target_os = "macos") {
        max / 1e6
    } else {
        max / 1e3
    }
}
