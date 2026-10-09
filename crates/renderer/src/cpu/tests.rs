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
    // The photo's white for Whites (ADR 0073), as the tone kernel measures it.
    let mut white: Option<f32> = None;
    let mut detail_gains = None;
    let mut dehaze: Option<ops::dehaze::DehazeModel> = None;
    let (w, h) = (img.width() as usize, img.height() as usize);
    // Masks, over the whole image (no geometry in these plans).
    let masks = plan.stages.iter().find_map(|s| match s {
        Stage::Local { masks } => Some(crate::masks::LocalField::new(
            masks,
            crate::masks::Frame::whole(w as u32, h as u32),
            &plan.generated_masks,
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
            Stage::Tone { params } => {
                base = Some(ops::tone::ToneBase::from_scene(&scene(), dehaze.as_ref()));
                white = (params.whites != 0.0 && params.whites_relative)
                    .then(|| ops::tone::white_point_stops(img, gains));
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
                    let params = ops::tone::ToneParams {
                        white_stops: white.unwrap_or(0.0),
                        ..params
                    };
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
                    ref parametric,
                    rgb: ref master,
                    ref channels,
                } => {
                    let mut out = rgb;
                    for (v, channel) in out.iter_mut().zip(channels.iter()) {
                        let tone = color::linear_to_srgb(v.clamp(0.0, 1.0));
                        *v =
                            color::srgb_to_linear(channel.eval(master.eval(parametric.eval(tone))));
                    }
                    out
                }
                Stage::Saturation { factor } => ops::saturation::apply(rgb, factor),
                Stage::Calibration { ref calibration } => ops::calibration::apply(
                    rgb,
                    &ops::calibration::CalibrationTable::new(calibration),
                ),
                Stage::ColourGrading { ref grading } => {
                    ops::colour_grading::apply(rgb, &ops::colour_grading::GradeTable::new(grading))
                }
                Stage::Vibrance { amount } => ops::vibrance::apply(rgb, amount),
                Stage::ColourMixer { bands } => {
                    ops::colour_mixer::apply(rgb, &ops::colour_mixer::MixerTable::new(&bands))
                }
            };
        }
    }
    image
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|px| {
            if plan.compresses_output() {
                image_core::gamut::compress(*px, &image_core::gamut::ON_OUTPUT)
            } else {
                *px
            }
        })
        .map(|c| (color::linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8)
        .collect()
}

#[test]
fn matches_scalar_reference_for_all_stages() {
    let img = chart();
    let recipe = every_stage_recipe();
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
fn the_colour_mixer_is_the_black_and_white_mix() {
    // Red and blue patches side by side, made black and white. Brightening reds in the
    // mixer lightens the red patch's grey and leaves the blue one (ADR 0051: a
    // Lightroom preset's B&W mix maps onto the mixer's luminance).
    let (w, h) = (8u32, 2u32);
    let px = |x: u32| {
        if x < w / 2 {
            [0.40f32, 0.06, 0.05]
        } else {
            [0.05, 0.08, 0.40]
        }
    };
    let data: Vec<u16> = (0..w * h)
        .flat_map(|i| px(i % w).map(|v| (v * 65535.0) as u16))
        .collect();
    let img = LinearImage::new(w, h, data).unwrap();
    let mono = EditRecipe {
        saturation: -100.0,
        sharpening: 0.0,
        ..Default::default()
    };
    let redder = EditRecipe {
        mixer: Some(crate::ColourMixer {
            red: crate::HslShift {
                luminance: 60.0,
                ..Default::default()
            },
            ..Default::default()
        }),
        ..mono.clone()
    };
    let (a, b) = (render(&mono, &img), render(&redder, &img));
    let grey = |o: &OutputImage, x: usize| {
        let p = &o.data()[x * 3..x * 3 + 3];
        assert!(
            p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1,
            "{p:?}"
        );
        i32::from(p[0])
    };
    assert!(
        grey(&b, 1) > grey(&a, 1) + 10,
        "red: {} -> {}",
        grey(&a, 1),
        grey(&b, 1)
    );
    assert!((grey(&b, 6) - grey(&a, 6)).abs() <= 2, "blue moved");
}

#[test]
fn black_and_white_photos_can_be_toned() {
    // Split toning on black and white (ADR 0052): grading runs after Saturation -100,
    // so the dark end turns blue and the light end warm, not grey.
    // A grey ramp in linear light, from near black to bright.
    let w = 64u32;
    let data: Vec<u16> = (0..w)
        .flat_map(|x| {
            let v = (0.002 + 0.8 * (x as f32 / (w - 1) as f32).powi(2)) * 65535.0;
            [v as u16; 3]
        })
        .collect();
    let img = LinearImage::new(w, 1, data).unwrap();
    let toned = EditRecipe {
        saturation: -100.0,
        sharpening: 0.0,
        colour_grading: Some(crate::ops::colour_grading::ColourGrading {
            shadows: crate::ops::colour_grading::GradeWheel {
                hue: 220.0,
                saturation: 60.0,
                luminance: 0.0,
            },
            highlights: crate::ops::colour_grading::GradeWheel {
                hue: 40.0,
                saturation: 60.0,
                luminance: 0.0,
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let out = render(&toned, &img);
    let w = w as usize;
    let px = |x: usize| {
        let p = &out.data()[x * 3..x * 3 + 3];
        (i32::from(p[0]), i32::from(p[2]))
    };
    let (r_dark, b_dark) = px(w / 10);
    let (r_light, b_light) = px(w * 9 / 10);
    assert!(b_dark > r_dark + 3, "dark end not blue: {r_dark} {b_dark}");
    assert!(
        r_light > b_light + 3,
        "light end not warm: {r_light} {b_light}"
    );
}

#[test]
fn calibration_moves_colours_and_keeps_greys() {
    // The classic teal-and-orange calibration (ADR 0053): blue toward cyan and
    // stronger. Greys stay grey; a blue sky turns toward teal.
    let level = |v: f32| (v * 65535.0) as u16;
    let data: Vec<u16> = [[0.18, 0.18, 0.18], [0.6, 0.6, 0.6], [0.05, 0.1, 0.5]]
        .iter()
        .flat_map(|p: &[f32; 3]| p.map(level))
        .collect();
    let img = LinearImage::new(3, 1, data).unwrap();
    let plain = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    let calibrated = EditRecipe {
        calibration: Some(crate::ops::calibration::Calibration {
            blue_hue: -100.0,
            blue_saturation: 50.0,
            ..Default::default()
        }),
        ..plain.clone()
    };
    let (a, b) = (render(&plain, &img), render(&calibrated, &img));
    let px = |o: &OutputImage, x: usize| {
        let p = &o.data()[x * 3..x * 3 + 3];
        [i32::from(p[0]), i32::from(p[1]), i32::from(p[2])]
    };
    for x in 0..2 {
        let (before, after) = (px(&a, x), px(&b, x));
        assert!(
            before.iter().zip(after).all(|(p, q)| (p - q).abs() <= 1),
            "grey moved: {before:?} {after:?}"
        );
    }
    let (before, after) = (px(&a, 2), px(&b, 2));
    // Toward cyan: more green against the blue.
    assert!(
        after[1] - after[2] > before[1] - before[2] + 5,
        "blue not turned toward cyan: {before:?} {after:?}"
    );
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
    let framed = geometry::resample_corrected(&img, &Geometry::default(), Some(&ca), None);
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

/// A profile of 16 knots whose distortion and vignetting are `d(r)` and `v(r)`.
fn lens_profile(
    w: u32,
    h: u32,
    d: Option<fn(f32) -> f32>,
    v: Option<fn(f32) -> f32>,
) -> crate::lens::LensCorrection {
    let knots: Vec<f32> = (0..16).map(|i| i as f32 / 15.0).collect();
    let curve = |f: fn(f32) -> f32| knots.iter().map(|&r| f(r)).collect::<Vec<_>>();
    let (dc, vc) = (d.map(curve), v.map(curve));
    let _ = (w, h);
    crate::lens::LensCorrection::new(&knots, dc.as_deref(), vc.as_deref()).unwrap()
}

#[test]
fn lens_vignetting_is_lifted_to_an_even_photo_even_above_the_sensors_white() {
    // A wall the lens darkened to half towards the corners: the centre near the
    // sensor's white, so the lifted corners go past it.
    let (w, h) = (300u32, 200u32);
    let falloff = |r: f32| 1.0 - 0.5 * r * r;
    let lens = lens_profile(w, h, None, Some(falloff));
    let half_diagonal = (w as f32).hypot(h as f32) / 2.0;
    let data: Vec<u16> = (0..w * h)
        .flat_map(|k| {
            let (x, y) = (
                (k % w) as f32 + 0.5 - w as f32 / 2.0,
                (k / w) as f32 + 0.5 - h as f32 / 2.0,
            );
            let b = falloff(x.hypot(y) / half_diagonal);
            [(60000.0 * b) as u16; 3]
        })
        .collect();
    let img = LinearImage::new(w, h, data).unwrap();
    // Exposure down a stop, so the evened wall (60000 everywhere) shows below white.
    let recipe = EditRecipe {
        exposure: -1.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None).with_lens(Some(lens));
    assert!((lens.headroom() - 2.0).abs() < 0.01);
    let out = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    let px = |x: u32, y: u32| out.data()[((y * w + x) * 3) as usize];
    let centre = px(w / 2, h / 2);
    for (x, y) in [
        (1, 1),
        (w - 2, 1),
        (1, h - 2),
        (w - 2, h - 2),
        (w / 2, 2),
        (2, h / 2),
    ] {
        let v = px(x, y);
        assert!(v.abs_diff(centre) <= 2, "({x}, {y}): {v} against {centre}");
    }
    // Without the profile, the corners are far darker.
    let plain = CpuRenderer
        .render(
            &RenderPlan::from_recipe(&recipe, None),
            &img,
            PixelFormat::Rgb8,
            &NeverCancel,
        )
        .unwrap();
    assert!(plain.data()[((w + 1) * 3) as usize] < centre - 30);
}

#[test]
fn lens_distortion_straightens_a_line_the_lens_bent() {
    // A barrel lens bent a horizontal line near the top into an arc: drawn where the
    // lens put it, the line is lower at the sides than in the middle.
    let (w, h) = (400u32, 300u32);
    let barrel = |r: f32| 1.0 - 0.06 * r * r;
    let lens = lens_profile(w, h, Some(barrel), None);
    let line_y = 40.0;
    let mut data = vec![2000u16; (w * h * 3) as usize];
    for i in 0..4000 {
        // The ideal line's points, put where the lens puts them.
        let x = i as f32 / 4000.0 * w as f32;
        let (sx, sy) = lens.at(w as f32, h as f32).distorted(x, line_y);
        let (px, py) = (sx.round() as i64, sy.round() as i64);
        if (0..w as i64).contains(&px) && (0..h as i64).contains(&py) {
            let k = ((py as u32 * w + px as u32) * 3) as usize;
            data[k..k + 3].copy_from_slice(&[50000; 3]);
        }
    }
    let img = LinearImage::new(w, h, data).unwrap();
    let plan = RenderPlan::from_recipe(&EditRecipe::default(), None).with_lens(Some(lens));
    let out = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    // In the output the line is straight and where an ideal lens would have put it
    // (a barrel correction needs no scaling to fill the frame).
    let row_of = |x: u32| {
        (0..h)
            .max_by_key(|&y| out.data()[((y * w + x) * 3) as usize])
            .unwrap() as f32
    };
    let rows = [20, 100, 200, 300, 380].map(row_of);
    for r in rows {
        assert!((r + 0.5 - line_y).abs() <= 1.0, "rows {rows:?}");
    }
    // Uncorrected, the bent line is lower at the sides.
    let plain = CpuRenderer
        .render(
            &RenderPlan::from_recipe(&EditRecipe::default(), None),
            &img,
            PixelFormat::Rgb8,
            &NeverCancel,
        )
        .unwrap();
    let plain_row = |x: u32| {
        (0..h)
            .max_by_key(|&y| plain.data()[((y * w + x) * 3) as usize])
            .unwrap()
    };
    assert!(plain_row(20) > plain_row(200) + 3);
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
        parts: Vec::new(),
        density: 100.0,
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
fn combined_masks_render_their_combination() {
    let img = chart();
    let w = img.width() as usize;
    let plain = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    let render = |r: &EditRecipe| {
        let plan = RenderPlan::from_recipe(r, None);
        CpuRenderer
            .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
            .unwrap()
    };
    // Darker across the top 40 %, less the left half (a hard-edged linear gradient
    // there: fully on left of x = 0.49, off right of 0.5).
    let darker = linear_mask(1, [0.5, 0.0], [0.5, 0.4], -1.0, 0.0, 0.0);
    let left = crate::masks::MaskShape::Linear {
        start: [0.49, 0.5],
        end: [0.5, 0.5],
    };
    let combined = |mode, density| EditRecipe {
        masks: vec![crate::masks::Mask {
            parts: vec![crate::masks::MaskPart {
                mode,
                shape: left.clone(),
            }],
            density,
            ..darker.clone()
        }],
        ..plain.clone()
    };
    let (a, only) = (
        render(&plain),
        render(&EditRecipe {
            masks: vec![darker.clone()],
            ..plain.clone()
        }),
    );
    let subtracted = render(&combined(crate::masks::Combine::Subtract, 100.0));
    let intersected = render(&combined(crate::masks::Combine::Intersect, 100.0));
    let half = render(&combined(crate::masks::Combine::Subtract, 50.0));
    // Mean of the top row's left or right quarter.
    let quarter = |img: &image_core::OutputImage, right: bool| {
        let row = &img.data()[..w * 3];
        let q = if right {
            &row[w * 9 / 4..]
        } else {
            &row[..w * 3 / 4]
        };
        q.iter().map(|&v| f64::from(v)).sum::<f64>() / q.len() as f64
    };
    let (plain_l, plain_r) = (quarter(&a, false), quarter(&a, true));
    // Subtracting the left leaves it alone and darkens the right as the mask alone.
    assert_eq!(quarter(&subtracted, false), plain_l);
    assert_eq!(quarter(&subtracted, true), quarter(&only, true));
    assert!(quarter(&subtracted, true) < plain_r - 10.0);
    // Intersecting is the other way round.
    assert_eq!(quarter(&intersected, true), plain_r);
    assert_eq!(quarter(&intersected, false), quarter(&only, false));
    // Half the density darkens half as much (in stops: between the two).
    let (h, full) = (quarter(&half, true), quarter(&subtracted, true));
    assert!(h < plain_r - 3.0 && h > full + 3.0, "{plain_r} {h} {full}");
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

#[test]
fn spots_are_applied_first_and_follow_the_photo() {
    // A dark dot on grey, healed from the right; with a quarter turn the dot is still
    // gone (spots are in the source's coordinates, applied before framing).
    let (w, h) = (80u32, 60u32);
    let data: Vec<u16> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let dot = (x as f32 - 20.0).hypot(y as f32 - 30.0) < 3.0;
            [if dot { 500 } else { 20000 }; 3]
        })
        .collect();
    let img = LinearImage::new(w, h, data).unwrap();
    let spot = crate::retouch::Spot {
        x: 20.5 / 80.0,
        y: 30.5 / 60.0,
        source_x: 50.5 / 80.0,
        source_y: 30.5 / 60.0,
        radius: 5.0 / 80.0,
        ..Default::default()
    };
    let plain = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    let healed = EditRecipe {
        spots: vec![spot],
        ..plain.clone()
    };
    let level = |o: &OutputImage, x: usize, y: usize| o.data()[(y * o.width() as usize + x) * 3];
    let before = render(&plain, &img);
    let after = render(&healed, &img);
    assert!(level(&before, 20, 30) + 20 < level(&after, 20, 30));
    assert_eq!(level(&after, 20, 30), level(&after, 60, 10));
    // Turned a quarter: the dot's place moves, and it is still healed.
    let turned = EditRecipe {
        geometry: Some(crate::Geometry {
            rotation: 1,
            ..Default::default()
        }),
        ..healed.clone()
    };
    let out = render(&turned, &img);
    assert_eq!((out.width(), out.height()), (60, 80));
    let min = out.data().iter().copied().min().unwrap();
    assert!(
        min + 3 >= level(&after, 60, 10),
        "a dark pixel is left: {min}"
    );
    // Moving the spot away renders the dot again (no stale cache).
    let moved = EditRecipe {
        spots: vec![crate::retouch::Spot {
            x: 60.5 / 80.0,
            source_x: 70.5 / 80.0,
            ..spot
        }],
        ..plain.clone()
    };
    assert_eq!(level(&render(&moved, &img), 20, 30), level(&before, 20, 30));
    assert_eq!(render(&healed, &img).data(), after.data());
}

#[test]
fn sixteen_bit_output_matches_eight_bit_and_keeps_finer_steps() {
    // A smooth dark ramp: 8-bit output bands it into few levels; 16-bit keeps them.
    let w = 512u32;
    let data: Vec<u16> = (0..w).flat_map(|x| [(x * 8) as u16; 3]).collect();
    let img = LinearImage::new(w, 1, data).unwrap();
    let recipe = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    let plan = RenderPlan::from_recipe(&recipe, None);
    let eight = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb8, &NeverCancel)
        .unwrap();
    let sixteen = CpuRenderer
        .render(&plan, &img, PixelFormat::Rgb16, &NeverCancel)
        .unwrap();
    assert_eq!(sixteen.data().len(), eight.data().len() * 2);
    let deep = sixteen.samples16().unwrap();
    for (a, b) in eight.data().iter().zip(&deep) {
        assert!(
            (f32::from(*a) - f32::from(*b) / 257.0).abs() <= 0.6,
            "{a} vs {b}"
        );
    }
    let levels = |v: Vec<u32>| {
        let mut v = v;
        v.dedup();
        v.len()
    };
    let l8 = levels(
        eight
            .data()
            .iter()
            .step_by(3)
            .map(|&v| u32::from(v))
            .collect(),
    );
    let l16 = levels(deep.iter().step_by(3).map(|&v| u32::from(v)).collect());
    assert!(l16 > l8 * 4, "16-bit {l16} levels, 8-bit {l8}");
}

#[test]
fn removals_fill_first_follow_the_photo_and_are_cached() {
    // A dark post on grey, painted over; with a quarter turn it is still gone
    // (removals are in the source's coordinates, filled before framing).
    let (w, h) = (120u32, 90u32);
    let data: Vec<u16> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let post = (40..44).contains(&x) && (20..70).contains(&y);
            [if post { 500 } else { 20000 }; 3]
        })
        .collect();
    let img = LinearImage::new(w, h, data).unwrap();
    let removal = crate::remove::Removal {
        strokes: vec![crate::masks::brush::Stroke {
            erase: false,
            size: 5.0 / 120f32.hypot(90.0),
            feather: 0.0,
            flow: 100.0,
            points: vec![[42.0 / 120.0, 20.0 / 90.0], [42.0 / 120.0, 70.0 / 90.0]],
        }],
    };
    let plain = EditRecipe {
        sharpening: 0.0,
        ..Default::default()
    };
    let removed = EditRecipe {
        removals: vec![removal.clone()],
        ..plain.clone()
    };
    let level = |o: &OutputImage, x: usize, y: usize| o.data()[(y * o.width() as usize + x) * 3];
    let before = render(&plain, &img);
    let after = render(&removed, &img);
    assert!(level(&before, 42, 45) + 20 < level(&after, 42, 45));
    assert!(level(&after, 42, 45).abs_diff(level(&after, 90, 10)) <= 1);
    let turned = EditRecipe {
        geometry: Some(crate::Geometry {
            rotation: 1,
            ..Default::default()
        }),
        ..removed.clone()
    };
    let out = render(&turned, &img);
    assert_eq!((out.width(), out.height()), (90, 120));
    let min = out.data().iter().copied().min().unwrap();
    assert!(
        min + 3 >= level(&after, 90, 10),
        "a dark pixel is left: {min}"
    );
    // The same again (from the cache, and deterministic), and a spot after the
    // removal still applies on top of it.
    assert_eq!(render(&removed, &img).data(), after.data());
    let with_spot = EditRecipe {
        spots: vec![crate::retouch::Spot {
            kind: crate::retouch::SpotKind::Clone,
            x: 90.5 / 120.0,
            y: 45.5 / 90.0,
            source_x: 42.5 / 120.0,
            source_y: 45.5 / 90.0,
            radius: 3.0 / 120.0,
            ..Default::default()
        }],
        ..removed.clone()
    };
    let cloned = render(&with_spot, &img);
    assert!(level(&cloned, 90, 45).abs_diff(level(&after, 42, 45)) <= 1);
    // A cancelled render fails rather than caching a half fill.
    let moved = EditRecipe {
        removals: vec![crate::remove::Removal {
            strokes: vec![crate::masks::brush::Stroke {
                points: vec![[80.0 / 120.0, 30.0 / 90.0]],
                ..removal.strokes[0].clone()
            }],
        }],
        ..plain.clone()
    };
    let plan = RenderPlan::from_recipe(&moved, None);
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert!(matches!(
        CpuRenderer.render(&plan, &img, PixelFormat::Rgb8, &cancelled),
        Err(RenderError::Cancelled)
    ));
}

/// A recipe using every stage, global and local: the one renders are checked with.
fn every_stage_recipe() -> EditRecipe {
    EditRecipe {
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
        calibration: Some(crate::ops::calibration::Calibration {
            shadow_tint: 20.0,
            red_hue: 15.0,
            blue_hue: -40.0,
            blue_saturation: 30.0,
            ..Default::default()
        }),
        colour_grading: Some(crate::ops::colour_grading::ColourGrading {
            shadows: crate::ops::colour_grading::GradeWheel {
                hue: 200.0,
                saturation: 40.0,
                luminance: -10.0,
            },
            highlights: crate::ops::colour_grading::GradeWheel {
                hue: 35.0,
                saturation: 30.0,
                luminance: 5.0,
            },
            ..Default::default()
        }),
        parametric_curve: Some(crate::ops::parametric_curve::ParametricCurve {
            shadows: 30.0,
            highlights: -40.0,
            ..Default::default()
        }),
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
                parts: vec![crate::masks::MaskPart {
                    mode: crate::masks::Combine::Subtract,
                    shape: crate::masks::MaskShape::Linear {
                        start: [0.0, 0.2],
                        end: [0.3, 0.4],
                    },
                }],
                density: 70.0,
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
                parts: Vec::new(),
                density: 100.0,
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
    }
}

#[test]
fn a_window_is_exactly_that_part_of_the_whole_render() {
    // Every stage, cropped, straightened and turned, with a spot: windows anywhere
    // (corners, edges, odd sizes, the whole) match the whole render byte for byte.
    let img = chart();
    let recipe = EditRecipe {
        geometry: Some(crate::Geometry {
            straighten: 3.0,
            crop: crate::geometry::CropRect {
                x: 0.08,
                y: 0.1,
                w: 0.85,
                h: 0.8,
            },
            ..Default::default()
        }),
        spots: vec![crate::retouch::Spot {
            x: 0.3,
            y: 0.4,
            source_x: 0.6,
            source_y: 0.4,
            radius: 0.05,
            ..Default::default()
        }],
        ..every_stage_recipe()
    };
    for format in [PixelFormat::Rgb8, PixelFormat::Rgba8] {
        let plan = RenderPlan::from_recipe(&recipe, None);
        let whole = CpuRenderer
            .render(&plan, &img, format, &NeverCancel)
            .unwrap();
        let (ow, oh) = (whole.width(), whole.height());
        let bytes = format.bytes_per_pixel();
        for (x, y, w, h) in [
            (0, 0, 17, 9),
            (ow / 3, oh / 4, 41, 23),
            (ow - 13, oh - 7, 13, 7),
            (5, oh / 2, ow - 10, 1),
            (0, 0, ow, oh),
            // Larger than the output: clamped to it.
            (ow - 4, oh - 4, 100, 100),
        ] {
            let window = CpuRenderer
                .render_window(&plan, &img, format, (x, y, w, h), &NeverCancel)
                .unwrap();
            let (ww, wh) = (window.width() as usize, window.height() as usize);
            assert_eq!(
                (ww, wh),
                ((w.min(ow - x)) as usize, (h.min(oh - y)) as usize)
            );
            for r in 0..wh {
                let from = ((y as usize + r) * ow as usize + x as usize) * bytes;
                assert_eq!(
                    &window.data()[r * ww * bytes..(r + 1) * ww * bytes],
                    &whole.data()[from..from + ww * bytes],
                    "{format:?} window ({x}, {y}, {w}, {h}), row {r}"
                );
            }
        }
    }
}

/// Where a pixel of the upright picture comes from in the stored one, for each EXIF
/// Orientation, as the EXIF standard defines them (`w`, `h`: the stored size).
fn exif_source(o: u16, x: usize, y: usize, w: usize, h: usize) -> (usize, usize) {
    match o {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (y, h - 1 - x),
        7 => (w - 1 - y, h - 1 - x),
        8 => (w - 1 - y, x),
        _ => (x, y),
    }
}

/// `img` shown upright by EXIF Orientation `o`, as the decoders turn it.
fn upright(img: &LinearImage, o: u16) -> LinearImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let (dw, dh) = if o >= 5 { (h, w) } else { (w, h) };
    let mut data = Vec::with_capacity(dw * dh * 3);
    for y in 0..dh {
        for x in 0..dw {
            let (sx, sy) = exif_source(o, x, y, w, h);
            data.extend_from_slice(&img.data()[(sy * w + sx) * 3..(sy * w + sx) * 3 + 3]);
        }
    }
    LinearImage::new(dw as u32, dh as u32, data).unwrap()
}

#[test]
fn turns_match_exif_orientations() {
    use crate::geometry::Turn;
    let (w, h) = (6usize, 4usize);
    for o in 1..=8u16 {
        let t = Turn::from_exif(o);
        let (dw, dh) = if o >= 5 { (h, w) } else { (w, h) };
        // The turn takes each stored pixel's centre to where EXIF puts it upright.
        for y in 0..dh {
            for x in 0..dw {
                let (sx, sy) = exif_source(o, x, y, w, h);
                let p = t.point([(sx as f32 + 0.5) / w as f32, (sy as f32 + 0.5) / h as f32]);
                let expected = [(x as f32 + 0.5) / dw as f32, (y as f32 + 0.5) / dh as f32];
                assert!(
                    (p[0] - expected[0]).abs() < 1e-5 && (p[1] - expected[1]).abs() < 1e-5,
                    "orientation {o}"
                );
            }
        }
        assert!(t.after(t.inverse()).is_identity() && t.inverse().after(t).is_identity());
    }
}

#[test]
fn an_edit_made_on_a_file_as_stored_renders_the_same_on_it_upright() {
    use crate::geometry::{CropRect, Geometry, Turn};
    let stored = chart();
    // An edit from before version 30: turned, cropped, straightened, a gradient over
    // the top and a heal spot, all on the file as stored.
    let old = EditRecipe {
        exposure: 0.2,
        geometry: Some(Geometry {
            rotation: 1,
            straighten: 3.0,
            crop: CropRect {
                x: 0.1,
                y: 0.05,
                w: 0.8,
                h: 0.85,
            },
            ..Default::default()
        }),
        masks: vec![linear_mask(1, [0.5, 0.0], [0.5, 0.6], -1.0, 0.0, 0.0)],
        spots: vec![crate::retouch::Spot {
            x: 0.3,
            y: 0.4,
            source_x: 0.6,
            source_y: 0.4,
            radius: 0.05,
            ..Default::default()
        }],
        unoriented: true,
        ..Default::default()
    };
    let before = render(&old, &stored);
    for o in [3u16, 6, 7, 8] {
        let adapted = old.on_upright(Turn::from_exif(o));
        assert!(!adapted.unoriented);
        let after = render(&adapted, &upright(&stored, o));
        assert_eq!(
            (after.width(), after.height()),
            (before.width(), before.height()),
            "orientation {o}"
        );
        let worst = after
            .data()
            .iter()
            .zip(before.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 2, "orientation {o}: off by {worst}");
    }
    // Recipes made on upright photos are left as they are.
    let current = EditRecipe {
        unoriented: false,
        ..old.clone()
    };
    assert_eq!(current.on_upright(Turn::from_exif(6)), current);
}
