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
    let (w, h) = (img.width() as usize, img.height() as usize);
    // Masks, over the whole image (no geometry in these plans).
    let masks = plan.stages.iter().find_map(|s| match s {
        Stage::Local { masks } => Some(crate::masks::LocalField::new(
            masks,
            crate::masks::Frame::whole(w as u32, h as u32),
        )),
        _ => None,
    });
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
                if params.needs_base() || masks.as_ref().is_some_and(|m| m.has_clarity()) {
                    base = Some(ops::tone::ToneBase::from_scene(&scene(), dehaze.as_ref()));
                }
                detail_gains = Some(gains);
            }
            _ => {}
        }
    }
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
            let clarity = masks
                .as_ref()
                .map(|m| move |x: usize, y: usize| m.clarity(x, y, w, h));
            ops::detail::apply_reference(
                &mut image,
                &log_y,
                chroma.as_ref(),
                (w, h),
                base.as_ref(),
                &params,
                clarity.as_ref().map(|f| f as &dyn Fn(usize, usize) -> f32),
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
                    // Masks' Exposure is left out of the surroundings, as in the kernel.
                    let s = masks.as_ref().map_or(0.0, |m| m.stops(k % w, k / w, w, h));
                    let d = base
                        .as_ref()
                        .map_or(0.0, |b| b.stops_at(k % w, k / w, w, h, log_y - s) - s);
                    ops::tone::apply(rgb, d, &params)
                }
                Stage::WhiteBalance { gains } => {
                    [rgb[0] * gains[0], rgb[1] * gains[1], rgb[2] * gains[2]]
                }
                Stage::Exposure { multiplier } => rgb.map(|c| c * multiplier),
                Stage::Local { .. } => {
                    let g = masks.as_ref().unwrap().log2_gains(k % w, k / w, w, h);
                    [
                        rgb[0] * g[0].exp2(),
                        rgb[1] * g[1].exp2(),
                        rgb[2] * g[2].exp2(),
                    ]
                }
                Stage::Contrast { gamma } => rgb.map(|c| ops::contrast::apply(c, gamma)),
                Stage::BaseCurve => rgb.map(ops::look::standard),
                Stage::PointCurve {
                    rgb: ref master,
                    ref channels,
                } => {
                    let mut out = rgb;
                    for (v, channel) in out.iter_mut().zip(channels.iter()) {
                        let tone = color::linear_to_srgb(v.clamp(0.0, 1.0));
                        *v = color::srgb_to_linear(channel.eval(master.eval(tone)));
                    }
                    out
                }
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
        point_curve: Some(crate::ops::point_curve::PointCurve::new(&[
            [0.05, 0.0],
            [0.3, 0.24],
            [0.7, 0.8],
            [1.0, 0.95],
        ])),
        channel_curves: Some(crate::ops::point_curve::ChannelCurves {
            blue: Some(crate::ops::point_curve::PointCurve::new(&[
                [0.0, 0.08],
                [1.0, 0.9],
            ])),
            ..Default::default()
        }),
        masks: vec![
            linear_mask(1, [0.5, 0.0], [0.5, 0.6], -0.8, 30.0, 40.0),
            linear_mask(2, [0.0, 0.5], [0.7, 0.5], 0.5, -20.0, -30.0),
            crate::masks::Mask {
                id: 3,
                hidden: false,
                shape: crate::masks::MaskShape::Radial {
                    centre: [0.4, 0.6],
                    radius: [0.3, 0.15],
                    angle: 30.0,
                    feather: 60.0,
                },
                invert: true,
                adjustments: crate::masks::LocalAdjustments {
                    exposure: -0.6,
                    warmth: 15.0,
                    clarity: 25.0,
                },
            },
            crate::masks::Mask {
                id: 4,
                hidden: false,
                shape: crate::masks::MaskShape::Brush {
                    strokes: vec![
                        crate::masks::Stroke {
                            erase: false,
                            size: 0.06,
                            feather: 60.0,
                            flow: 80.0,
                            points: vec![[0.1, 0.8], [0.5, 0.7], [0.9, 0.85]],
                        },
                        crate::masks::Stroke {
                            erase: true,
                            size: 0.03,
                            feather: 30.0,
                            flow: 100.0,
                            points: vec![[0.5, 0.6], [0.5, 0.9]],
                        },
                    ],
                },
                invert: false,
                adjustments: crate::masks::LocalAdjustments {
                    exposure: 0.7,
                    warmth: -25.0,
                    clarity: 30.0,
                },
            },
        ],
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

fn linear_mask(
    id: u32,
    start: [f32; 2],
    end: [f32; 2],
    exposure: f32,
    warmth: f32,
    clarity: f32,
) -> crate::masks::Mask {
    crate::masks::Mask {
        id,
        hidden: false,
        shape: crate::masks::MaskShape::Linear { start, end },
        invert: false,
        adjustments: crate::masks::LocalAdjustments {
            exposure,
            warmth,
            clarity,
        },
    }
}

#[test]
fn masks_change_only_what_they_cover() {
    let img = chart();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let plain = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    // Darker at the top, fading out by 40 % of the height.
    let masked = EditRecipe {
        masks: vec![linear_mask(1, [0.5, 0.0], [0.5, 0.4], -1.0, 0.0, 0.0)],
        ..plain.clone()
    };
    let render = |r: &EditRecipe| {
        let plan = RenderPlan::from_recipe(r, None);
        CpuRenderer
            .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
            .unwrap()
    };
    let (a, b) = (render(&plain), render(&masked));
    let row_mean = |img: &image_core::OutputImage, y: usize| {
        let row = &img.data()[y * w * 3..(y + 1) * w * 3];
        row.iter().map(|&v| f64::from(v)).sum::<f64>() / row.len() as f64
    };
    // The chart's top rows are a grey ramp: darker under the mask...
    assert!(row_mean(&b, 2) < row_mean(&a, 2) - 20.0);
    // ...and the bottom half untouched, to the byte.
    let half = h / 2 * w * 3;
    assert_eq!(&a.data()[half..], &b.data()[half..]);
    // Clarity alone still runs the detail stage.
    let clarity = EditRecipe {
        masks: vec![linear_mask(1, [0.5, 0.0], [0.5, 0.4], 0.0, 0.0, 80.0)],
        ..plain.clone()
    };
    let plan = RenderPlan::from_recipe(&clarity, None);
    assert!(
        plan.stages
            .iter()
            .any(|s| matches!(s, Stage::Detail { .. }))
    );
    assert_ne!(render(&clarity).data(), a.data());
    // A hidden mask is kept but renders nothing.
    let hidden = EditRecipe {
        masks: vec![crate::masks::Mask {
            hidden: true,
            ..linear_mask(1, [0.5, 0.0], [0.5, 0.4], -1.0, 0.0, 0.0)
        }],
        ..plain.clone()
    };
    assert_eq!(
        RenderPlan::from_recipe(&hidden, None),
        RenderPlan::from_recipe(&plain, None)
    );
    assert_eq!(render(&hidden).data(), a.data());
    // A mask without adjustments renders nothing.
    let idle = EditRecipe {
        masks: vec![linear_mask(1, [0.5, 0.0], [0.5, 0.4], 0.0, 0.0, 0.0)],
        ..plain.clone()
    };
    assert_eq!(
        RenderPlan::from_recipe(&idle, None),
        RenderPlan::from_recipe(&plain, None)
    );
}

#[test]
fn masks_stay_on_the_picture_when_cropped() {
    use crate::geometry::{AspectRatio, CropRect, Geometry};
    let img = chart();
    let mask = vec![linear_mask(1, [0.5, 0.0], [0.5, 0.5], -1.0, 25.0, 0.0)];
    let crop = CropRect {
        x: 0.0,
        y: 0.25,
        w: 1.0,
        h: 0.5,
    };
    let geometry = Geometry {
        crop,
        aspect: AspectRatio::Free,
        ..Default::default()
    };
    let recipe = |masks: Vec<crate::masks::Mask>, geometry: Option<Geometry>| EditRecipe {
        sharpening: 0.0,
        masks,
        geometry,
        ..Default::default()
    };
    let render = |r: &EditRecipe| {
        CpuRenderer
            .render(
                &RenderPlan::from_recipe(r, None),
                &img,
                PixelFormat::Rgb8,
                &NeverCancel,
            )
            .unwrap()
    };
    // The cropped render equals the middle of the whole render.
    let whole = render(&recipe(mask.clone(), None));
    let cropped = render(&recipe(mask, Some(geometry)));
    let (w, h) = (img.width() as usize, img.height() as usize);
    let top = h / 4;
    let rows = cropped.height() as usize;
    assert_eq!(cropped.width() as usize, w);
    let expected = &whole.data()[top * w * 3..(top + rows) * w * 3];
    let diff = expected
        .iter()
        .zip(cropped.data())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(diff <= 1, "{diff}");
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
    let kernels = kernels::compile(&plan, &chart(), crate::masks::Frame::whole(1, 1));
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
