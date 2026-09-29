//! CPU vs GPU comparison for the Phase 0 render plan.
//!
//!     cargo run -p gpu-spike --release --bin gpu-bench -- [RAW or JPEG file]

use std::time::Instant;

use image_core::{LinearImage, NeverCancel, PixelFormat, Pyramid};
use raw::{DecodeOptions, DecodeScale, DecoderRegistry};
use renderer::{CpuRenderer, EditRecipe, RenderBackend, RenderPlan};

fn main() {
    let Some(gpu) = gpu_spike::GpuRenderer::new() else {
        println!("No GPU adapter available: CPU path only (this is a supported configuration).");
        return;
    };
    println!("Adapter: {}", gpu.adapter_name());

    let source: LinearImage = match std::env::args().nth(1) {
        Some(path) => {
            DecoderRegistry::with_defaults()
                .decode(
                    path.as_ref(),
                    DecodeOptions::new(DecodeScale::Full),
                    &NeverCancel,
                )
                .expect("decode")
                .image
        }
        None => fixtures::chart_linear(6000, 4000),
    };
    let recipe = EditRecipe {
        exposure: 0.35,
        contrast: 25.0,
        temperature: 15.0,
        saturation: 20.0,
        // Per-pixel stages only: the spike has no neighbourhood stages.
        sharpening: 0.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
    let pyramid = Pyramid::build(source, 256);

    println!(
        "\n| Size | MP | CPU (ms) | GPU incl. upload+readback (ms) | GPU resident, readback only (ms) | max diff |"
    );
    println!("|---|---|---|---|---|---|");
    for level in pyramid.levels().iter().take(3) {
        let mp = f64::from(level.width()) * f64::from(level.height()) / 1e6;
        let cpu_out = CpuRenderer
            .render(&plan, level, PixelFormat::Rgba8, &NeverCancel)
            .unwrap();
        let cpu = median_ms(10, || {
            CpuRenderer
                .render(&plan, level, PixelFormat::Rgba8, &NeverCancel)
                .unwrap();
        });
        let gpu_full = median_ms(10, || {
            gpu.render(&plan, level, PixelFormat::Rgba8, &NeverCancel)
                .unwrap();
        });
        let resident = gpu.upload(level);
        let gpu_out = gpu.render_resident(&plan, &resident).unwrap();
        let gpu_res = median_ms(10, || {
            gpu.render_resident(&plan, &resident).unwrap();
        });
        let diff = cpu_out
            .data()
            .iter()
            .zip(gpu_out.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        println!(
            "| {}x{} | {mp:.1} | {cpu:.1} | {gpu_full:.1} | {gpu_res:.1} | {diff} |",
            level.width(),
            level.height()
        );
    }
}

fn median_ms(n: usize, mut f: impl FnMut()) -> f64 {
    f();
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v[n / 2]
}
