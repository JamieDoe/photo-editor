//! Relative white-balance (temperature) adjustment.
//!
//! The slider shifts the white point along the Planckian locus in mired space,
//! relative to the as-shot white balance already applied by the decoder. Gains are
//! the ratio between the blackbody white at the shifted temperature and at the
//! reference temperature, normalised to preserve the luminance of neutrals.
//!
//! Phase 0 simplification: gains are applied in linear sRGB after the camera matrix,
//! not in camera space. See docs/RENDERING.md.

use image_core::color::{REC709_LUMA, XYZ_TO_LINEAR_SRGB};

const REFERENCE_KELVIN: f32 = 6500.0;
/// Mired shift at slider +/-100.
const MIRED_PER_UNIT: f32 = 1.2;
const MIN_KELVIN: f32 = 1667.0;
const MAX_KELVIN: f32 = 25_000.0;

/// Channel gains for a temperature slider value in -100..100 (positive = warmer).
pub fn temperature_gains(amount: f32) -> [f32; 3] {
    let mired = 1.0e6 / REFERENCE_KELVIN + amount * MIRED_PER_UNIT;
    let kelvin = (1.0e6 / mired.max(1.0)).clamp(MIN_KELVIN, MAX_KELVIN);
    let target = blackbody_rgb(kelvin);
    let reference = blackbody_rgb(REFERENCE_KELVIN);
    let mut gains = [
        target[0] / reference[0],
        target[1] / reference[1],
        target[2] / reference[2],
    ];
    let luma: f32 = gains.iter().zip(REC709_LUMA).map(|(g, w)| g * w).sum();
    for g in &mut gains {
        *g /= luma;
    }
    gains
}

/// Linear sRGB of a blackbody's chromaticity (Y = 1), Kim et al. cubic approximation
/// of the Planckian locus, valid 1667 K..25000 K.
pub fn blackbody_rgb(kelvin: f32) -> [f32; 3] {
    let t = f64::from(kelvin.clamp(MIN_KELVIN, MAX_KELVIN));
    let (t2, t3) = (t * t, t * t * t);
    let x = if t <= 4000.0 {
        -0.266_123_9e9 / t3 - 0.234_358_9e6 / t2 + 0.877_695_6e3 / t + 0.179_910
    } else {
        -3.025_846_9e9 / t3 + 2.107_037_9e6 / t2 + 0.222_634_7e3 / t + 0.240_390
    };
    let (x2, x3) = (x * x, x * x * x);
    let y = if t <= 2222.0 {
        -1.106_381_4 * x3 - 1.348_110_20 * x2 + 2.185_558_32 * x - 0.202_196_83
    } else if t <= 4000.0 {
        -0.954_947_6 * x3 - 1.374_185_93 * x2 + 2.091_370_15 * x - 0.167_488_67
    } else {
        3.081_758_0 * x3 - 5.873_386_70 * x2 + 3.751_129_97 * x - 0.370_014_83
    };
    let xyz = [x / y, 1.0, (1.0 - x - y) / y];
    let mut rgb = [0.0f32; 3];
    for (out, row) in rgb.iter_mut().zip(XYZ_TO_LINEAR_SRGB) {
        *out = row
            .iter()
            .zip(xyz)
            .map(|(m, v)| f64::from(*m) * v)
            .sum::<f64>() as f32;
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_identity() {
        for g in temperature_gains(0.0) {
            assert!((g - 1.0).abs() < 1e-5, "{g}");
        }
    }

    #[test]
    fn positive_is_warmer_negative_is_cooler() {
        let warm = temperature_gains(50.0);
        assert!(warm[0] > 1.0 && warm[2] < 1.0, "{warm:?}");
        let cool = temperature_gains(-50.0);
        assert!(cool[0] < 1.0 && cool[2] > 1.0, "{cool:?}");
    }

    #[test]
    fn preserves_neutral_luminance() {
        for amount in [-100.0, -30.0, 40.0, 100.0] {
            let g = temperature_gains(amount);
            let luma: f32 = g.iter().zip(REC709_LUMA).map(|(g, w)| g * w).sum();
            assert!((luma - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn gains_are_monotonic_in_amount() {
        let mut last = temperature_gains(-100.0);
        for i in -99..=100 {
            let g = temperature_gains(i as f32);
            assert!(g[0] >= last[0] && g[2] <= last[2], "at {i}");
            last = g;
        }
    }

    #[test]
    fn blackbody_6500k_is_near_white() {
        // The Planckian locus at 6500 K is slightly magenta of D65 (which lies on the
        // daylight locus), so allow a few percent. Gains are ratios against the same
        // Planckian reference, so this offset never reaches the image.
        let rgb = blackbody_rgb(6504.0);
        let max = rgb.iter().copied().fold(0.0, f32::max);
        for c in rgb {
            assert!((c / max - 1.0).abs() < 0.08, "{rgb:?}");
        }
    }
}
