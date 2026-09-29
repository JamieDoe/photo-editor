//! The scene at map resolution, and edge-aware maps built from it (ADR 0023, 0028).
//!
//! Stages that depend on a pixel's surroundings (Highlights and Shadows, Clarity,
//! Dehaze) work from a small colour map of the whole image: the source after white
//! balance and exposure, area-averaged to [`MAP_LONG_EDGE`] pixels on its long edge.
//! A fixed map size makes them resolution independent (the preview and the export
//! see the same surroundings) and cheap.
//!
//! A [`GuidedMap`] is a guided filter's per-cell linear model (`a`, `b`): evaluated
//! at a full-resolution pixel with that pixel's own guide value, `a * guide + b`
//! follows edges finer than the map.

use image_core::LinearImage;
use image_core::color::REC709_LUMA;
use rayon::prelude::*;

/// Long edge of the scene map in cells.
pub const MAP_LONG_EDGE: u32 = 256;

/// Area-averaged linear RGB of the scene (after `gains`), [`MAP_LONG_EDGE`] cells on
/// its long edge.
#[derive(Debug, Clone)]
pub struct SceneMap {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[f32; 3]>,
}

impl SceneMap {
    /// The map of `source` after per-channel `gains`. Averaging is linear, so this is
    /// [`SceneMap::unit`] scaled by the gains: renders cache the unit map and only
    /// rescale it while white balance or exposure change.
    pub fn build(source: &LinearImage, gains: [f32; 3]) -> Self {
        Self::unit(source).scaled(gains)
    }

    /// This map after per-channel `gains`.
    pub fn scaled(&self, gains: [f32; 3]) -> Self {
        Self {
            width: self.width,
            height: self.height,
            rgb: self
                .rgb
                .iter()
                .map(|c| [c[0] * gains[0], c[1] * gains[1], c[2] * gains[2]])
                .collect(),
        }
    }

    /// The map of `source` as decoded (no gains).
    pub fn unit(source: &LinearImage) -> Self {
        Self::unit_sized(source, MAP_LONG_EDGE)
    }

    /// Like [`SceneMap::unit`], with `long_edge` cells on the long edge (at most the
    /// image's own size).
    pub fn unit_sized(source: &LinearImage, long_edge: u32) -> Self {
        let (sw, sh) = (source.width() as usize, source.height() as usize);
        let long = sw.max(sh).max(1);
        let scale = f64::from(long_edge).min(long as f64) / long as f64;
        let w = ((sw as f64 * scale).round() as usize).max(1);
        let h = ((sh as f64 * scale).round() as usize).max(1);
        let norm = 1.0 / 65535.0;

        // One map row per task. f32 partial sums per source row (at most ~50 pixels
        // per cell per row), accumulated across rows in f64.
        let cell_of: Vec<u32> = (0..sw).map(|x| (x * w / sw) as u32).collect();
        let rgb: Vec<[f32; 3]> = (0..h)
            .into_par_iter()
            .flat_map_iter(|my| {
                let (y0, y1) = (my * sh / h, ((my + 1) * sh / h).max(my * sh / h + 1));
                let mut sums = vec![[0.0f64; 3]; w];
                let mut counts = vec![0u32; w];
                let mut row_sums = vec![[0.0f32; 3]; w];
                for y in y0..y1.min(sh) {
                    row_sums.iter_mut().for_each(|s| *s = [0.0; 3]);
                    let row = source.row(y as u32);
                    for (px, &cell) in row.as_chunks::<3>().0.iter().zip(&cell_of) {
                        let s = &mut row_sums[cell as usize];
                        s[0] += f32::from(px[0]);
                        s[1] += f32::from(px[1]);
                        s[2] += f32::from(px[2]);
                        counts[cell as usize] += 1;
                    }
                    for (s, r) in sums.iter_mut().zip(&row_sums) {
                        for c in 0..3 {
                            s[c] += f64::from(r[c]);
                        }
                    }
                }
                sums.into_iter().zip(counts).map(move |(s, n)| {
                    if n == 0 {
                        [0.0; 3]
                    } else {
                        let n = f64::from(n);
                        [0, 1, 2].map(|c| (s[c] / n * norm) as f32)
                    }
                })
            })
            .collect();
        Self {
            width: w,
            height: h,
            rgb,
        }
    }

    /// Linear Rec.709 luminance of each cell.
    pub fn luminance(&self) -> Vec<f32> {
        self.rgb.iter().map(|&c| luma(c)).collect()
    }
}

#[inline]
pub fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * REC709_LUMA[0] + rgb[1] * REC709_LUMA[1] + rgb[2] * REC709_LUMA[2]
}

/// A guided filter's linear model per map cell, smoothed: `a * guide + b`.
#[derive(Debug, Clone)]
pub struct GuidedMap {
    width: usize,
    height: usize,
    a: Vec<f32>,
    b: Vec<f32>,
}

impl GuidedMap {
    /// Filters `input` guided by `guide` (both `w` x `h`) with box radius `r` and
    /// regularisation `eps`: where the guide varies much more than `eps`, the output
    /// follows it (edges are kept); where it is flat, the output is a local mean.
    pub fn new(guide: &[f32], input: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Self {
        let gg: Vec<f32> = guide.iter().map(|v| v * v).collect();
        let gi: Vec<f32> = guide.iter().zip(input).map(|(g, i)| g * i).collect();
        let mean_g = box_blur(guide, w, h, r);
        let mean_i = box_blur(input, w, h, r);
        let mean_gg = box_blur(&gg, w, h, r);
        let mean_gi = box_blur(&gi, w, h, r);
        let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
            .map(|k| {
                let var = (mean_gg[k] - mean_g[k] * mean_g[k]).max(0.0);
                let cov = mean_gi[k] - mean_g[k] * mean_i[k];
                let a = cov / (var + eps);
                (a, mean_i[k] - a * mean_g[k])
            })
            .unzip();
        Self {
            width: w,
            height: h,
            a: box_blur(&a, w, h, r),
            b: box_blur(&b, w, h, r),
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// The model at pixel (`x`, `y`) of an `image_w` x `image_h` image whose guide
    /// value there is `guide` (bilinear between cells).
    pub fn value_at(&self, x: usize, y: usize, image_w: usize, image_h: usize, guide: f32) -> f32 {
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
        lerp2(&self.a) * guide + lerp2(&self.b)
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
                    x0 as u32,
                    ((x0 + 1).min(self.width - 1)) as u32,
                    gx - x0 as f32,
                )
            })
            .collect()
    }

    /// The model's `a` and `b` for pixel row `y` of an `image_h`-tall image, at every
    /// map column (vertical interpolation done), into `a_row` and `b_row`.
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

    /// The model's raw per-cell values (for map-resolution uses).
    pub fn cell(&self, k: usize, guide: f32) -> f32 {
        self.a[k] * guide + self.b[k]
    }
}

/// Evaluates a [`GuidedMap`] row by row: the interpolated `a` and `b` of one pixel
/// row, then pixels by column. Reuses its buffers.
#[derive(Debug, Default)]
pub struct RowModel {
    a: Vec<f32>,
    b: Vec<f32>,
}

impl RowModel {
    pub fn load(&mut self, map: &GuidedMap, y: usize, image_h: usize) {
        map.row(y, image_h, &mut self.a, &mut self.b);
    }

    #[inline]
    pub fn eval(&self, col: (u32, u32, f32), guide: f32) -> f32 {
        let (x0, x1, t) = (col.0 as usize, col.1 as usize, col.2);
        let a = self.a[x0] + (self.a[x1] - self.a[x0]) * t;
        let b = self.b[x0] + (self.b[x1] - self.b[x0]) * t;
        a * guide + b
    }
}

/// Mean over a (2r+1)² window, clamped at the borders (divides by the pixels inside).
pub fn box_blur(data: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
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

    #[test]
    fn map_averages_cells_and_applies_gains() {
        let (w, h) = (512u32, 256u32);
        let data: Vec<u16> = (0..w * h).flat_map(|_| [30000, 20000, 10000]).collect();
        let img = LinearImage::new(w, h, data).unwrap();
        let map = SceneMap::build(&img, [2.0, 1.0, 0.5]);
        assert_eq!((map.width, map.height), (256, 128));
        for c in &map.rgb {
            assert!((c[0] - 60000.0 / 65535.0).abs() < 1e-5);
            assert!((c[1] - 20000.0 / 65535.0).abs() < 1e-5);
            assert!((c[2] - 5000.0 / 65535.0).abs() < 1e-5);
        }
    }

    #[test]
    fn guided_map_keeps_edges_and_smooths_flat_areas() {
        let (w, h) = (64, 32);
        let guide: Vec<f32> = (0..w * h)
            .map(|k| if k % w < w / 2 { 0.0 } else { 1.0 })
            .collect();
        // Noisy input that follows the guide's step.
        let input: Vec<f32> = (0..w * h)
            .map(|k| guide[k] + if (k * 7) % 3 == 0 { 0.05 } else { -0.05 })
            .collect();
        let m = GuidedMap::new(&guide, &input, w, h, 4, 1e-3);
        // Either side of the step keeps its level (within the noise's mean).
        for x in [5, w / 2 - 2, w / 2 + 1, w - 5] {
            let v = m.cell(16 * w + x, guide[16 * w + x]);
            assert!((v - guide[16 * w + x]).abs() < 0.05, "x {x}: {v}");
        }
    }

    #[test]
    fn box_blur_averages_and_handles_borders() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let out = box_blur(&data, 4, 1, 1);
        assert_eq!(out, vec![1.5, 2.0, 3.0, 3.5]);
    }
}
