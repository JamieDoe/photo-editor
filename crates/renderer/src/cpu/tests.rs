use std::sync::atomic::AtomicBool;

use image_core::{NeverCancel, color};

use super::*;
use crate::{EditRecipe, Stage, ops};

fn chart() -> LinearImage {
    fixtures::chart_linear(96, 64)
}

fn render(recipe: &EditRecipe, img: &LinearImage) -> OutputImage {
    CpuRenderer
        .render(
            &RenderPlan::from_recipe(recipe),
            img,
            PixelFormat::Rgb8,
            &NeverCancel,
        )
        .unwrap()
}

/// Reference renderer: scalar ops per pixel, no LUTs or fusion.
fn reference(plan: &RenderPlan, img: &LinearImage) -> Vec<u8> {
    let mut out = Vec::new();
    for px in img.data().as_chunks::<3>().0 {
        let mut rgb = [px[0], px[1], px[2]].map(|v| f32::from(v) / 65535.0);
        for stage in &plan.stages {
            rgb = match *stage {
                Stage::WhiteBalance { gains } => {
                    [rgb[0] * gains[0], rgb[1] * gains[1], rgb[2] * gains[2]]
                }
                Stage::Exposure { multiplier } => rgb.map(|c| c * multiplier),
                Stage::Contrast { gamma } => rgb.map(|c| ops::contrast::apply(c, gamma)),
                Stage::Saturation { factor } => ops::saturation::apply(rgb, factor),
            };
        }
        out.extend(rgb.map(|c| (color::linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8));
    }
    out
}

#[test]
fn matches_scalar_reference_for_all_stages() {
    let img = chart();
    let recipe = EditRecipe {
        exposure: 0.6,
        contrast: 45.0,
        temperature: -35.0,
        saturation: 30.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe);
    let fast = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    let slow = reference(&plan, &img);
    let max_diff = fast
        .data()
        .iter()
        .zip(&slow)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(max_diff <= 1, "max diff {max_diff}");
}

#[test]
fn identity_reproduces_display_referred_source() {
    // An 8-bit sRGB source, linearised, must come back unchanged with no edits.
    let table = color::srgb8_to_linear16_table();
    let data: Vec<u16> = (0..=255u16).flat_map(|v| [table[v as usize]; 3]).collect();
    let img = LinearImage::new(256, 1, data).unwrap();
    let out = render(&EditRecipe::default(), &img);
    for (i, px) in out.data().as_chunks::<3>().0.iter().enumerate() {
        assert_eq!(*px, [i as u8; 3]);
    }
}

#[test]
fn rgba_output_is_opaque() {
    let img = chart();
    let out = CpuRenderer
        .render(
            &RenderPlan::from_recipe(&EditRecipe::default()),
            &img,
            PixelFormat::Rgba8,
            &NeverCancel,
        )
        .unwrap();
    assert_eq!(out.byte_size(), 96 * 64 * 4);
    assert!(out.data().as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}

#[test]
fn exposure_plus_one_doubles_linear_values() {
    let img = LinearImage::new(1, 1, vec![6554; 3]).unwrap(); // ~0.1 linear
    let out = render(
        &EditRecipe {
            exposure: 1.0,
            ..Default::default()
        },
        &img,
    );
    let expected = (color::linear_to_srgb(0.2) * 255.0).round() as u8;
    assert!(out.data()[0].abs_diff(expected) <= 1);
}

#[test]
fn pre_cancelled_render_returns_cancelled() {
    let img = chart();
    let cancelled = AtomicBool::new(true);
    let result = CpuRenderer.render(
        &RenderPlan::new(vec![]),
        &img,
        PixelFormat::Rgb8,
        &cancelled,
    );
    assert_eq!(result.unwrap_err(), RenderError::Cancelled);
}

#[test]
fn render_into_rejects_mismatched_buffer() {
    let img = chart();
    let mut out = OutputImage::new(10, 10, PixelFormat::Rgba8).unwrap();
    let err = CpuRenderer.render_into(&RenderPlan::new(vec![]), &img, &mut out, &NeverCancel);
    assert!(matches!(err, Err(RenderError::Backend(_))));
}

#[test]
fn chunking_covers_every_row_for_awkward_widths() {
    // Width that doesn't divide CHUNK_PIXELS, height spanning several chunks.
    let (w, h) = (1531u32, 97u32);
    let img = LinearImage::new(w, h, vec![65535; (w * h * 3) as usize]).unwrap();
    let out = CpuRenderer
        .render(
            &RenderPlan::new(vec![]),
            &img,
            PixelFormat::Rgb8,
            &NeverCancel,
        )
        .unwrap();
    assert!(out.data().iter().all(|&v| v == 255));
}

#[test]
fn consecutive_gains_are_fused() {
    let plan = RenderPlan::new(vec![
        Stage::WhiteBalance {
            gains: [2.0, 1.0, 0.5],
        },
        Stage::Exposure { multiplier: 2.0 },
        Stage::Saturation { factor: 1.0 },
    ]);
    let kernels = kernels::compile(&plan);
    assert_eq!(kernels.len(), 2);
    assert!(matches!(kernels[0], kernels::Kernel::Gain([4.0, 2.0, 1.0])));
}
