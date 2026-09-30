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
            &RenderPlan::from_recipe(recipe, None),
            img,
            PixelFormat::Rgb8,
            &NeverCancel,
        )
        .unwrap()
}

/// Reference renderer: scalar ops, whole image stage by stage, no LUTs, fusion or
/// chunking.
fn reference(plan: &RenderPlan, img: &LinearImage) -> Vec<u8> {
    // The surroundings map and the detail stage's luminance come from the source
    // after the gains before them.
    let mut gains = [1.0f32; 3];
    let mut base = None;
    let mut detail_gains = None;
    let mut dehaze: Option<ops::dehaze::DehazeModel> = None;
    for stage in &plan.stages {
        let scene = || ops::scene::SceneMap::build(img, gains);
        match *stage {
            Stage::WhiteBalance { gains: g } => (0..3).for_each(|c| gains[c] *= g[c]),
            Stage::Exposure { multiplier } => gains.iter_mut().for_each(|g| *g *= multiplier),
            Stage::Dehaze { amount } => {
                dehaze = Some(ops::dehaze::DehazeModel::build(&scene(), amount));
            }
            Stage::Tone { .. } => {
                base = Some(ops::tone::ToneBase::from_scene(&scene(), dehaze.as_ref()));
            }
            Stage::Detail { params } => {
                if params.needs_base() {
                    base = Some(ops::tone::ToneBase::from_scene(&scene(), dehaze.as_ref()));
                }
                detail_gains = Some(gains);
            }
            _ => {}
        }
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut image: Vec<f32> = img.data().iter().map(|&v| f32::from(v) / 65535.0).collect();
    for stage in &plan.stages {
        if let Stage::Detail { params } = *stage {
            let g = detail_gains.unwrap();
            let mut log_y = vec![0.0f32; w * h];
            for (k, (px, out)) in img
                .data()
                .as_chunks::<3>()
                .0
                .iter()
                .zip(&mut log_y)
                .enumerate()
            {
                let mut y = ops::scene::luma([0, 1, 2].map(|c| f32::from(px[c]) / 65535.0 * g[c]));
                if let Some(d) = &dehaze {
                    y = d.apply_luminance(y, d.transmission_at(k % w, k / w, w, h, y));
                }
                *out = ops::detail::fast_log2(y);
            }
            // The colour noise map, built from the source like the luminance.
            let chroma = (params.noise != 0.0).then(|| {
                let unit = ops::scene::SceneMap::unit_sized(img, ops::noise::CHROMA_MAP_LONG_EDGE);
                ops::noise::ChromaMap::build(&unit, g, dehaze.as_ref(), &params.noise())
            });
            ops::detail::apply_reference(
                &mut image,
                &log_y,
                chroma.as_ref(),
                w,
                h,
                base.as_ref(),
                &params,
            );
            continue;
        }
        for (k, px) in image.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            let rgb = *px;
            *px = match *stage {
                Stage::Detail { .. } => unreachable!("whole-image stage handled above"),
                Stage::Vignette { amount } => {
                    let dx = ((k % w) as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                    let dy = ((k / w) as f32 + 0.5) / h as f32 * 2.0 - 1.0;
                    let g = ops::finishing::vignette_stops(amount, dx * dx, dy * dy).exp2();
                    rgb.map(|c| c * g)
                }
                Stage::Grain { amount } => {
                    let scale = ops::finishing::grain_scale(w, h);
                    ops::finishing::apply_grain(rgb, amount, k % w, k / w, scale)
                }
                Stage::Dehaze { .. } => {
                    let d = dehaze.as_ref().unwrap();
                    let t = d.transmission_at(k % w, k / w, w, h, ops::scene::luma(rgb));
                    d.apply(rgb, t)
                }
                Stage::Tone { params } => {
                    let [wr, wg, wb] = image_core::color::REC709_LUMA;
                    let log_y = (rgb[0] * wr + rgb[1] * wg + rgb[2] * wb).max(1e-6).log2();
                    let d = base
                        .as_ref()
                        .map_or(0.0, |b| b.stops_at(k % w, k / w, w, h, log_y));
                    ops::tone::apply(rgb, d, &params)
                }
                Stage::WhiteBalance { gains } => {
                    [rgb[0] * gains[0], rgb[1] * gains[1], rgb[2] * gains[2]]
                }
                Stage::Exposure { multiplier } => rgb.map(|c| c * multiplier),
                Stage::Contrast { gamma } => rgb.map(|c| ops::contrast::apply(c, gamma)),
                Stage::BaseCurve => rgb.map(ops::look::standard),
                Stage::Saturation { factor } => ops::saturation::apply(rgb, factor),
                Stage::Vibrance { amount } => ops::vibrance::apply(rgb, amount),
                Stage::ColourMixer { bands } => {
                    ops::colour_mixer::apply(rgb, &ops::colour_mixer::MixerTable::new(&bands))
                }
            };
        }
    }
    image
        .iter()
        .map(|&c| (color::linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8)
        .collect()
}

#[test]
fn matches_scalar_reference_for_all_stages() {
    let img = chart();
    let recipe = EditRecipe {
        exposure: 0.6,
        contrast: 45.0,
        highlights: -40.0,
        shadows: 50.0,
        whites: 20.0,
        blacks: -30.0,
        temperature: -35.0,
        saturation: 30.0,
        texture: 60.0,
        clarity: 45.0,
        dehaze: 50.0,
        noise_reduction: 70.0,
        vignette: -40.0,
        grain: 30.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
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
    let flat = EditRecipe {
        look: crate::Look::Flat,
        ..Default::default()
    };
    let out = render(&flat, &img);
    for (i, px) in out.data().as_chunks::<3>().0.iter().enumerate() {
        assert_eq!(*px, [i as u8; 3]);
    }
}

#[test]
fn rgba_output_is_opaque() {
    let img = chart();
    let out = CpuRenderer
        .render(
            &RenderPlan::from_recipe(&EditRecipe::default(), None),
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
            look: crate::Look::Flat,
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
fn detail_matches_the_whole_image_reference_across_chunks() {
    // Many chunks of a few rows each, and a blur radius (4 at this size) that reaches
    // well past a chunk: every chunk must read its neighbours' rows.
    let (w, h) = (2600u32, 180u32);
    let data: Vec<u16> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let v =
                (((x * 37 + y * 91) % 211) as u16) * 150 + ((x / 40 + y / 30) % 3) as u16 * 9000;
            [v, v / 2 + 3000, v / 3 + 1000]
        })
        .collect();
    let img = LinearImage::new(w, h, data).unwrap();
    let recipe = EditRecipe {
        exposure: 0.5,
        texture: -70.0,
        clarity: 80.0,
        dehaze: -40.0,
        noise_reduction: 100.0,
        look: crate::Look::Flat,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
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
fn cropped_and_straightened_renders_frame_the_source_first() {
    use crate::geometry::{self, AspectRatio, CropRect, Geometry};
    let img = chart();
    let geometry = Geometry {
        straighten: 4.0,
        crop: CropRect {
            x: 0.1,
            y: 0.2,
            w: 0.6,
            h: 0.5,
        },
        aspect: AspectRatio::Free,
        ..Geometry::default()
    };
    let recipe = EditRecipe {
        exposure: 0.5,
        vignette: -40.0,
        geometry: Some(geometry),
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
    let out = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    assert_eq!(
        (out.width(), out.height()),
        geometry.output_size(img.width(), img.height())
    );
    // The same as rendering the framed source without geometry.
    let framed = geometry::resample(&img, &geometry);
    let plain = RenderPlan {
        geometry: None,
        ..plan.clone()
    };
    let expected = CpuRenderer
        .render(&plain, &framed, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    assert_eq!(out.data(), expected.data());
}

#[test]
fn chromatic_aberration_alone_frames_the_source_first() {
    use crate::chromatic::ChromaticAberration;
    use crate::geometry::{self, Geometry};
    let img = chart();
    let ca = ChromaticAberration {
        red: [0.002, 0.0],
        blue: [-0.001, 0.0005],
    };
    let recipe = EditRecipe {
        exposure: 0.3,
        chromatic_aberration: Some(ca),
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
    assert_eq!(plan.chromatic_aberration, Some(ca));
    let out = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    assert_eq!((out.width(), out.height()), (img.width(), img.height()));
    let framed = geometry::resample_corrected(&img, &Geometry::default(), Some(&ca));
    let plain = RenderPlan {
        chromatic_aberration: None,
        ..plan.clone()
    };
    let expected = CpuRenderer
        .render(&plain, &framed, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    assert_eq!(out.data(), expected.data());
    // Measured as nothing: no framing at all.
    let none = EditRecipe {
        chromatic_aberration: Some(ChromaticAberration::default()),
        ..Default::default()
    };
    assert_eq!(
        RenderPlan::from_recipe(&none, None).chromatic_aberration,
        None
    );
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
    let kernels = kernels::compile(&plan, &chart());
    assert_eq!(kernels.len(), 2);
    assert!(matches!(kernels[0], kernels::Kernel::Gain([4.0, 2.0, 1.0])));
}

#[test]
fn the_cached_surroundings_map_never_leaks_between_images() {
    // Two different images of the same size, rendered one after the other with the
    // same shadows setting: each must match its own reference, not the other's map.
    let recipe = EditRecipe {
        shadows: 80.0,
        highlights: -50.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
    let a = chart();
    let b = LinearImage::new(96, 64, a.data().iter().rev().copied().collect()).unwrap();
    for img in [&a, &b, &a] {
        let fast = CpuRenderer
            .render(&plan, img, PixelFormat::Rgb8, &NeverCancel)
            .unwrap();
        let slow = reference(&plan, img);
        let max_diff = fast
            .data()
            .iter()
            .zip(&slow)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap();
        assert!(max_diff <= 1, "max diff {max_diff}");
    }
}
