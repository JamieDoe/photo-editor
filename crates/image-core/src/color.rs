//! Transfer functions and constants for the sRGB / Rec.709 working space.

/// Rec.709 luminance weights for linear RGB.
pub const REC709_LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// Linear XYZ (D65) to linear sRGB.
pub const XYZ_TO_LINEAR_SRGB: [[f32; 3]; 3] = [
    [3.240_454_2, -1.537_138_5, -0.498_531_4],
    [-0.969_266, 1.876_010_8, 0.041_556],
    [0.055_643_4, -0.204_025_9, 1.057_225_2],
];

/// sRGB electro-optical transfer function (encoded to linear), input in [0, 1].
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB inverse EOTF (linear to encoded), input in [0, 1].
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Lookup table converting 8-bit sRGB-encoded samples to 16-bit linear samples.
pub fn srgb8_to_linear16_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    for (i, out) in table.iter_mut().enumerate() {
        let linear = srgb_to_linear(i as f32 / 255.0);
        *out = (linear * f32::from(u16::MAX)).round() as u16;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_round_trip() {
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            assert!((linear_to_srgb(srgb_to_linear(v)) - v).abs() < 1e-5, "{v}");
        }
    }

    #[test]
    fn srgb_reference_values() {
        assert!((linear_to_srgb(0.18) - 0.461_356).abs() < 1e-4);
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-4);
    }

    #[test]
    fn linear16_table_endpoints_monotonic() {
        let t = srgb8_to_linear16_table();
        assert_eq!(t[0], 0);
        assert_eq!(t[255], u16::MAX);
        assert!(t.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn xyz_d65_white_maps_to_rgb_white() {
        let white = [0.950_47_f32, 1.0, 1.088_83];
        for row in XYZ_TO_LINEAR_SRGB {
            let v: f32 = row.iter().zip(white).map(|(m, w)| m * w).sum();
            assert!((v - 1.0).abs() < 1e-3, "{v}");
        }
    }
}
