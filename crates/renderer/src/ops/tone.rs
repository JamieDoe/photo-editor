//! Highlights, Shadows, Whites and Blacks (ADR 0023).
//!
//! All four brighten or darken a pixel by a number of stops, applied equally to R, G
//! and B so hues do not shift. They differ in what decides the amount:
//!
//! - **Shadows / Highlights** are *local*: the amount depends on the brightness of the
//!   pixel's surroundings (the edge-aware [`ToneBase`]), not on the pixel itself. Dark
//!   areas are lifted as a whole while the texture inside them keeps its contrast, and
//!   strong edges do not glow, because the base map follows them.
//! - **Whites / Blacks** set the ends of the tonal range: the amount depends on the
//!   pixel's own brightness, near white or near black.
//!
//! Brightness is measured in stops below sensor white (`d = -log2(Y)`, so mid grey
//! 0.18 is 2.47 stops down). Every weight is a smoothstep, so there are no tonal steps.

use image_core::LinearImage;
use image_core::color::REC709_LUMA;
use rayon::prelude::*;

/// Maximum lift of Shadows +100 in the darkest areas, in stops.
pub const SHADOWS_STOPS: f32 = 2.0;
/// Maximum change of Highlights ±100 in the brightest areas, in stops.
pub const HIGHLIGHTS_STOPS: f32 = 1.5;
/// Maximum change of Whites ±100 at white, in stops.
pub const WHITES_STOPS: f32 = 1.0;
/// Maximum change of Blacks ±100 in the deepest shadows, in stops.
pub const BLACKS_STOPS: f32 = 1.5;

/// Luminance floor for the logarithm (about 20 stops below white).
const FLOOR: f32 = 1.0e-6;

/// Slider values (-100..100) of the tone controls.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToneParams {
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl ToneParams {
    pub fn is_identity(&self) -> bool {
        self.highlights == 0.0 && self.shadows == 0.0 && self.whites == 0.0 && self.blacks == 0.0
    }

    /// Whether the local (neighbourhood) part is needed.
    pub fn is_local(&self) -> bool {
        self.highlights != 0.0 || self.shadows != 0.0
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Stops below white of a linear luminance.
pub fn stops_below_white(y: f32) -> f32 {
    -(y.max(FLOOR)).log2()
}

/// Local part, from the surroundings' brightness `d` (stops below white): shadows act
/// from about mid grey downwards, highlights within about 3 stops of white.
pub fn local_stops(d: f32, p: &ToneParams) -> f32 {
    let shadows = smoothstep(2.0, 5.5, d);
    let highlights = 1.0 - smoothstep(0.25, 3.0, d);
    p.shadows / 100.0 * SHADOWS_STOPS * shadows
        + p.highlights / 100.0 * HIGHLIGHTS_STOPS * highlights
}

/// End-point part, from the pixel's own brightness `d`: whites act on the top 1.5
/// stops, blacks on the deepest tones.
pub fn endpoint_stops(d: f32, p: &ToneParams) -> f32 {
    let whites = 1.0 - smoothstep(0.0, 1.5, d);
    let blacks = smoothstep(4.0, 8.0, d);
    p.whites / 100.0 * WHITES_STOPS * whites + p.blacks / 100.0 * BLACKS_STOPS * blacks
}

/// Reference implementation for one pixel: `base_d` is the surroundings' brightness
/// from [`ToneBase::stops_at`] (ignored when there is no local part).
pub fn apply(rgb: [f32; 3], base_d: f32, p: &ToneParams) -> [f32; 3] {
    let [wr, wg, wb] = REC709_LUMA;
    let d = stops_below_white(rgb[0] * wr + rgb[1] * wg + rgb[2] * wb);
    let gain = (local_stops(base_d, p) + endpoint_stops(d, p)).exp2();
    rgb.map(|c| c * gain)
}

/// Long edge of the base map. Fixed (not a fraction of the render size), so a preview
/// and the full-resolution export see the same surroundings and look the same.
pub const BASE_LONG_EDGE: u32 = 256;
/// Guided filter window radius on the base map: about a tenth of the picture.
const RADIUS: usize = 13;
/// Guided filter regularisation, in stops²: brightness steps well above about half a
/// stop count as edges and are kept.
const EPSILON: f32 = 0.25;

/// The surroundings' brightness of every point of an image: a fast guided filter
/// (He & Sun) of log luminance, self-guided, computed on a small map. For a pixel with
/// log luminance `i`, the smoothed value is `a·i + b`, with `a` and `b` interpolated
/// from the map; flat areas get their average, strong edges are kept.
#[derive(Debug, Clone)]
pub struct ToneBase {
    width: usize,
    height: usize,
    a: Vec<f32>,
    b: Vec<f32>,
}

impl ToneBase {
    /// Builds the map for `source` after per-channel `gains` (the white balance and
    /// exposure applied before the tone stage).
    pub fn build(source: &LinearImage, gains: [f32; 3]) -> Self {
        let (sw, sh) = (source.width() as usize, source.height() as usize);
        let long = sw.max(sh).max(1);
        let scale = f64::from(BASE_LONG_EDGE).min(long as f64) / long as f64;
        let w = ((sw as f64 * scale).round() as usize).max(1);
        let h = ((sh as f64 * scale).round() as usize).max(1);
        let [wr, wg, wb] = REC709_LUMA;
        let (gr, gg, gb) = (gains[0] * wr, gains[1] * wg, gains[2] * wb);

        // Area-average linear luminance into the map, one map row per task.
        let cell_of: Vec<u32> = (0..sw).map(|x| (x * w / sw) as u32).collect();
        let luminance: Vec<f32> = (0..h)
            .into_par_iter()
            .flat_map_iter(|my| {
                let (y0, y1) = (my * sh / h, ((my + 1) * sh / h).max(my * sh / h + 1));
                // f32 partial sums per source row (at most ~50 pixels per cell per row),
                // accumulated across rows in f64.
                let mut sums = vec![0.0f64; w];
                let mut counts = vec![0u32; w];
                let mut row_sums = vec![0.0f32; w];
                for y in y0..y1.min(sh) {
                    row_sums.iter_mut().for_each(|s| *s = 0.0);
                    let row = source.row(y as u32);
                    for (px, &cell) in row.as_chunks::<3>().0.iter().zip(&cell_of) {
                        row_sums[cell as usize] +=
                            f32::from(px[0]) * gr + f32::from(px[1]) * gg + f32::from(px[2]) * gb;
                        counts[cell as usize] += 1;
                    }
                    for (s, r) in sums.iter_mut().zip(&row_sums) {
                        *s += f64::from(*r);
                    }
                }
                sums.into_iter().zip(counts).map(|(s, c)| {
                    if c == 0 {
                        0.0
                    } else {
                        (s / f64::from(c) / 65535.0) as f32
                    }
                })
            })
            .collect();
        let i: Vec<f32> = luminance.iter().map(|&l| l.max(FLOOR).log2()).collect();
        Self::from_log_luminance(w, h, &i)
    }

    /// The guided filter on a `w` x `h` map of log2 luminance.
    pub fn from_log_luminance(w: usize, h: usize, i: &[f32]) -> Self {
        let ii: Vec<f32> = i.iter().map(|v| v * v).collect();
        let mean_i = box_blur(i, w, h, RADIUS);
        let mean_ii = box_blur(&ii, w, h, RADIUS);
        let (a, b): (Vec<f32>, Vec<f32>) = mean_i
            .iter()
            .zip(&mean_ii)
            .map(|(&m, &mm)| {
                let var = (mm - m * m).max(0.0);
                let a = var / (var + EPSILON);
                (a, m - a * m)
            })
            .unzip();
        Self {
            width: w,
            height: h,
            a: box_blur(&a, w, h, RADIUS),
            b: box_blur(&b, w, h, RADIUS),
        }
    }

    /// Surroundings' brightness, in stops below white, at pixel (`x`, `y`) of an image
    /// `image_w` x `image_h` whose own log2 luminance there is `log_y`.
    pub fn stops_at(&self, x: usize, y: usize, image_w: usize, image_h: usize, log_y: f32) -> f32 {
        let gx = ((x as f32 + 0.5) * self.width as f32 / image_w as f32 - 0.5)
            .clamp(0.0, (self.width - 1) as f32);
        let gy = ((y as f32 + 0.5) * self.height as f32 / image_h as f32 - 0.5)
            .clamp(0.0, (self.height - 1) as f32);
        let (x0, y0) = (gx as usize, gy as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (tx, ty) = (gx - x0 as f32, gy - y0 as f32);
        let lerp2 = |m: &[f32]| {
            let top = m[y0 * self.width + x0] * (1.0 - tx) + m[y0 * self.width + x1] * tx;
            let bottom = m[y1 * self.width + x0] * (1.0 - tx) + m[y1 * self.width + x1] * tx;
            top * (1.0 - ty) + bottom * ty
        };
        -(lerp2(&self.a) * log_y + lerp2(&self.b))
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Map columns and weights for each of `image_w` pixel columns: pixel `x` blends
    /// map columns `x0` and `x1` by `t` (precomputed once per render by fast backends).
    pub fn columns(&self, image_w: usize) -> Vec<(u32, u32, f32)> {
        (0..image_w)
            .map(|x| {
                let gx = ((x as f32 + 0.5) * self.width as f32 / image_w as f32 - 0.5)
                    .clamp(0.0, (self.width - 1) as f32);
                let x0 = gx as usize;
                (
                    (x0) as u32,
                    ((x0 + 1).min(self.width - 1)) as u32,
                    gx - x0 as f32,
                )
            })
            .collect()
    }

    /// The map's `a` and `b` for pixel row `y` of an `image_h`-tall image, at every map
    /// column (vertical interpolation done), into `a_row` and `b_row`.
    pub fn row(&self, y: usize, image_h: usize, a_row: &mut Vec<f32>, b_row: &mut Vec<f32>) {
        let gy = ((y as f32 + 0.5) * self.height as f32 / image_h as f32 - 0.5)
            .clamp(0.0, (self.height - 1) as f32);
        let y0 = gy as usize;
        let y1 = (y0 + 1).min(self.height - 1);
        let ty = gy - y0 as f32;
        let w = self.width;
        a_row.clear();
        b_row.clear();
        for x in 0..w {
            let (a0, a1) = (self.a[y0 * w + x], self.a[y1 * w + x]);
            let (b0, b1) = (self.b[y0 * w + x], self.b[y1 * w + x]);
            a_row.push(a0 + (a1 - a0) * ty);
            b_row.push(b0 + (b1 - b0) * ty);
        }
    }
}

/// Mean over a (2r+1)² window, clamped at the borders (divides by the pixels inside).
fn box_blur(data: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], len: usize, stride: usize, lines: usize, line_stride: usize| {
        let mut out = vec![0.0f32; src.len()];
        for line in 0..lines {
            let base = line * line_stride;
            let at = |k: usize| src[base + k * stride];
            let mut sum = 0.0f64;
            let mut count = 0u32;
            for k in 0..=r.min(len - 1) {
                sum += f64::from(at(k));
                count += 1;
            }
            for k in 0..len {
                out[base + k * stride] = (sum / f64::from(count)) as f32;
                if k + r + 1 < len {
                    sum += f64::from(at(k + r + 1));
                    count += 1;
                }
                if k >= r {
                    sum -= f64::from(at(k - r));
                    count -= 1;
                }
            }
        }
        out
    };
    let horizontal = pass(data, w, 1, h, w);
    pass(&horizontal, h, w, w, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(highlights: f32, shadows: f32, whites: f32, blacks: f32) -> ToneParams {
        ToneParams {
            highlights,
            shadows,
            whites,
            blacks,
        }
    }

    #[test]
    fn neutral_sliders_change_nothing() {
        let p = ToneParams::default();
        assert!(p.is_identity());
        for d in [0.0, 1.0, 2.5, 5.0, 12.0] {
            assert_eq!(local_stops(d, &p), 0.0);
            assert_eq!(endpoint_stops(d, &p), 0.0);
        }
        assert_eq!(apply([0.1, 0.2, 0.3], 3.0, &p), [0.1, 0.2, 0.3]);
    }

    #[test]
    fn shadows_act_on_dark_surroundings_and_highlights_on_bright_ones() {
        let s = params(0.0, 100.0, 0.0, 0.0);
        assert!(
            (local_stops(8.0, &s) - SHADOWS_STOPS).abs() < 1e-6,
            "deep shadow"
        );
        assert!(local_stops(2.47, &s) <= 0.15, "mid grey barely moves");
        assert_eq!(local_stops(0.5, &s), 0.0, "highlights untouched");
        let h = params(-100.0, 0.0, 0.0, 0.0);
        assert!(
            (local_stops(0.0, &h) + HIGHLIGHTS_STOPS).abs() < 1e-6,
            "white recovered"
        );
        assert!(local_stops(2.47, &h).abs() <= 0.15, "mid grey barely moves");
        assert_eq!(local_stops(6.0, &h), 0.0, "shadows untouched");
    }

    #[test]
    fn whites_and_blacks_move_the_ends_of_the_range() {
        let w = params(0.0, 0.0, 100.0, 0.0);
        assert!((endpoint_stops(0.0, &w) - WHITES_STOPS).abs() < 1e-6);
        assert_eq!(endpoint_stops(3.0, &w), 0.0);
        let b = params(0.0, 0.0, 0.0, -100.0);
        assert!((endpoint_stops(10.0, &b) + BLACKS_STOPS).abs() < 1e-6);
        assert_eq!(endpoint_stops(2.47, &b), 0.0, "mid grey untouched");
    }

    #[test]
    fn gains_keep_colour_ratios() {
        let out = apply([0.02, 0.01, 0.005], 6.0, &params(0.0, 60.0, 0.0, 0.0));
        assert!(out[0] > 0.02);
        assert!((out[0] / out[1] - 2.0).abs() < 1e-4 && (out[1] / out[2] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_flat_area_is_its_own_surroundings() {
        let (w, h) = (40, 30);
        let i = vec![-4.0f32; w * h];
        let base = ToneBase::from_log_luminance(w, h, &i);
        for (x, y) in [(0, 0), (20, 15), (39, 29)] {
            assert!((base.stops_at(x, y, w, h, -4.0) - 4.0).abs() < 1e-4);
        }
    }

    #[test]
    fn strong_edges_are_kept_so_they_do_not_glow() {
        // Left half 6 stops down, right half 1 stop down: a hard edge in the middle.
        let (w, h) = (64, 32);
        let i: Vec<f32> = (0..w * h)
            .map(|k| if k % w < w / 2 { -6.0 } else { -1.0 })
            .collect();
        let base = ToneBase::from_log_luminance(w, h, &i);
        // Right next to the edge, each side's surroundings stay close to its own level,
        // where a plain blur would average them (−3.5).
        let dark = base.stops_at(w / 2 - 2, h / 2, w, h, -6.0);
        let bright = base.stops_at(w / 2 + 1, h / 2, w, h, -1.0);
        assert!((dark - 6.0).abs() < 0.6, "dark side {dark}");
        assert!((bright - 1.0).abs() < 0.6, "bright side {bright}");
    }

    #[test]
    fn the_map_does_not_depend_on_render_size() {
        // The same scene at two sizes: preview and export must see the same surroundings.
        let scene = |w: u32, h: u32| {
            let data: Vec<u16> = (0..w * h)
                .flat_map(|k| {
                    let (x, y) = (k % w, k / w);
                    let v = if (x * 4 / w + y * 3 / h).is_multiple_of(2) {
                        3000
                    } else {
                        40000
                    };
                    [v; 3]
                })
                .collect();
            LinearImage::new(w, h, data).unwrap()
        };
        let small = ToneBase::build(&scene(600, 400), [1.0; 3]);
        let large = ToneBase::build(&scene(2400, 1600), [1.0; 3]);
        assert_eq!(small.size(), large.size());
        for (fx, fy) in [(0.1, 0.1), (0.5, 0.5), (0.8, 0.3)] {
            let at = |b: &ToneBase, w: usize, h: usize| {
                let (x, y) = ((fx * w as f32) as usize, (fy * h as f32) as usize);
                b.stops_at(x, y, w, h, -3.0)
            };
            assert!((at(&small, 600, 400) - at(&large, 2400, 1600)).abs() < 0.05);
        }
    }

    #[test]
    fn box_blur_averages_and_handles_borders() {
        let data = vec![0.0, 0.0, 3.0, 0.0, 0.0];
        let out = box_blur(&data, 5, 1, 1);
        assert_eq!(out, vec![0.0, 1.0, 1.0, 1.0, 0.0]);
    }
}
