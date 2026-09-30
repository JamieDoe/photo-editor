//! Remove chromatic aberration (ADR 0035): lateral colour fringing.
//!
//! A lens bends red, green and blue light by slightly different amounts, so the red
//! and blue images come out a little larger or smaller than the green one. Edges get
//! coloured fringes that grow towards the corners. The fix scales red and blue about
//! the centre so their edges line up with green's. It runs in the framing resample
//! (with crop, straighten and perspective), so it costs one cached pass.
//!
//! The scales are measured from the photo, without lens profiles ([`estimate`]), and
//! stored in the recipe, so previews and the export use the same numbers.
//!
//! A channel's scale at distance ρ from the centre (a share of the half-diagonal) is
//! `1 + a + b ρ²`: the corrected channel at a point samples the photo that much
//! further out.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use image_core::LinearImage;

use crate::ops::detail::{BlurScratch, box_plane};

/// Largest coefficient either way: 0.5 % of the half-diagonal (18 px at a 24 MP
/// corner), well beyond real lenses.
pub const MAX_COEFFICIENT: f32 = 0.005;

/// Measured lateral chromatic aberration: red's and blue's scale relative to green,
/// as `[a, b]` in `1 + a + b ρ²`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChromaticAberration {
    pub red: [f32; 2],
    pub blue: [f32; 2],
}

impl ChromaticAberration {
    /// Finite, within range, with no negative zeros (so hashing is stable).
    pub fn sanitized(self) -> Self {
        let clean = |v: f32| {
            let v = if v.is_finite() {
                v.clamp(-MAX_COEFFICIENT, MAX_COEFFICIENT)
            } else {
                0.0
            };
            if v == 0.0 { 0.0 } else { v }
        };
        Self {
            red: self.red.map(clean),
            blue: self.blue.map(clean),
        }
    }

    /// Changes no pixels.
    pub fn is_identity(&self) -> bool {
        let s = self.sanitized();
        s.red == [0.0; 2] && s.blue == [0.0; 2]
    }

    /// How far red and blue move at the corners of a `w` x `h` image, in pixels.
    pub fn corner_shift(&self, w: u32, h: u32) -> [f32; 2] {
        let r = half_diagonal(w as f32, h as f32);
        [
            (self.red[0] + self.red[1]).abs() * r,
            (self.blue[0] + self.blue[1]).abs() * r,
        ]
    }

    /// Bits for cache keys.
    pub fn to_bits(self) -> [u32; 4] {
        let s = self.sanitized();
        [s.red[0], s.red[1], s.blue[0], s.blue[1]].map(f32::to_bits)
    }
}

fn half_diagonal(w: f32, h: f32) -> f32 {
    0.5 * (w * w + h * h).sqrt()
}

/// Where red and blue are sampled for a point of a `w` x `h` source.
#[derive(Debug, Clone, Copy)]
pub struct Radial {
    cx: f32,
    cy: f32,
    inv_r2: f32,
    red: [f32; 2],
    blue: [f32; 2],
}

impl Radial {
    pub fn new(ca: &ChromaticAberration, w: f32, h: f32) -> Self {
        let s = ca.sanitized();
        let r = half_diagonal(w, h);
        Self {
            cx: w / 2.0,
            cy: h / 2.0,
            inv_r2: 1.0 / (r * r),
            red: s.red,
            blue: s.blue,
        }
    }

    /// Red's and blue's sample points for source point (`x`, `y`) (pixels, pixel
    /// centres at +0.5).
    #[inline]
    pub fn sample_points(&self, x: f32, y: f32) -> [(f32, f32); 2] {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let rho2 = (dx * dx + dy * dy) * self.inv_r2;
        [self.red, self.blue].map(|[a, b]| {
            let s = 1.0 + a + b * rho2;
            (self.cx + dx * s, self.cy + dy * s)
        })
    }
}

/// Tile side for the measurement, in pixels of the working image.
const TILE: usize = 32;
/// Added before the log, as a share of white, so shadow noise does not look like edges.
const LOG_OFFSET: f32 = 1.0 / 64.0;
/// Values at or above this are treated as clipped: channels clip at different levels,
/// which draws false colour edges.
const CLIP: u16 = 63_000;
/// A tile needs this much band-passed green energy (mean square, in stops²).
const MIN_ENERGY: f64 = 0.002;
/// Red (or blue) must follow green this closely in a tile: coloured edges do not.
const MIN_CORRELATION: f64 = 0.9;
/// Tiles that must qualify for an answer (in each half of the photo).
const MIN_TILES: usize = 16;
const ITERATIONS: usize = 8;

/// Measures the photo's lateral chromatic aberration, or `None` when it has too few
/// clean, neutral edges to go by, or its two halves disagree ([`Measurement`]).
pub fn estimate(image: &LinearImage) -> Option<ChromaticAberration> {
    measure(image)
        .filter(Measurement::is_reliable)
        .map(|m| m.correction)
}

/// A measurement, with how far its evidence agrees with itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub correction: ChromaticAberration,
    /// Red's and blue's shift at the corners, in pixels of the measured image.
    pub shift: [f32; 2],
    /// How far the two halves (alternate tiles, each covering the whole frame)
    /// disagree about those shifts, in the same pixels.
    pub spread: [f32; 2],
}

impl Measurement {
    /// The halves agree: within a fifth of a pixel plus a third of the shift.
    pub fn is_reliable(&self) -> bool {
        (0..2).all(|c| self.spread[c] <= 0.2 + self.shift[c].abs() / 3.0)
    }
}

/// See [`estimate`]: the measurement before the agreement check.
///
/// On log brightness, band-passed so each region's own colour drops out, red (and
/// blue) is warped towards green by a model with four numbers: the two scale
/// coefficients and a constant shift (from half-size RAW decoding, which takes red
/// and blue from different photosites; it is measured so it does not bias the scale,
/// but not applied). Each Gauss–Newton step solves for the change that best lines up
/// every edge pixel along its gradient, so edges of any direction contribute what they
/// can. Tiles count only when both channels see the same edges there (neutral, not
/// clipped); tiles that still disagree after a step are down-weighted (Huber).
pub fn measure(image: &LinearImage) -> Option<Measurement> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 4 * TILE || h < 4 * TILE {
        return None;
    }
    let planes = band_passed(image);
    let tiles = candidate_tiles(image, &planes[1], w, h);
    let halves: [Vec<(usize, usize)>; 2] = [0, 1].map(|k| {
        tiles
            .iter()
            .copied()
            .filter(|&(x, y)| (x / TILE + y / TILE) % 2 == k)
            .collect()
    });
    let r = half_diagonal(w as f32, h as f32);
    let mut fits = [[0.0f64; 4]; 2];
    let mut shift = [0.0f32; 2];
    let mut spread = [0.0f32; 2];
    for (k, c) in [0, 2].into_iter().enumerate() {
        let fit = |tiles: &[(usize, usize)], init: [f64; 4], min: usize| {
            fit_channel(&planes[c], &planes[1], tiles, w, h, r, init, min)
        };
        let a = fit(&halves[0], [0.0; 4], MIN_TILES)?;
        let b = fit(&halves[1], [0.0; 4], MIN_TILES)?;
        // All the evidence, starting from the halves' mean (a step or two).
        let mean = [0, 1, 2, 3].map(|i| 0.5 * (a[i] + b[i]));
        fits[k] = fit(&tiles, mean, 2 * MIN_TILES)?;
        shift[k] = (fits[k][0] + fits[k][1]) as f32;
        spread[k] = ((a[0] + a[1]) - (b[0] + b[1])).abs() as f32;
    }
    let round = |v: f64| ((v / f64::from(r)) * 1.0e6).round() as f32 / 1.0e6;
    let [red, blue] = fits;
    Some(Measurement {
        correction: ChromaticAberration {
            red: [round(red[0]), round(red[1])],
            blue: [round(blue[0]), round(blue[1])],
        }
        .sanitized(),
        shift,
        spread,
    })
}

/// Log brightness per channel, lightly smoothed, minus its surroundings.
fn band_passed(image: &LinearImage) -> [Vec<f32>; 3] {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let px = image.data().as_chunks::<3>().0;
    [0, 1, 2].map(|c| {
        let mut plane: Vec<f32> = px
            .iter()
            .map(|p| (f32::from(p[c]) / 65535.0 + LOG_OFFSET).log2())
            .collect();
        let mut scratch = BlurScratch::default();
        box_plane(&mut plane, w, h, 1, 2, &mut scratch);
        let mut around = plane.clone();
        box_plane(&mut around, w, h, 6, 2, &mut scratch);
        plane.iter_mut().zip(&around).for_each(|(v, a)| *v -= a);
        plane
    })
}

/// Top-left corners of tiles with enough green edges and nothing clipped. Tiles keep a
/// margin from the image's edges so warped samples stay inside.
fn candidate_tiles(image: &LinearImage, g: &[f32], w: usize, h: usize) -> Vec<(usize, usize)> {
    let data = image.data();
    let mut tiles = Vec::new();
    for ty in (TILE..h - 2 * TILE).step_by(TILE) {
        for tx in (TILE..w - 2 * TILE).step_by(TILE) {
            let mut energy = 0.0f64;
            let mut clipped = false;
            for y in ty..ty + TILE {
                let row = &g[y * w + tx..y * w + tx + TILE];
                energy += row.iter().map(|v| f64::from(v * v)).sum::<f64>();
                clipped |= data[(y * w + tx) * 3..(y * w + tx + TILE) * 3]
                    .iter()
                    .any(|&v| v >= CLIP);
            }
            if !clipped && energy / (TILE * TILE) as f64 >= MIN_ENERGY {
                tiles.push((tx, ty));
            }
        }
    }
    tiles
}

/// What one tile contributes to a Gauss–Newton step.
#[derive(Debug, Default, Clone, Copy)]
struct TileStep {
    /// Normal equations (J^T J, J^T r) for the four unknowns.
    a: [[f64; 4]; 4],
    v: [f64; 4],
    gg: f64,
    cc: f64,
    gc: f64,
    rr: f64,
}

/// `[a, b, tx, ty]` for channel `c` against green `g`, with `a` and `b` in pixels at
/// the corner (divide by the half-diagonal `r` for the recipe's coefficients), from
/// `init`; `None` with fewer than `min_tiles` usable tiles.
#[allow(clippy::too_many_arguments)]
fn fit_channel(
    c: &[f32],
    g: &[f32],
    tiles: &[(usize, usize)],
    w: usize,
    h: usize,
    r: f32,
    init: [f64; 4],
    min_tiles: usize,
) -> Option<[f64; 4]> {
    let mut theta = init;
    let mut used = 0;
    for _ in 0..ITERATIONS {
        let steps: Vec<TileStep> = tiles
            .par_iter()
            .map(|&(tx, ty)| tile_step(c, g, tx, ty, w, h, r, &theta))
            .collect();
        let valid: Vec<bool> = steps
            .iter()
            .map(|s| {
                let corr = s.gc / (s.gg * s.cc).sqrt().max(1e-12);
                let ratio = s.cc / s.gg.max(1e-12);
                corr >= MIN_CORRELATION && (0.5..=2.0).contains(&ratio)
            })
            .collect();
        // Relative residual per tile; Huber weights beyond twice the median.
        let mut rel: Vec<f64> = steps
            .iter()
            .zip(&valid)
            .filter(|(_, ok)| **ok)
            .map(|(s, _)| (s.rr / s.cc.max(1e-12)).sqrt())
            .collect();
        used = rel.len();
        if used < min_tiles {
            return None;
        }
        rel.sort_by(f64::total_cmp);
        let k = 2.0 * rel[rel.len() / 2];
        let mut a = [[0.0f64; 4]; 4];
        let mut v = [0.0f64; 4];
        for (s, _) in steps.iter().zip(&valid).filter(|(_, ok)| **ok) {
            let e = (s.rr / s.cc.max(1e-12)).sqrt();
            let wt = if e <= k { 1.0 } else { k / e };
            for ((vi, row), (svi, srow)) in v.iter_mut().zip(&mut a).zip(s.v.iter().zip(&s.a)) {
                *vi += wt * svi;
                for (x, sx) in row.iter_mut().zip(srow) {
                    *x += wt * sx;
                }
            }
        }
        // A little ridge on the ρ² term: without edges at many radii it is poorly
        // determined, and should then stay near 0.
        let trace = (0..4).map(|i| a[i][i]).sum::<f64>();
        a[1][1] += 1e-3 * trace;
        let delta = solve4(a, v)?;
        for i in 0..4 {
            theta[i] += delta[i];
        }
        // Stop once the corner moves by under a hundredth of a pixel.
        if delta[0].abs() + delta[1].abs() < 0.01 {
            break;
        }
    }
    (used >= min_tiles).then_some(theta)
}

/// One tile's normal equations at the current estimate `theta`.
#[allow(clippy::too_many_arguments)]
fn tile_step(
    c: &[f32],
    g: &[f32],
    tx: usize,
    ty: usize,
    w: usize,
    h: usize,
    r: f32,
    theta: &[f64; 4],
) -> TileStep {
    // The warped channel on the tile plus a one-pixel border, for its gradient.
    const S: usize = TILE + 2;
    let mut warped = [0.0f32; S * S];
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let t = theta.map(|v| v as f32);
    let rel = |x: usize, y: usize| {
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let (ux, uy) = (dx / r, dy / r);
        (ux, uy, ux * ux + uy * uy)
    };
    for j in 0..S {
        for i in 0..S {
            let (x, y) = (tx + i - 1, ty + j - 1);
            let (ux, uy, rho2) = rel(x, y);
            let s = t[0] + t[1] * rho2;
            warped[j * S + i] = sample(c, w, h, x as f32 + s * ux + t[2], y as f32 + s * uy + t[3]);
        }
    }
    let mut out = TileStep::default();
    for j in 1..=TILE {
        for i in 1..=TILE {
            let (gv, cv) = (
                f64::from(g[(ty + j - 1) * w + tx + i - 1]),
                f64::from(warped[j * S + i]),
            );
            out.gg += gv * gv;
            out.cc += cv * cv;
            out.gc += gv * cv;
        }
    }
    // The channel's own contrast in this tile: an edge of another height must not
    // read as a shifted one.
    let k = (out.gc / out.gg.max(1e-12)) as f32;
    for j in 1..=TILE {
        for i in 1..=TILE {
            let (x, y) = (tx + i - 1, ty + j - 1);
            let gv = k * g[y * w + x];
            let cv = warped[j * S + i];
            let gx = 0.5 * (g[y * w + x + 1] - g[y * w + x - 1]);
            let gy = 0.5 * (g[(y + 1) * w + x] - g[(y - 1) * w + x]);
            let wx = 0.5 * (warped[j * S + i + 1] - warped[j * S + i - 1]);
            let wy = 0.5 * (warped[(j + 1) * S + i] - warped[(j - 1) * S + i]);
            let (jx, jy) = (0.5 * (k * gx + wx), 0.5 * (k * gy + wy));
            let (ux, uy, rho2) = rel(x, y);
            let radial = jx * ux + jy * uy;
            let phi = [radial, radial * rho2, jx, jy].map(f64::from);
            let res = f64::from(gv - cv);
            for a in 0..4 {
                out.v[a] += phi[a] * res;
                for b in 0..4 {
                    out.a[a][b] += phi[a] * phi[b];
                }
            }
            out.rr += res * res;
        }
    }
    out
}

/// Bilinear sample of a plane at pixel-index coordinates, clamped to the edges.
#[inline]
fn sample(p: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = p[y0 * w + x0] + (p[y0 * w + x1] - p[y0 * w + x0]) * fx;
    let bottom = p[y1 * w + x0] + (p[y1 * w + x1] - p[y1 * w + x0]) * fx;
    top + (bottom - top) * fy
}

/// Solves a 4x4 system by Gaussian elimination with partial pivoting.
fn solve4(mut a: [[f64; 4]; 4], mut v: [f64; 4]) -> Option<[f64; 4]> {
    for col in 0..4 {
        let pivot = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        v.swap(col, pivot);
        let pivot_row = a[col];
        for row in col + 1..4 {
            let f = a[row][col] / pivot_row[col];
            for (x, p) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *x -= f * p;
            }
            v[row] -= f * v[col];
        }
    }
    let mut x = [0.0f64; 4];
    for row in (0..4).rev() {
        let s: f64 = (row + 1..4).map(|k| a[row][k] * x[k]).sum();
        x[row] = (v[row] - s) / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uniform noise in 0..1 (splitmix64).
    fn hash(x: u64) -> f32 {
        let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A neutral scene of tilted blocks with hard edges (brightness 0..1).
    fn scene(x: f32, y: f32, c: usize, colourful: bool) -> f32 {
        let t = 0.17f32;
        let (u, v) = (x * t.cos() - y * t.sin(), x * t.sin() + y * t.cos());
        let (i, j) = ((u / 23.0).floor() as i64, (v / 17.0).floor() as i64);
        let block = (i as u64).wrapping_mul(1_000_003) ^ (j as u64);
        // Colourful: each block its own colour.
        let tint = if colourful {
            0.3 + 0.7 * hash(block.wrapping_mul(7) + c as u64)
        } else {
            1.0
        };
        (0.08 + 0.8 * hash(block)) * tint
    }

    /// The scene as a lens with lateral CA draws it: red magnified by `m_red` (and
    /// shifted by `shift`), blue by `m_blue`, each pixel supersampled 3x3. Where
    /// `tint` is set, the left third is coloured (a colour edge, not CA).
    fn photo(
        w: u32,
        h: u32,
        m_red: f32,
        m_blue: f32,
        shift: (f32, f32),
        tint: bool,
    ) -> LinearImage {
        photo_of(w, h, m_red, m_blue, shift, tint, false)
    }

    fn photo_of(
        w: u32,
        h: u32,
        m_red: f32,
        m_blue: f32,
        shift: (f32, f32),
        tint: bool,
        colourful: bool,
    ) -> LinearImage {
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for sj in 0..3 {
                    for si in 0..3 {
                        let (px, py) = (
                            x as f32 + (si as f32 + 0.5) / 3.0,
                            y as f32 + (sj as f32 + 0.5) / 3.0,
                        );
                        let at = |c: usize, m: f32, s: (f32, f32)| {
                            let (sx, sy) = (cx + (px - s.0 - cx) / m, cy + (py - s.1 - cy) / m);
                            scene(sx, sy, c, colourful)
                        };
                        acc[0] += at(0, m_red, shift);
                        acc[1] += at(1, 1.0, (0.0, 0.0));
                        acc[2] += at(2, m_blue, (0.0, 0.0));
                    }
                }
                let gain = if tint && x < w / 3 {
                    [1.4, 1.0, 0.6]
                } else {
                    [1.0; 3]
                };
                for c in 0..3 {
                    let n = 0.004 * (hash(u64::from(y * w + x) * 3 + c as u64) - 0.5);
                    let v = (acc[c] / 9.0 * gain[c] * 0.7 + n).clamp(0.0, 1.0);
                    data.push((v * 60_000.0) as u16);
                }
            }
        }
        LinearImage::new(w, h, data).unwrap()
    }

    #[test]
    fn measures_the_red_and_blue_scale() {
        let (w, h) = (600, 400);
        let img = photo(w, h, 1.0015, 0.999, (0.25, 0.25), true);
        let ca = estimate(&img).expect("an estimate");
        let [red, blue] = [ca.red[0] + ca.red[1], ca.blue[0] + ca.blue[1]];
        // At the corner: 0.54 px red, -0.36 px blue.
        assert!((red - 0.0015).abs() < 0.00015, "{ca:?}");
        assert!((blue + 0.001).abs() < 0.00015, "{ca:?}");
    }

    #[test]
    fn converges_from_two_pixels_out() {
        // 1.8 px at the corner, the far end of real lenses at this size.
        let img = photo(600, 400, 1.005, 0.996, (0.0, 0.0), false);
        let ca = estimate(&img).expect("an estimate");
        let [red, blue] = [ca.red[0] + ca.red[1], ca.blue[0] + ca.blue[1]];
        assert!((red - 0.005).abs() < 0.0003, "{ca:?}");
        assert!((blue + 0.004).abs() < 0.0003, "{ca:?}");
    }

    #[test]
    fn finds_nothing_in_a_clean_photo() {
        let img = photo(600, 400, 1.0, 1.0, (0.0, 0.0), false);
        let ca = estimate(&img).expect("an estimate");
        let shift = ca.corner_shift(600, 400);
        assert!(shift[0] < 0.05 && shift[1] < 0.05, "{ca:?}");
    }

    #[test]
    fn colour_edges_do_not_read_as_fringes() {
        // Every edge is between two colours, so each channel steps by its own amount:
        // declined, or nothing found, but never fringes.
        let img = photo_of(600, 400, 1.0, 1.0, (0.0, 0.0), false, true);
        if let Some(ca) = estimate(&img) {
            let shift = ca.corner_shift(600, 400);
            assert!(shift[0] < 0.05 && shift[1] < 0.05, "{ca:?}");
        }
    }

    #[test]
    fn declines_without_edges() {
        let data = (0..300 * 200 * 3)
            .map(|i| 20_000 + (hash(i) * 200.0) as u16)
            .collect();
        assert_eq!(estimate(&LinearImage::new(300, 200, data).unwrap()), None);
    }

    #[test]
    fn sampling_scales_about_the_centre() {
        let ca = ChromaticAberration {
            red: [0.001, 0.0],
            blue: [-0.002, 0.001],
        };
        let r = Radial::new(&ca, 600.0, 400.0);
        assert_eq!(r.sample_points(300.0, 200.0), [(300.0, 200.0); 2]);
        let [(rx, _), (bx, _)] = r.sample_points(600.0, 200.0);
        assert!((rx - 600.3).abs() < 1e-3, "{rx}");
        // ρ² = 300² / 360.55² = 0.692: scale 1 - 0.002 + 0.000692.
        assert!(
            (bx - (300.0 + 300.0 * (1.0 - 0.002 + 0.001 * 0.6923))).abs() < 1e-3,
            "{bx}"
        );
        assert!((ca.corner_shift(600, 400)[1] - 0.001 * 360.555).abs() < 1e-3);
    }

    #[test]
    fn sanitising_clamps_and_cleans() {
        let ca = ChromaticAberration {
            red: [1.0, f32::NAN],
            blue: [-0.0, -1.0],
        }
        .sanitized();
        assert_eq!(ca.red, [MAX_COEFFICIENT, 0.0]);
        assert_eq!(ca.blue[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(ca.blue[1], -MAX_COEFFICIENT);
        assert!(ChromaticAberration::default().is_identity());
    }
}
