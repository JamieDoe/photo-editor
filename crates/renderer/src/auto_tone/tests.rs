use image_core::{LinearImage, NeverCancel, OutputImage, PixelFormat};

use super::*;
use crate::{CpuRenderer, RenderBackend, RenderPlan};

/// A scene of linear light `f(x, y)` (grey with a little colour), 240 × 160.
fn scene(f: impl Fn(f32, f32) -> f32) -> LinearImage {
    let (w, h) = (240u32, 160u32);
    let data = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let v = f(x as f32 / w as f32, y as f32 / h as f32);
            [v * 1.05, v, v * 0.92].map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16)
        })
        .collect();
    LinearImage::new(w, h, data).unwrap()
}

fn measure(
    image: &LinearImage,
) -> impl FnMut(&EditRecipe) -> Result<ToneStats, crate::RenderError> + '_ {
    move |r| {
        let plan = RenderPlan::from_recipe(r, None);
        CpuRenderer
            .render(&plan, image, PixelFormat::Rgb8, &NeverCancel)
            .map(|out| ToneStats::of(&out))
    }
}

fn auto(image: &LinearImage) -> (AutoTone, ToneStats) {
    let base = EditRecipe::default();
    let tone = auto_tone(&base, measure(image)).unwrap();
    let after = measure(image)(&tone.apply(&base)).unwrap();
    (tone, after)
}

#[test]
fn stats_read_percentiles_shares_and_colour() {
    // Four equal bands of grey (luma 0, 64, 128, 255), one of them red.
    let mut data = Vec::new();
    for v in [0u8, 64, 128, 255] {
        for _ in 0..100 {
            data.extend([v, v, v]);
        }
    }
    data[0..3].copy_from_slice(&[255, 0, 0]);
    let img = OutputImage::from_raw(400, 1, PixelFormat::Rgb8, data).unwrap();
    let s = ToneStats::of(&img);
    assert!(
        (s.percentile(0.5) - 64.0 / 256.0).abs() < 2.0 / 256.0,
        "{}",
        s.percentile(0.5)
    );
    assert!((s.share_above(0.9) - 0.25).abs() < 1e-6);
    assert!((s.share_below(0.1) - 0.25).abs() < 0.01);
    assert!((s.colourfulness - 1.0 / 400.0).abs() < 1e-4);
}

#[test]
fn a_dark_photo_is_brightened_to_a_middle_median() {
    let (tone, after) = auto(&scene(|x, y| 0.004 + 0.03 * x * (0.5 + y)));
    assert!(tone.exposure > 1.0, "{tone:?}");
    assert!(
        after.percentile(0.5) > MEDIAN.0 - 0.05,
        "{}",
        after.percentile(0.5)
    );
}

#[test]
fn a_bright_photo_is_darkened_and_its_highlights_recovered() {
    let (tone, after) = auto(&scene(|x, y| 0.35 + 0.65 * x.max(y)));
    assert!(tone.exposure < 0.0, "{tone:?}");
    assert!(
        after.share_above(BRIGHT) <= BRIGHT_SHARE + 0.03,
        "{}",
        after.share_above(BRIGHT)
    );
}

#[test]
fn a_flat_photo_gains_contrast_and_reaches_its_ends() {
    let (tone, after) = auto(&scene(|x, _| 0.12 + 0.06 * x));
    assert!(tone.contrast > 0.0, "{tone:?}");
    let spread = after.percentile(0.995) - after.percentile(0.005);
    let before = measure(&scene(|x, _| 0.12 + 0.06 * x))(&EditRecipe::default()).unwrap();
    assert!(
        spread > before.percentile(0.995) - before.percentile(0.005),
        "{tone:?}"
    );
}

#[test]
fn a_well_exposed_photo_is_left_almost_alone() {
    // Tones spread evenly over seven stops (as a well-lit scene's are), scaled until
    // the default render's median is in the middle of its band.
    let lit = |k: f32| scene(move |x, y| k * (-7.0 * (1.0 - (0.15 * y + 0.85 * x))).exp2());
    let median = |k: f32| {
        measure(&lit(k))(&EditRecipe::default())
            .unwrap()
            .percentile(0.5)
    };
    let (mut lo, mut hi) = (0.01f32, 1.0f32);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        if median(mid) < (MEDIAN.0 + MEDIAN.1) / 2.0 {
            lo = mid
        } else {
            hi = mid
        }
    }
    let (tone, _) = auto(&lit(lo));
    // Only its clipped top (scaled up, the scene's brightest stop passes white) is
    // recovered; nothing else moves much.
    assert_eq!(
        (tone.exposure, tone.contrast, tone.shadows),
        (0.0, 0.0, 0.0),
        "{tone:?}"
    );
    assert!(
        tone.highlights < 0.0 && tone.whites.abs() <= 10.0 && tone.blacks.abs() <= 10.0,
        "{tone:?}"
    );
}

#[test]
fn a_night_scene_stays_dark() {
    // Mostly black with a few lights: Exposure stops at its limit.
    let (tone, _) = auto(&scene(|x, y| {
        if (x * 20.0).fract() < 0.05 && y < 0.3 {
            0.8
        } else {
            0.0005
        }
    }));
    assert!(tone.exposure <= EXPOSURE_RANGE.1, "{tone:?}");
}

#[test]
fn every_setting_is_in_range_and_the_same_each_time() {
    for img in [
        scene(|x, y| 0.01 + 0.5 * x * y),
        scene(|x, _| 0.2 + 0.1 * (x * 30.0).sin()),
    ] {
        let (a, _) = auto(&img);
        let (b, _) = auto(&img);
        assert_eq!(a, b);
        for v in [
            a.contrast,
            a.highlights,
            a.shadows,
            a.whites,
            a.blacks,
            a.vibrance,
        ] {
            assert!((-100.0..=100.0).contains(&v) && v == v.round(), "{a:?}");
        }
        assert!(
            (EXPOSURE_RANGE.0..=EXPOSURE_RANGE.1).contains(&a.exposure),
            "{a:?}"
        );
    }
}

#[test]
fn one_setting_alone_aims_at_its_target_even_on_a_fine_photo() {
    // A photo Auto leaves alone (median within the band): Exposure asked for by name
    // still moves it to the target.
    let lit = |k: f32| scene(move |x, y| k * (-7.0 * (1.0 - (0.15 * y + 0.85 * x))).exp2());
    let img = lit(0.5);
    let base = EditRecipe::default();
    let exposure = auto_setting(&base, ToneSetting::Exposure, measure(&img)).unwrap();
    let after = measure(&img)(&EditRecipe {
        exposure,
        ..base.clone()
    })
    .unwrap();
    assert!(
        (after.percentile(0.5) - MEDIAN_TARGET).abs() < 0.03,
        "{exposure}: {}",
        after.percentile(0.5)
    );
}

#[test]
fn one_setting_follows_the_rest_of_the_edit() {
    // Tones up to under a stop below white (where Whites acts): their end falls a
    // little short of white, so Auto Whites raises it; with Exposure pushed until it
    // clips, Auto Whites pulls it in instead.
    let img = scene(|x, _| 0.06 + 0.6 * x);
    let plain = auto_setting(&EditRecipe::default(), ToneSetting::Whites, measure(&img)).unwrap();
    assert!(plain > 0.0, "{plain}");
    let bright = EditRecipe {
        exposure: 0.6,
        ..Default::default()
    };
    let clipped = auto_setting(&bright, ToneSetting::Whites, measure(&img)).unwrap();
    assert!(clipped < 0.0, "{clipped}");
}

#[test]
fn a_setting_that_cannot_change_the_photo_stays_at_zero() {
    // A photo no setting changes (the same tones whatever the recipe): however far a
    // setting goes, nothing moves, so it stays where it is rather than going to its
    // limit.
    let img = scene(|x, _| 0.05 + 0.6 * x);
    let fixed = measure(&img)(&EditRecipe::default()).unwrap();
    let unmoved = |_: &EditRecipe| Ok::<_, crate::RenderError>(fixed.clone());
    for setting in [
        ToneSetting::Whites,
        ToneSetting::Blacks,
        ToneSetting::Exposure,
    ] {
        assert_eq!(
            auto_setting(&EditRecipe::default(), setting, unmoved).unwrap(),
            0.0,
            "{setting:?}"
        );
    }
}
