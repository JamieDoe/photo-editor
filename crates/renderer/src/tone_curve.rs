//! The tone curve the UI draws (ADR 0029): what the Light controls do to a neutral
//! tone, against how the default recipe renders it.
//!
//! Both axes are display values (sRGB-encoded, 0..1). The x axis is a tone as the
//! default recipe shows it; the y axis is the same scene tone with the recipe's
//! exposure, tone controls (Highlights and Shadows as they act on an area of that
//! brightness), contrast and base look. An unedited photo gives the diagonal.
//!
//! Local effects that depend on the picture (Dehaze, and Highlights and Shadows next
//! to brighter or darker surroundings) cannot be one curve; the graph shows their
//! effect on even areas.

use image_core::color::linear_to_srgb;

use crate::EditRecipe;
use crate::Look;
use crate::ops::{contrast, look, tone};

/// `points` samples of the curve (at x = i / (points - 1)), each the display value
/// 0..1 the recipe gives that tone.
pub fn tone_curve(recipe: &EditRecipe, points: usize) -> Vec<f32> {
    let r = recipe.sanitized();
    let default = EditRecipe::default();
    let n = points.max(2);
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1) as f32;
            let scene = scene_for_display(&default, x);
            display_of(&r, scene)
        })
        .collect()
}

/// The display value (sRGB-encoded) of a neutral scene-linear tone under `r`'s Light
/// controls and look.
fn display_of(r: &EditRecipe, scene: f32) -> f32 {
    let mut v = scene * r.exposure.exp2();
    let params = r.tone();
    if !params.is_identity() {
        // An even area: the surroundings are the tone itself.
        let d = tone::stops_below_white(v);
        v = tone::apply([v; 3], d, &params)[1];
    }
    if r.contrast != 0.0 {
        v = contrast::apply(v, contrast::gamma_for(r.contrast));
    }
    if r.look == Look::Standard {
        v = look::standard(v);
    }
    linear_to_srgb(v.clamp(0.0, 1.0))
}

/// The scene tone the default recipe displays as `x` (bisection: the response is
/// monotonic).
fn scene_for_display(default: &EditRecipe, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if display_of(default, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_the_diagonal() {
        for (i, y) in tone_curve(&EditRecipe::default(), 49).iter().enumerate() {
            let x = i as f32 / 48.0;
            assert!((y - x).abs() < 2e-3, "{x}: {y}");
        }
    }

    #[test]
    fn controls_bend_it_the_expected_way() {
        let at = |r: EditRecipe, x: usize| tone_curve(&r, 49)[x];
        let base = EditRecipe::default();
        let mid = 24;
        assert!(
            at(
                EditRecipe {
                    exposure: 1.0,
                    ..base
                },
                mid
            ) > at(base, mid) + 0.05
        );
        // Contrast: darker below the middle, brighter above.
        let c = EditRecipe {
            contrast: 60.0,
            ..base
        };
        assert!(at(c, 10) < at(base, 10) && at(c, 40) > at(base, 40));
        // Shadows lift the low end more than the high end.
        let s = EditRecipe {
            shadows: 80.0,
            ..base
        };
        assert!(at(s, 8) - at(base, 8) > at(s, 44) - at(base, 44));
        // The Flat look (no base curve) sits below the default in the shadows.
        let flat = EditRecipe {
            look: Look::Flat,
            ..base
        };
        assert!(at(flat, 12) < at(base, 12));
    }

    #[test]
    fn is_monotonic_and_in_range() {
        let r = EditRecipe {
            exposure: 0.7,
            contrast: 40.0,
            highlights: -60.0,
            shadows: 50.0,
            whites: 30.0,
            blacks: -40.0,
            ..EditRecipe::default()
        };
        let c = tone_curve(&r, 49);
        assert!(c.iter().all(|y| (0.0..=1.0).contains(y)));
        assert!(c.windows(2).all(|w| w[1] >= w[0] - 1e-4), "{c:?}");
    }
}
