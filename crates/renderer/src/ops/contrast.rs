//! Contrast as an S-curve around mid grey.
//!
//! Applied per channel in a perceptual (gamma 2.2) domain. The curve fixes 0, mid grey
//! and 1, is C1-continuous at the pivot with slope `gamma`, and passes values above 1
//! (e.g. after positive exposure) through unchanged so contrast never clips highlights
//! on its own.

const PERCEPTUAL_GAMMA: f32 = 2.2;
const MID_GREY: f32 = 0.18;
/// Strength at slider +/-100: gamma = 2^(+/-STRENGTH).
const STRENGTH: f32 = 0.8;

/// Curve exponent for a contrast slider value in -100..100.
pub fn gamma_for(amount: f32) -> f32 {
    (amount / 100.0 * STRENGTH).exp2()
}

/// Applies the contrast curve to one scene-linear channel value.
pub fn apply(linear: f32, gamma: f32) -> f32 {
    if !(0.0..1.0).contains(&linear) {
        return linear.max(0.0);
    }
    let p = linear.powf(1.0 / PERCEPTUAL_GAMMA);
    let m = MID_GREY.powf(1.0 / PERCEPTUAL_GAMMA);
    let q = if p <= m {
        m * (p / m).powf(gamma)
    } else {
        1.0 - (1.0 - m) * ((1.0 - p) / (1.0 - m)).powf(gamma)
    };
    q.powf(PERCEPTUAL_GAMMA)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_identity() {
        let g = gamma_for(0.0);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            assert!((apply(x, g) - x).abs() < 1e-5);
        }
    }

    #[test]
    fn fixes_black_mid_grey_and_white() {
        for amount in [-100.0, 50.0, 100.0] {
            let g = gamma_for(amount);
            assert!(apply(0.0, g).abs() < 1e-6);
            assert!((apply(MID_GREY, g) - MID_GREY).abs() < 1e-5);
            assert!((apply(0.999_999, g) - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn positive_contrast_darkens_shadows_and_brightens_highlights() {
        let g = gamma_for(60.0);
        assert!(apply(0.05, g) < 0.05);
        assert!(apply(0.6, g) > 0.6);
        let g = gamma_for(-60.0);
        assert!(apply(0.05, g) > 0.05);
        assert!(apply(0.6, g) < 0.6);
    }

    #[test]
    fn monotonic() {
        for amount in [-100.0, 100.0] {
            let g = gamma_for(amount);
            let mut last = 0.0;
            for i in 1..=1000 {
                let y = apply(i as f32 / 1000.0, g);
                assert!(y >= last);
                last = y;
            }
        }
    }

    #[test]
    fn values_above_one_pass_through() {
        assert_eq!(apply(2.5, gamma_for(80.0)), 2.5);
    }
}
