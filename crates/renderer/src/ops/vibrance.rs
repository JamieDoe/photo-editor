//! Vibrance (ADR 0024): saturation that favours muted colours and spares skin tones.
//!
//! Runs on display-referred values (after the base look), where "how saturated a
//! colour looks" is meaningful. Chroma is scaled around Rec.709 luminance, like
//! Saturation, so brightness is kept; only the amount varies per pixel:
//!
//! - Positive: up to 2× chroma for muted colours, falling to none for fully saturated
//!   ones, and at most 40 % of that on skin hues, so faces don't turn orange.
//! - Negative: mutes, strong colours more than weak ones (to 0.4× / 0.7× chroma at
//!   -100), never to grey.

use image_core::color::REC709_LUMA;

const SKIN_PROTECTION: f32 = 0.6;
/// Skin hues in linear RGB, in degrees (HSV hue on linear values: 0 = red, 60 =
/// yellow); roughly 0..48° in encoded sRGB.
const SKIN_HUE: f32 = 20.0;
const SKIN_HALF_WIDTH: f32 = 20.0;

/// `amount` is the slider value / 100 (-1..1).
#[inline]
pub fn apply(rgb: [f32; 3], amount: f32) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max <= 0.0 || max == min {
        return rgb;
    }
    // Perceptual-ish saturation: HSV saturation of gamma ~2 values.
    let s = 1.0 - (min.max(0.0) / max).sqrt();
    let weight = if amount > 0.0 {
        let muted = (1.0 - s) * (1.0 - s);
        muted * (1.0 - SKIN_PROTECTION * skin(r, g, b, max, min))
    } else {
        0.6 * (0.5 + 0.5 * s)
    };
    let factor = 1.0 + amount * weight;
    let y = r * REC709_LUMA[0] + g * REC709_LUMA[1] + b * REC709_LUMA[2];
    rgb.map(|c| (y + (c - y) * factor).max(0.0))
}

/// 1 at the centre of the skin-hue band, falling to 0 at its edges.
#[inline]
fn skin(r: f32, g: f32, b: f32, max: f32, min: f32) -> f32 {
    // Skin is red-dominant with green above blue: hue 0..60°.
    if r < max || g < b {
        return 0.0;
    }
    let hue = 60.0 * (g - b) / (max - min);
    let t = (hue - SKIN_HUE) / SKIN_HALF_WIDTH;
    (1.0 - t * t).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chroma(rgb: [f32; 3]) -> f32 {
        rgb.iter().copied().fold(0.0, f32::max) - rgb.iter().copied().fold(1.0, f32::min)
    }

    fn luma(rgb: [f32; 3]) -> f32 {
        rgb.iter().zip(REC709_LUMA).map(|(c, w)| c * w).sum()
    }

    #[test]
    fn zero_and_neutrals_are_unchanged() {
        assert_eq!(apply([0.2, 0.4, 0.6], 0.0), [0.2, 0.4, 0.6]);
        assert_eq!(apply([0.3; 3], 1.0), [0.3; 3]);
        assert_eq!(apply([0.0; 3], -1.0), [0.0; 3]);
    }

    #[test]
    fn keeps_luminance() {
        for rgb in [[0.3, 0.25, 0.2], [0.1, 0.4, 0.2], [0.05, 0.1, 0.6]] {
            for a in [-1.0, 0.5, 1.0] {
                assert!((luma(apply(rgb, a)) - luma(rgb)).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn boosts_muted_colours_more_than_saturated_ones() {
        let muted = [0.30, 0.34, 0.40]; // a hazy blue sky
        let vivid = [0.02, 0.08, 0.60]; // a saturated blue
        let gain = |c: [f32; 3]| chroma(apply(c, 1.0)) / chroma(c);
        assert!(gain(muted) > 1.5, "{}", gain(muted));
        assert!(gain(vivid) < 1.1, "{}", gain(vivid));
    }

    #[test]
    fn spares_skin_tones() {
        let skin = [0.55, 0.32, 0.22];
        let foliage = [0.22, 0.32, 0.18]; // similar saturation, green
        let gain = |c: [f32; 3]| chroma(apply(c, 1.0)) / chroma(c);
        assert!(
            gain(skin) - 1.0 < 0.6 * (gain(foliage) - 1.0),
            "{} {}",
            gain(skin),
            gain(foliage)
        );
    }

    #[test]
    fn negative_mutes_but_never_to_grey() {
        for c in [[0.6, 0.1, 0.05], [0.3, 0.32, 0.36]] {
            let out = apply(c, -1.0);
            let ratio = chroma(out) / chroma(c);
            assert!((0.35..0.75).contains(&ratio), "{ratio}");
        }
    }

    #[test]
    fn monotonic_in_amount_and_never_negative() {
        let c = [0.5, 0.2, 0.1];
        let mut last = 0.0;
        for i in -10..=10 {
            let out = apply(c, i as f32 / 10.0);
            assert!(out.iter().all(|&v| v >= 0.0));
            assert!(chroma(out) >= last);
            last = chroma(out);
        }
    }
}
