//! Vignette and Grain (ADR 0031): the Detail section's finishing touches.
//!
//! Both depend on where a pixel is in the frame, measured in fractions of the image,
//! so the preview and the export place them the same way.
//!
//! - **Vignette** darkens (negative) or lightens (positive) towards the corners along
//!   an ellipse that follows the frame, starting a third of the way out and reaching
//!   its full amount at the corners. It is a gain in stops, equal on R, G and B, and
//!   runs on scene-referred values before the tone curve, as lens falloff would, so
//!   brightened corners still roll off in the highlights.
//! - **Grain** is a fixed film-grain pattern (smooth value noise on a lattice of
//!   [`GRAIN_CELLS`] cells across the long edge): the same grains at every render
//!   size. It changes brightness only, most in the midtones, and runs last, on the
//!   finished image.

use image_core::color::REC709_LUMA;

/// Vignette at -100 darkens the corners by this many stops.
const VIGNETTE_DARKEN_STOPS: f32 = 1.5;
/// Vignette at +100 lightens the corners by this many stops.
const VIGNETTE_LIGHTEN_STOPS: f32 = 1.0;
/// Where the vignette starts and ends, as a share of the centre-to-corner distance.
const VIGNETTE_START: f32 = 0.3;
const VIGNETTE_END: f32 = 1.0;

/// Grain lattice cells across the long edge: about 4 px grains at 24 MP.
pub const GRAIN_CELLS: f32 = 1500.0;
/// Grain at 100 varies midtones by up to this many stops.
const GRAIN_STOPS: f32 = 0.35;

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Normalised squared distances from the centre for each column (or row) of a
/// dimension `n`: `((i + 0.5) / n * 2 - 1)^2`, 0 at the centre and 1 at the edges.
pub fn axis_squares(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            t * t
        })
        .collect()
}

/// The vignette's gain in stops for a Vignette value (-100..100) at a pixel whose
/// normalised squared offsets from the centre are `dx2` and `dy2`.
#[inline]
pub fn vignette_stops(amount: f32, dx2: f32, dy2: f32) -> f32 {
    // 0 at the centre, 1 at the corners.
    let d = ((dx2 + dy2) * 0.5).sqrt();
    let falloff = smoothstep(VIGNETTE_START, VIGNETTE_END, d);
    let a = amount / 100.0;
    let scale = if a < 0.0 {
        VIGNETTE_DARKEN_STOPS
    } else {
        VIGNETTE_LIGHTEN_STOPS
    };
    a * scale * falloff
}

/// Deterministic noise in -1..1 for a lattice point.
#[inline]
fn lattice(ix: i32, iy: i32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iy as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 8_388_607.5 - 1.0
}

/// Film-like grain at lattice coordinates (`gx`, `gy`): two layers of value noise,
/// the second finer and rotated 30°, so the square lattice does not show.
#[inline]
pub fn grain_noise(gx: f32, gy: f32) -> f32 {
    const COS: f32 = 0.866_025_4;
    const SIN: f32 = 0.5;
    let (rx, ry) = (COS * gx - SIN * gy, SIN * gx + COS * gy);
    let n = 0.65 * value_noise(gx, gy) + 0.55 * value_noise(1.7 * rx + 17.3, 1.7 * ry + 5.1);
    n.clamp(-1.0, 1.0)
}

/// Smooth value noise at lattice coordinates (`gx`, `gy`).
#[inline]
fn value_noise(gx: f32, gy: f32) -> f32 {
    let (x0, y0) = (gx.floor(), gy.floor());
    let (tx, ty) = (gx - x0, gy - y0);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let top = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * sx;
    let bottom = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * sx;
    top + (bottom - top) * sy
}

/// Lattice cells per pixel for an image of `width` x `height`.
pub fn grain_scale(width: usize, height: usize) -> f32 {
    GRAIN_CELLS / width.max(height).max(1) as f32
}

/// The grain's gain in stops for a Grain value (0..100) at noise value `n` (-1..1)
/// on a display value `y` (0..1): strongest in the midtones.
#[inline]
pub fn grain_stops(amount: f32, n: f32, y: f32) -> f32 {
    let midtones = 4.0 * y.clamp(0.0, 1.0) * (1.0 - y.clamp(0.0, 1.0));
    amount / 100.0 * GRAIN_STOPS * n * midtones
}

/// Grain for one pixel at (`x`, `y`) of an image with `scale` lattice cells per pixel.
#[inline]
pub fn apply_grain(rgb: [f32; 3], amount: f32, x: usize, y: usize, scale: f32) -> [f32; 3] {
    let n = grain_noise((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
    let luma = rgb[0] * REC709_LUMA[0] + rgb[1] * REC709_LUMA[1] + rgb[2] * REC709_LUMA[2];
    let gain = grain_stops(amount, n, luma).exp2();
    rgb.map(|c| c * gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops_at(amount: f32, w: usize, h: usize, x: usize, y: usize) -> f32 {
        vignette_stops(amount, axis_squares(w)[x], axis_squares(h)[y])
    }

    #[test]
    fn vignette_leaves_the_centre_and_acts_on_the_corners() {
        assert_eq!(stops_at(-100.0, 300, 200, 150, 100), 0.0);
        let corner = stops_at(-100.0, 300, 200, 0, 0);
        assert!((corner + VIGNETTE_DARKEN_STOPS).abs() < 0.05, "{corner}");
        assert!(stops_at(80.0, 300, 200, 299, 199) > 0.7);
        assert_eq!(stops_at(0.0, 300, 200, 0, 0), 0.0);
    }

    #[test]
    fn vignette_is_symmetric_and_resolution_independent() {
        assert_eq!(
            stops_at(-50.0, 300, 200, 10, 20),
            stops_at(-50.0, 300, 200, 289, 179)
        );
        // The same place in the frame at two sizes.
        let small = stops_at(-60.0, 400, 300, 40, 30);
        let large = stops_at(-60.0, 1600, 1200, 161, 121);
        assert!((small - large).abs() < 0.01, "{small} vs {large}");
    }

    #[test]
    fn grain_is_deterministic_centred_and_bounded() {
        let values: Vec<f32> = (0..10_000)
            .map(|k| grain_noise((k % 100) as f32 * 0.37, (k / 100) as f32 * 0.37))
            .collect();
        assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!(mean.abs() < 0.05, "{mean}");
        assert_eq!(grain_noise(12.3, 45.6), grain_noise(12.3, 45.6));
    }

    #[test]
    fn grain_is_strongest_in_the_midtones_and_off_at_zero() {
        assert_eq!(apply_grain([0.4; 3], 0.0, 10, 10, 1.0), [0.4; 3]);
        assert!(grain_stops(100.0, 1.0, 0.5) > 4.0 * grain_stops(100.0, 1.0, 0.95));
        assert!(grain_stops(100.0, 1.0, 0.5) > 4.0 * grain_stops(100.0, 1.0, 0.03));
    }

    #[test]
    fn grain_keeps_hue() {
        let out = apply_grain([0.5, 0.3, 0.2], 100.0, 7, 9, 0.8);
        assert!((out[0] / out[1] - 0.5 / 0.3).abs() < 1e-4);
        assert!((out[2] / out[1] - 0.2 / 0.3).abs() < 1e-4);
    }

    #[test]
    fn grain_is_placed_by_the_frame_not_the_pixel_grid() {
        // Corresponding pixels of a 4x smaller render sample the same noise field.
        let (sl, ss) = (grain_scale(6064, 4040), grain_scale(1516, 1010));
        let at_large = grain_noise((4.0 * 300.0 + 2.0) * sl, (4.0 * 200.0 + 2.0) * sl);
        let at_small = grain_noise((300.0 + 0.5) * ss, (200.0 + 0.5) * ss);
        assert!(
            (at_large - at_small).abs() < 1e-3,
            "{at_large} vs {at_small}"
        );
    }
}
