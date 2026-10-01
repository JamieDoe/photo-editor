//! Global saturation: scale chroma around Rec.709 luminance in linear light.

use image_core::color::REC709_LUMA;

/// Chroma multiplier for a slider value in -100..100 (-100 = monochrome, 100 = 2x).
pub fn factor_for(amount: f32) -> f32 {
    1.0 + amount / 100.0
}

/// Colours pushed past sRGB keep their negative channels here; they are brought back
/// smoothly on output (ADR 0060) rather than clipped channel by channel.
pub fn apply(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = rgb[0] * REC709_LUMA[0] + rgb[1] * REC709_LUMA[1] + rgb[2] * REC709_LUMA[2];
    rgb.map(|c| y + (c - y) * factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_identity() {
        assert_eq!(apply([0.2, 0.4, 0.6], factor_for(0.0)), [0.2, 0.4, 0.6]);
    }

    #[test]
    fn minus_100_is_monochrome_with_same_luminance() {
        let out = apply([0.8, 0.2, 0.1], factor_for(-100.0));
        assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6);
        let y = 0.8 * REC709_LUMA[0] + 0.2 * REC709_LUMA[1] + 0.1 * REC709_LUMA[2];
        assert!((out[0] - y).abs() < 1e-6);
    }

    #[test]
    fn neutrals_are_unchanged() {
        assert_eq!(apply([0.3; 3], factor_for(100.0)), [0.3; 3]);
    }

    #[test]
    fn colours_pushed_past_srgb_are_compressed_on_output_keeping_their_hue() {
        // An orange pushed hard: blue goes below zero here (ADR 0060) ...
        let orange = [0.6, 0.25, 0.05];
        let pushed = apply(orange, factor_for(100.0));
        assert!(pushed[2] < 0.0, "{pushed:?}");
        // ... and the output's compression brings it back inside, keeping the ratio
        // of red to green (its hue) where clipping blue at zero would not.
        let out = image_core::gamut::compress(pushed, &image_core::gamut::ON_OUTPUT);
        assert!(out.iter().all(|&c| c >= -1e-4), "{out:?}");
        assert_eq!((out[0], out[1]), (pushed[0], pushed[1]));
        // Pushed further than the compression reaches, the encoder's clamp still
        // keeps it in range.
        let extreme = apply([1.0, 0.0, 0.0], factor_for(100.0));
        let out = image_core::gamut::compress(extreme, &image_core::gamut::ON_OUTPUT);
        assert!(out.iter().all(|c| c.is_finite()));
    }
}
