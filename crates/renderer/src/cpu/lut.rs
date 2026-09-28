use std::sync::OnceLock;

use image_core::color::linear_to_srgb;

/// Piecewise-linear LUT for a per-channel curve on [0, 1]. Values outside [0, 1] are
/// passed through (clamped at 0), matching the reference curve definitions.
pub(super) struct CurveLut {
    table: Vec<f32>,
}

impl CurveLut {
    const SIZE: usize = 4096;

    pub(super) fn build(f: impl Fn(f32) -> f32) -> Self {
        let table = (0..=Self::SIZE)
            .map(|i| f(i as f32 / Self::SIZE as f32))
            .collect();
        Self { table }
    }

    #[inline]
    pub(super) fn eval(&self, x: f32) -> f32 {
        if !(0.0..1.0).contains(&x) {
            return x.max(0.0);
        }
        let pos = x * Self::SIZE as f32;
        let i = pos as usize;
        let t = pos - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * t
    }
}

/// Linear [0, 1] -> 8-bit sRGB encoding via a dense nearest-neighbour table.
pub(super) struct OutputLut {
    table: Vec<u8>,
}

impl OutputLut {
    const SIZE: usize = 16384;

    fn build() -> Self {
        let table = (0..=Self::SIZE)
            .map(|i| (linear_to_srgb(i as f32 / Self::SIZE as f32) * 255.0).round() as u8)
            .collect();
        Self { table }
    }

    #[inline]
    pub(super) fn encode(&self, x: f32) -> u8 {
        // NaN clamps to 0 via the saturating float->int cast.
        let i = (x.clamp(0.0, 1.0) * Self::SIZE as f32 + 0.5) as usize;
        self.table[i.min(Self::SIZE)]
    }
}

pub(super) fn output_lut() -> &'static OutputLut {
    static LUT: OnceLock<OutputLut> = OnceLock::new();
    LUT.get_or_init(OutputLut::build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::contrast;

    #[test]
    fn output_lut_matches_reference_within_one_code() {
        let lut = output_lut();
        for i in 0..=10_000 {
            let x = i as f32 / 10_000.0;
            let exact = (linear_to_srgb(x) * 255.0).round() as i32;
            assert!((i32::from(lut.encode(x)) - exact).abs() <= 1, "{x}");
        }
        assert_eq!(lut.encode(-1.0), 0);
        assert_eq!(lut.encode(5.0), 255);
        assert_eq!(lut.encode(f32::NAN), 0);
    }

    #[test]
    fn curve_lut_matches_reference() {
        for amount in [-100.0, -40.0, 40.0, 100.0] {
            let g = contrast::gamma_for(amount);
            let lut = CurveLut::build(|x| contrast::apply(x, g));
            for i in 0..=2000 {
                let x = i as f32 / 2000.0;
                let (a, b) = (lut.eval(x), contrast::apply(x, g));
                // Compare in display encoding, where the error is visible.
                let err = (linear_to_srgb(a.min(1.0)) - linear_to_srgb(b.min(1.0))).abs();
                assert!(err < 1.0 / 255.0, "amount {amount} x {x}: {a} vs {b}");
            }
        }
    }
}
