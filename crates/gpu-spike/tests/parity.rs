//! GPU/CPU parity: the same plan must produce (nearly) the same pixels.
//! Skips when no adapter is available, since GPU is never mandatory.

use image_core::{NeverCancel, PixelFormat};
use renderer::{CpuRenderer, EditRecipe, RenderBackend, RenderPlan};

#[test]
fn gpu_matches_cpu_within_quantisation() {
    let Some(gpu) = gpu_spike::GpuRenderer::new() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let source = fixtures::chart_linear(333, 211);
    // The spike implements the per-pixel stages only, so no default sharpening.
    let base = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    for recipe in [
        base,
        EditRecipe {
            exposure: 0.7,
            contrast: 40.0,
            temperature: -30.0,
            saturation: 35.0,
            ..base
        },
        EditRecipe {
            exposure: -1.0,
            contrast: -60.0,
            temperature: 80.0,
            saturation: -100.0,
            ..base
        },
    ] {
        let plan = RenderPlan::from_recipe(&recipe, None);
        let cpu = CpuRenderer
            .render(&plan, &source, PixelFormat::Rgba8, &NeverCancel)
            .unwrap();
        let gpu_out = gpu
            .render(&plan, &source, PixelFormat::Rgba8, &NeverCancel)
            .unwrap();
        let max = cpu
            .data()
            .iter()
            .zip(gpu_out.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 2, "{recipe:?}: max diff {max}");
    }
}
