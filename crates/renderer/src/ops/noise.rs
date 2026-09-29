//! Noise reduction (ADR 0030): luminance and colour noise, one slider.
//!
//! Both parts are guided filters (He et al.) over the planes the detail stage already
//! measures, at radii that are a fixed fraction of the image, so the preview (where
//! downscaling has already averaged noise away) and the export look alike:
//!
//! - **Luminance**: log2 luminance, self-guided. Variations smaller than the noise
//!   expected at that brightness are smoothed; larger ones (detail, edges) are kept.
//!   Noise in log terms grows as tones get darker (shot noise), so the threshold
//!   doubles every two stops down.
//! - **Colour**: the pixel's colour as ratios R/Y and B/Y, guided by log luminance
//!   over a wider radius. Colour blotches (which have no luminance edge) are
//!   averaged; colour changes that coincide with a luminance edge are kept. Luminance
//!   is untouched by this part.
//!
//! The luminance part is a gain in stops (hue-stable); the colour part sets the
//! ratios and keeps luminance.

use super::dehaze::DehazeModel;
use super::detail::{BlurScratch, box_plane};

use super::scene::SceneMap;
use image_core::color::REC709_LUMA;

/// Luminance filter radius as a fraction of the long edge (1 px at 1516, 5 at 6064).
const LUMA_RADIUS_FRACTION: f32 = 0.0008;
/// Expected luminance noise at white, in stops, at slider 100.
const LUMA_NOISE_AT_WHITE: f32 = 0.03;
/// Luminance steps (stops) that colour smoothing respects at slider 100.
const CHROMA_EDGE_STOPS: f32 = 0.3;

/// Slider 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseParams {
    pub amount: f32,
}

impl NoiseParams {
    pub fn is_identity(&self) -> bool {
        self.amount == 0.0
    }

    fn strength(&self) -> f32 {
        (self.amount / 100.0).clamp(0.0, 1.0)
    }

    /// Regularisation of the luminance filter where the local mean log luminance is
    /// `mean_log`: the square of twice the expected noise (so noise is well inside it).
    #[inline]
    pub fn luma_eps(&self, mean_log: f32) -> f32 {
        let d = (-mean_log).max(0.0);
        let sigma = (LUMA_NOISE_AT_WHITE * (0.5 * d).exp2()).min(0.6);
        let e = 2.0 * self.strength() * sigma;
        e * e + 1.0e-8
    }

    pub fn chroma_eps(&self) -> f32 {
        let e = CHROMA_EDGE_STOPS * self.strength();
        e * e + 1.0e-6
    }

    /// Share of the filtered colour used (full from slider 50).
    pub fn chroma_mix(&self) -> f32 {
        (2.0 * self.strength()).min(1.0)
    }

    /// Largest colour change (in R/Y or B/Y) treated as noise: beyond it (up to twice
    /// it) the smoothed colour fades out, so real colour edges keep their colour even
    /// where the luminance guide cannot tell them apart.
    pub fn chroma_limit(&self) -> f32 {
        0.1 + 0.25 * self.strength()
    }
}

pub fn luma_radius(width: usize, height: usize) -> usize {
    ((width.max(height) as f32 * LUMA_RADIUS_FRACTION).round() as usize).clamp(1, 16)
}

/// Buffers [`guided`] reuses between calls.
#[derive(Debug, Default)]
pub struct GuidedScratch {
    /// The guide's box mean and variance (shared by filters with the same guide).
    mean_g: Vec<f32>,
    var_g: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    tmp: Vec<f32>,
    blur: BlurScratch,
}

/// Fills `s.mean_g` and `s.var_g` for `guide`.
fn guide_stats(guide: &[f32], w: usize, h: usize, r: usize, s: &mut GuidedScratch) {
    s.mean_g.clear();
    s.mean_g.extend_from_slice(guide);
    box_plane(&mut s.mean_g, w, h, r, 1, &mut s.blur);
    s.var_g.clear();
    s.var_g.extend(guide.iter().map(|g| g * g));
    box_plane(&mut s.var_g, w, h, r, 1, &mut s.blur);
    for (v, m) in s.var_g.iter_mut().zip(&s.mean_g) {
        *v = (*v - m * m).max(0.0);
    }
}

/// The guided filter's output given the guide's stats in `s`: `input` (the guide
/// itself when `None`), regularisation `eps` of the guide's local mean.
#[allow(clippy::too_many_arguments)]
fn guided_with_stats(
    guide: &[f32],
    input: Option<&[f32]>,
    w: usize,
    h: usize,
    r: usize,
    eps: &impl Fn(f32) -> f32,
    s: &mut GuidedScratch,
    out: &mut Vec<f32>,
) {
    s.a.clear();
    s.b.clear();
    match input {
        // Self-guided: the covariance is the variance and the input mean the guide's.
        None => {
            for (&m, &var) in s.mean_g.iter().zip(&s.var_g) {
                let a = var / (var + eps(m));
                s.a.push(a);
                s.b.push(m - a * m);
            }
        }
        Some(input) => {
            // b holds the input mean, tmp the mean of guide x input.
            s.b.extend_from_slice(input);
            box_plane(&mut s.b, w, h, r, 1, &mut s.blur);
            s.tmp.clear();
            s.tmp.extend(guide.iter().zip(input).map(|(g, i)| g * i));
            box_plane(&mut s.tmp, w, h, r, 1, &mut s.blur);
            for k in 0..w * h {
                let (m, mi) = (s.mean_g[k], s.b[k]);
                let a = (s.tmp[k] - m * mi) / (s.var_g[k] + eps(m));
                s.a.push(a);
                s.b[k] = mi - a * m;
            }
        }
    }
    box_plane(&mut s.a, w, h, r, 1, &mut s.blur);
    box_plane(&mut s.b, w, h, r, 1, &mut s.blur);
    out.clear();
    out.extend(
        guide
            .iter()
            .zip(&s.a)
            .zip(&s.b)
            .map(|((g, a), b)| a * g + b),
    );
}

/// Guided filter of `input` (or of `guide` itself when `None`) over a `w` x `h`
/// plane with box radius `r`, into `out`. The regularisation may depend on the
/// guide's local mean. Reads `2 * r` pixels around each output pixel, so a band of a
/// larger image gives the whole image's result `2 * r` rows in from its cut edges.
#[allow(clippy::too_many_arguments)]
pub fn guided(
    guide: &[f32],
    input: Option<&[f32]>,
    w: usize,
    h: usize,
    r: usize,
    eps: impl Fn(f32) -> f32,
    s: &mut GuidedScratch,
    out: &mut Vec<f32>,
) {
    guide_stats(guide, w, h, r, s);
    guided_with_stats(guide, input, w, h, r, &eps, s, out);
}

/// [`NoiseParams::luma_eps`] tabulated every 1/32 stop over 0..24 stops below white,
/// so the per-pixel filter does not evaluate a power.
struct EpsTable(Vec<f32>);

impl EpsTable {
    const STEPS: f32 = 32.0;
    const MAX_STOPS: f32 = 24.0;

    fn new(p: &NoiseParams) -> Self {
        let n = (Self::MAX_STOPS * Self::STEPS) as usize;
        Self(
            (0..=n)
                .map(|i| p.luma_eps(-(i as f32) / Self::STEPS))
                .collect(),
        )
    }

    #[inline]
    fn at(&self, mean_log: f32) -> f32 {
        let pos = (-mean_log).clamp(0.0, Self::MAX_STOPS - 1e-3) * Self::STEPS;
        let i = pos as usize;
        let t = pos - i as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * t
    }
}

/// The denoised log luminance of a `w` x `h` plane (radius from the full image's
/// size: [`luma_radius`]).
pub fn denoise_luma(
    p: &NoiseParams,
    log_y: &[f32],
    w: usize,
    h: usize,
    radius: usize,
    s: &mut GuidedScratch,
    out: &mut Vec<f32>,
) {
    let table = EpsTable::new(p);
    guide_stats(log_y, w, h, radius, s);
    guided_with_stats(log_y, None, w, h, radius, &|m| table.at(m), s, out);
}

/// Smoothed colour ratios of `w` x `h` planes `rr` and `rb` (R/Y, B/Y), guided by log
/// luminance, into `out_r` and `out_b`. The guide's
/// statistics are computed once for both.
#[allow(clippy::too_many_arguments)]
pub fn denoise_chroma(
    p: &NoiseParams,
    log_y: &[f32],
    rr: &[f32],
    rb: &[f32],
    w: usize,
    h: usize,
    radius: usize,
    s: &mut GuidedScratch,
    out_r: &mut Vec<f32>,
    out_b: &mut Vec<f32>,
) {
    let eps = p.chroma_eps();
    guide_stats(log_y, w, h, radius, s);
    guided_with_stats(log_y, Some(rr), w, h, radius, &|_| eps, s, out_r);
    guided_with_stats(log_y, Some(rb), w, h, radius, &|_| eps, s, out_b);
}

/// Colour is smoothed on a map of the whole image with this many cells on its long
/// edge (colour noise is low-frequency). A fixed size, like the scene map's, so the
/// preview and the export smooth the same structures; built once per render.
pub const CHROMA_MAP_LONG_EDGE: u32 = 512;
/// The colour filter's radius on that map, in cells.
const CHROMA_MAP_RADIUS: usize = 2;

/// Smoothed colour ratios (R/Y, B/Y) of the whole image, on a map of
/// [`CHROMA_MAP_LONG_EDGE`] cells.
#[derive(Debug, Clone)]
pub struct ChromaMap {
    width: usize,
    height: usize,
    qr: Vec<f32>,
    qb: Vec<f32>,
}

impl ChromaMap {
    /// From the image's gain-free map at chroma resolution
    /// (`SceneMap::unit_sized(source, CHROMA_MAP_LONG_EDGE)`), after the white balance
    /// and exposure `gains` and any `dehaze` before the stage: the colour the stage
    /// measures, as the luminance plane is.
    pub fn build(
        unit: &SceneMap,
        gains: [f32; 3],
        dehaze: Option<&DehazeModel>,
        p: &NoiseParams,
    ) -> Self {
        let (w, h) = (unit.width, unit.height);
        let gains = Self::effective_gains(gains, dehaze.is_some());
        let mut guide = Vec::with_capacity(w * h);
        let mut rr = Vec::with_capacity(w * h);
        let mut rb = Vec::with_capacity(w * h);
        for (k, c) in unit.rgb.iter().enumerate() {
            let mut c = [c[0] * gains[0], c[1] * gains[1], c[2] * gains[2]];
            if let Some(d) = dehaze {
                // The dehaze model's map covers the whole image; the cells stand in
                // for pixels of a w x h image.
                let t = d.map().value_at(k % w, k / w, w, h, d.guide(luma(c)));
                c = d.apply(c, t);
            }
            guide.push(super::detail::fast_log2(luma(c)));
            let (r, b) = ratios(c);
            rr.push(r);
            rb.push(b);
        }
        // The guide's statistics once, then both channels in parallel.
        let eps = p.chroma_eps();
        let mut sr = GuidedScratch::default();
        guide_stats(&guide, w, h, CHROMA_MAP_RADIUS, &mut sr);
        let mut sb = GuidedScratch {
            mean_g: sr.mean_g.clone(),
            var_g: sr.var_g.clone(),
            ..GuidedScratch::default()
        };
        let (mut qr, mut qb) = (Vec::new(), Vec::new());
        let r = CHROMA_MAP_RADIUS;
        rayon::join(
            || guided_with_stats(&guide, Some(&rr), w, h, r, &|_| eps, &mut sr, &mut qr),
            || guided_with_stats(&guide, Some(&rb), w, h, r, &|_| eps, &mut sb, &mut qb),
        );
        Self {
            width: w,
            height: h,
            qr,
            qb,
        }
    }

    /// The gains the map depends on. Without dehaze, only their ratios matter: the
    /// colour ratios ignore a common factor, and the guided filter ignores a constant
    /// shift of its log-luminance guide. So exposure is divided out, and dragging
    /// exposure reuses the map.
    pub fn effective_gains(gains: [f32; 3], dehaze: bool) -> [f32; 3] {
        if dehaze || gains[1] <= 0.0 {
            gains
        } else {
            gains.map(|g| g / gains[1])
        }
    }

    /// Map columns and weights for each of `image_w` pixel columns.
    pub fn columns(&self, image_w: usize) -> Vec<(u32, u32, f32)> {
        let w = self.width;
        (0..image_w)
            .map(|x| {
                let g =
                    ((x as f32 + 0.5) * w as f32 / image_w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
                let x0 = g as usize;
                (x0 as u32, (x0 + 1).min(w - 1) as u32, g - x0 as f32)
            })
            .collect()
    }

    /// The ratios for pixel row `y` of an `image_h`-tall image, interpolated
    /// vertically, one per map column, into `row`.
    pub fn load_row(&self, y: usize, image_h: usize, row: &mut Vec<(f32, f32)>) {
        let h = self.height;
        let g = ((y as f32 + 0.5) * h as f32 / image_h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (g as usize, g - (g as usize) as f32);
        let y1 = (y0 + 1).min(h - 1);
        let w = self.width;
        row.clear();
        row.extend((0..w).map(|x| {
            let r = self.qr[y0 * w + x] + (self.qr[y1 * w + x] - self.qr[y0 * w + x]) * ty;
            let b = self.qb[y0 * w + x] + (self.qb[y1 * w + x] - self.qb[y0 * w + x]) * ty;
            (r, b)
        }));
    }

    /// The ratios at a pixel, from its row (see [`ChromaMap::load_row`]) and column.
    #[inline]
    pub fn at(row: &[(f32, f32)], col: (u32, u32, f32)) -> (f32, f32) {
        let (a, b) = (row[col.0 as usize], row[col.1 as usize]);
        (a.0 + (b.0 - a.0) * col.2, a.1 + (b.1 - a.1) * col.2)
    }
}

/// Colour ratios (R/Y, B/Y) of a linear RGB pixel; neutral for black.
#[inline]
pub fn ratios(rgb: [f32; 3]) -> (f32, f32) {
    let y = luma(rgb);
    if y <= 1.0e-9 {
        (1.0, 1.0)
    } else {
        (rgb[0] / y, rgb[2] / y)
    }
}

/// The colour part for one pixel: towards the smoothed ratios (`rr`, `rb`) by the
/// slider's mix, where the change is noise-sized (see [`NoiseParams::chroma_limit`]).
#[inline]
pub fn denoise_colour(rgb: [f32; 3], rr: f32, rb: f32, p: &ColourStep) -> [f32; 3] {
    let y = luma(rgb);
    if y <= 1.0e-9 {
        return rgb;
    }
    let inv = 1.0 / y;
    let weight = |d: f32| {
        let t = ((d.abs() - p.limit) * p.inv_limit).clamp(0.0, 1.0);
        p.mix * (1.0 - t * t * (3.0 - 2.0 * t))
    };
    let (r0, b0) = (rgb[0] * inv, rgb[2] * inv);
    let (dr, db) = (rr - r0, rb - b0);
    let r1 = (r0 + dr * weight(dr)).max(0.0);
    let b1 = (b0 + db * weight(db)).max(0.0);
    let [wr, _, wb] = REC709_LUMA;
    let g = ((1.0 - wr * r1 - wb * b1) * INV_WG).max(0.0);
    [r1 * y, g * y, b1 * y]
}

const INV_WG: f32 = 1.0 / REC709_LUMA[1];

/// [`NoiseParams`]'s colour constants, computed once per render.
#[derive(Debug, Clone, Copy)]
pub struct ColourStep {
    mix: f32,
    limit: f32,
    inv_limit: f32,
}

impl ColourStep {
    pub fn new(p: &NoiseParams) -> Self {
        let limit = p.chroma_limit();
        Self {
            mix: p.chroma_mix(),
            limit,
            inv_limit: 1.0 / limit,
        }
    }
}

/// `rgb` with its colour ratios moved `mix` of the way to (`rr`, `rb`), keeping its
/// luminance.
#[inline]
pub fn with_ratios(rgb: [f32; 3], rr: f32, rb: f32, mix: f32) -> [f32; 3] {
    let y = luma(rgb);
    if y <= 1.0e-9 {
        return rgb;
    }
    let (r0, b0) = (rgb[0] / y, rgb[2] / y);
    let (r1, b1) = (
        (r0 + (rr - r0) * mix).max(0.0),
        (b0 + (rb - b0) * mix).max(0.0),
    );
    let [wr, wg, wb] = REC709_LUMA;
    let g = ((1.0 - wr * r1 - wb * b1) / wg).max(0.0);
    [r1 * y, g * y, b1 * y]
}

#[inline]
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * REC709_LUMA[0] + rgb[1] * REC709_LUMA[1] + rgb[2] * REC709_LUMA[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(k: usize) -> f32 {
        // Deterministic pseudo-noise in -1..1.
        let x = (k as u32).wrapping_mul(2_654_435_761) >> 8;
        (x % 2001) as f32 / 1000.0 - 1.0
    }

    fn spread(v: &[f32]) -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn luminance_noise_is_smoothed_and_edges_kept() {
        let (w, h) = (120, 60);
        let p = NoiseParams { amount: 100.0 };
        // A 2-stop step at mid grey with 0.05-stop noise.
        let clean = |x: usize| if x < w / 2 { -4.0 } else { -2.0 };
        let plane: Vec<f32> = (0..w * h).map(|k| clean(k % w) + 0.05 * noise(k)).collect();
        let mut out = Vec::new();
        guided(
            &plane,
            None,
            w,
            h,
            2,
            |m| p.luma_eps(m),
            &mut GuidedScratch::default(),
            &mut out,
        );
        let region = |v: &[f32], x0: usize| -> Vec<f32> {
            (10..50)
                .flat_map(|y| (x0..x0 + 20).map(move |x| (y, x)))
                .map(|(y, x)| v[y * w + x])
                .collect()
        };
        assert!(spread(&region(&out, 10)) < 0.5 * spread(&region(&plane, 10)));
        // Either side of the edge keeps its level.
        for (x, want) in [(w / 2 - 3, -4.0), (w / 2 + 2, -2.0)] {
            let v = out[30 * w + x];
            assert!((v - want).abs() < 0.2, "x {x}: {v}");
        }
    }

    #[test]
    fn darker_tones_are_smoothed_more() {
        let p = NoiseParams { amount: 60.0 };
        assert!(p.luma_eps(-8.0) > 10.0 * p.luma_eps(-2.0));
        assert!(NoiseParams { amount: 0.0 }.luma_eps(-8.0) < 1.0e-6);
    }

    #[test]
    fn colour_ratios_round_trip_and_keep_luminance() {
        let rgb = [0.4, 0.25, 0.1];
        let (rr, rb) = ratios(rgb);
        let same = with_ratios(rgb, rr, rb, 1.0);
        assert!(
            same.iter().zip(rgb).all(|(a, b)| (a - b).abs() < 1e-5),
            "{same:?}"
        );
        let grey = with_ratios(rgb, 1.0, 1.0, 1.0);
        assert!((grey[0] - grey[1]).abs() < 1e-5 && (grey[1] - grey[2]).abs() < 1e-5);
        assert!((luma(grey) - luma(rgb)).abs() < 1e-5);
    }

    #[test]
    fn real_colour_edges_keep_their_colour() {
        let p = NoiseParams { amount: 100.0 };
        let orange = [0.6, 0.3, 0.05];
        // A smoothed target far from the pixel's colour (a neighbouring patch).
        let (tr, tb) = ratios([0.2, 0.35, 0.3]);
        let step = ColourStep::new(&p);
        let kept = denoise_colour(orange, tr, tb, &step);
        assert!(
            kept.iter().zip(orange).all(|(a, b)| (a - b).abs() < 1e-5),
            "{kept:?}"
        );
        // A nearby (noise-sized) target is taken.
        let (nr, nb) = ratios(orange);
        let out = denoise_colour(orange, nr + 0.05, nb, &step);
        assert!((ratios(out).0 - (nr + 0.05)).abs() < 1e-4);
    }

    #[test]
    fn colour_blotches_are_averaged_without_a_luminance_edge() {
        let (w, h) = (80, 40);
        let p = NoiseParams { amount: 100.0 };
        let guide = vec![-3.0f32; w * h];
        let rr: Vec<f32> = (0..w * h).map(|k| 1.0 + 0.2 * noise(k)).collect();
        let mut out = Vec::new();
        let eps = p.chroma_eps();
        guided(
            &guide,
            Some(&rr),
            w,
            h,
            3,
            |_| eps,
            &mut GuidedScratch::default(),
            &mut out,
        );
        assert!(
            spread(&out) < 0.3 * spread(&rr),
            "{} vs {}",
            spread(&out),
            spread(&rr)
        );
    }

    #[test]
    fn radii_scale_with_the_image() {
        assert_eq!(luma_radius(1516, 1010), 1);
        assert_eq!(luma_radius(6064, 4040), 5);
    }
}
