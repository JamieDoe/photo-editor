//! Texture and Clarity (ADR 0026): local contrast at two scales.
//!
//! Both work on log2 luminance of the image entering the stage's neighbourhood (the
//! source after white balance and exposure), and change a pixel by a gain in stops,
//! the same on R, G and B, so colours keep their hue:
//!
//! - **Texture** is fine detail: the pixel against a small blur whose radius is a fixed
//!   fraction of the image (0.15 % of the long edge, so the preview and the export
//!   see the same detail relative to the picture).
//! - **Clarity** is medium-scale structure: the small blur against the edge-aware
//!   surroundings map shared with Highlights and Shadows (ADR 0023). The map follows
//!   strong edges, so clarity has no halos along them; flat regions get their
//!   texture's contrast raised or lowered.
//!
//! - **Sharpening** (ADR 0027) is capture sharpening: an unsharp mask of about one
//!   pixel *at the rendered size* (sharpness is a property of output pixels, so unlike
//!   Texture it does not scale with the image). Its change is limited to ±0.5 stop,
//!   which keeps halos along edges faint.
//!
//! Deep shadows fade out of Texture and Sharpening (mostly noise there) and Clarity
//! eases off towards white and black, where extra contrast would clip.

use image_core::color::REC709_LUMA;

use super::tone::ToneBase;

/// Texture at ±100 doubles (or removes) fine detail.
const TEXTURE_STRENGTH: f32 = 1.0;
/// Clarity at ±100 doubles (or removes) medium-scale structure.
const CLARITY_STRENGTH: f32 = 2.0;
/// Largest change either makes, in stops.
pub const MAX_STOPS: f32 = 1.5;
/// Small blur radius as a fraction of the image's long edge.
const RADIUS_FRACTION: f32 = 0.0015;
const FLOOR: f32 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetailParams {
    /// -100..100.
    pub texture: f32,
    /// -100..100.
    pub clarity: f32,
    /// 0..150.
    pub sharpening: f32,
}

/// Rows the sharpening blur reads either side (a 3x3 binomial, see
/// [`sharpen_blur_row`]).
pub const SHARPEN_REACH: usize = 1;
/// Sharpening at 100 multiplies one-pixel detail by 2.5 (the 3x3 blur captures only
/// the finest detail, so this matches a wider 1-px unsharp mask at 100 %).
const SHARPEN_STRENGTH: f32 = 1.5;
const SHARPEN_MAX_STOPS: f32 = 0.5;

impl DetailParams {
    pub fn is_identity(&self) -> bool {
        self.texture == 0.0 && self.clarity == 0.0 && self.sharpening == 0.0
    }

    /// Whether Texture or Clarity (which use the small blur) are set.
    pub fn uses_small_blur(&self) -> bool {
        self.texture != 0.0 || self.clarity != 0.0
    }

    /// Rows either side of a pixel the stage reads, for an image of this size.
    pub fn reach(&self, width: usize, height: usize) -> usize {
        let small = if self.uses_small_blur() {
            2 * radius_for(width, height)
        } else {
            0
        };
        let sharp = if self.sharpening != 0.0 {
            SHARPEN_REACH
        } else {
            0
        };
        small.max(sharp)
    }

    /// Clarity needs the surroundings map.
    pub fn needs_base(&self) -> bool {
        self.clarity != 0.0
    }
}

/// Box radius of the small blur for an image of `width` x `height`. Two box passes
/// are applied, so it reaches `2 * radius` pixels.
pub fn radius_for(width: usize, height: usize) -> usize {
    ((width.max(height) as f32 * RADIUS_FRACTION).round() as usize).clamp(1, 24)
}

/// log2 of linear luminance, accurate to ~1e-6 stops without a libm call (the same
/// function is used by the reference and the fast path, so they agree exactly).
#[inline]
pub fn fast_log2(x: f32) -> f32 {
    let bits = x.max(FLOOR).to_bits();
    let mut exponent = ((bits >> 23) & 0xff) as i32 - 127;
    let mut m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000); // 1..2
    if m > std::f32::consts::SQRT_2 {
        m *= 0.5;
        exponent += 1;
    }
    // log2(m) = 2/ln2 * atanh(s), s = (m-1)/(m+1), |s| <= 0.172: four odd terms.
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let p = s * (2.885_39 + s2 * (0.961_796_7 + s2 * (0.577_078 + s2 * 0.412_198_6)));
    exponent as f32 + p
}

/// Log2 luminance of one source row (u16 samples) after per-channel `gains`.
pub fn log_luminance_row(src: &[u16], gains: [f32; 3], out: &mut [f32]) {
    let s = 1.0 / 65535.0;
    let (gr, gg, gb) = (
        gains[0] * REC709_LUMA[0] * s,
        gains[1] * REC709_LUMA[1] * s,
        gains[2] * REC709_LUMA[2] * s,
    );
    for (px, o) in src.as_chunks::<3>().0.iter().zip(out.iter_mut()) {
        *o = fast_log2(f32::from(px[0]) * gr + f32::from(px[1]) * gg + f32::from(px[2]) * gb);
    }
}

/// Two box passes of `radius` in each direction over a `w` x `h` plane, in place.
/// Borders average only the pixels inside, so a plane that is a band of a larger
/// image gives the same result as the whole image for rows at least `2 * radius`
/// from the band's cut edges.
pub fn blur_plane(plane: &mut [f32], w: usize, h: usize, radius: usize, scratch: &mut BlurScratch) {
    scratch.line.resize(w, 0.0);
    scratch.sums.resize(w, 0.0);
    scratch.plane.resize(w * h, 0.0);
    for _ in 0..2 {
        for row in plane.chunks_mut(w) {
            box_line(row, radius, &mut scratch.line);
        }
    }
    for _ in 0..2 {
        vertical_pass(plane, &mut scratch.plane, w, h, radius, &mut scratch.sums);
        plane.copy_from_slice(&scratch.plane);
    }
}

/// Buffers [`blur_plane`] reuses between calls.
#[derive(Debug, Default)]
pub struct BlurScratch {
    line: Vec<f32>,
    sums: Vec<f32>,
    plane: Vec<f32>,
}

fn box_line(line: &mut [f32], r: usize, scratch: &mut [f32]) {
    let n = line.len();
    let src = &mut scratch[..n];
    src.copy_from_slice(line);
    let mut sum = 0.0f32;
    let mut count = 0.0f32;
    for v in &src[..=r.min(n - 1)] {
        sum += v;
        count += 1.0;
    }
    for k in 0..n {
        line[k] = sum / count;
        if k + r + 1 < n {
            sum += src[k + r + 1];
            count += 1.0;
        }
        if k >= r {
            sum -= src[k - r];
            count -= 1.0;
        }
    }
}

fn vertical_pass(src: &[f32], out: &mut [f32], w: usize, h: usize, r: usize, sums: &mut [f32]) {
    sums.iter_mut().for_each(|s| *s = 0.0);
    let mut count = 0.0f32;
    for y in 0..=r.min(h - 1) {
        for (s, v) in sums.iter_mut().zip(&src[y * w..(y + 1) * w]) {
            *s += v;
        }
        count += 1.0;
    }
    for y in 0..h {
        let inv = 1.0 / count;
        for (o, s) in out[y * w..(y + 1) * w].iter_mut().zip(sums.iter()) {
            *o = s * inv;
        }
        if y + r + 1 < h {
            let add = &src[(y + r + 1) * w..(y + r + 2) * w];
            for (s, v) in sums.iter_mut().zip(add) {
                *s += v;
            }
            count += 1.0;
        }
        if y >= r {
            let sub = &src[(y - r) * w..(y - r + 1) * w];
            for (s, v) in sums.iter_mut().zip(sub) {
                *s -= v;
            }
            count -= 1.0;
        }
    }
}

/// The sharpening blur of one row: a 3x3 binomial ([1 2 1] x [1 2 1] / 16, about a
/// 0.7-px Gaussian) from the row and its neighbours (pass the row itself at the
/// image's top or bottom). `vertical` is scratch.
pub fn sharpen_blur_row(
    above: &[f32],
    row: &[f32],
    below: &[f32],
    vertical: &mut Vec<f32>,
    out: &mut [f32],
) {
    let n = row.len();
    vertical.clear();
    vertical.extend(
        above
            .iter()
            .zip(row)
            .zip(below)
            .map(|((a, r), b)| a + 2.0 * r + b),
    );
    if n == 1 {
        out[0] = vertical[0] * 0.25;
        return;
    }
    out[0] = (3.0 * vertical[0] + vertical[1]) * 0.0625;
    for x in 1..n - 1 {
        out[x] = (vertical[x - 1] + 2.0 * vertical[x] + vertical[x + 1]) * 0.0625;
    }
    out[n - 1] = (vertical[n - 2] + 3.0 * vertical[n - 1]) * 0.0625;
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The change in stops at a pixel whose log2 luminance is `log_y`, given the small
/// blur `small`, the edge-aware surroundings `base` (only used by Clarity) and the
/// sharpening blur `sharp` (only used by Sharpening), all in log2.
#[inline]
pub fn gain_stops(p: &DetailParams, log_y: f32, small: f32, base: f32, sharp: f32) -> f32 {
    let d = -log_y; // stops below white
    let shadow_fade = 1.0 - smoothstep(6.0, 10.0, d);
    let texture = p.texture / 100.0 * TEXTURE_STRENGTH * shadow_fade;
    let clarity = p.clarity / 100.0
        * CLARITY_STRENGTH
        * smoothstep(0.0, 1.5, d)
        * (1.0 - smoothstep(7.0, 11.0, d));
    (texture * (log_y - small) + clarity * (small - base) + sharpen_stops(p, log_y, sharp))
        .clamp(-MAX_STOPS, MAX_STOPS)
}

/// The sharpening term alone: [`gain_stops`] when Texture and Clarity are zero (the
/// default recipe), without their work.
#[inline]
pub fn sharpen_stops(p: &DetailParams, log_y: f32, sharp: f32) -> f32 {
    let shadow_fade = 1.0 - smoothstep(6.0, 10.0, -log_y);
    (p.sharpening / 100.0 * SHARPEN_STRENGTH * shadow_fade * (log_y - sharp))
        .clamp(-SHARPEN_MAX_STOPS, SHARPEN_MAX_STOPS)
}

/// Reference implementation over a whole image of linear RGB `f32` (already after the
/// stages before this one), given the log2 luminance plane the neighbourhoods are
/// measured on (`log_y`, `width` x `height`) and the surroundings map.
pub fn apply_reference(
    rgb: &mut [f32],
    log_y: &[f32],
    width: usize,
    height: usize,
    base: Option<&ToneBase>,
    p: &DetailParams,
) {
    let mut small = log_y.to_vec();
    blur_plane(
        &mut small,
        width,
        height,
        radius_for(width, height),
        &mut BlurScratch::default(),
    );
    let mut sharp = vec![0.0f32; width * height];
    let mut vertical = Vec::new();
    for y in 0..height {
        let row = |r: usize| &log_y[r * width..(r + 1) * width];
        let (above, below) = (row(y.saturating_sub(1)), row((y + 1).min(height - 1)));
        let out = &mut sharp[y * width..(y + 1) * width];
        sharpen_blur_row(above, row(y), below, &mut vertical, out);
    }
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let base_log = match base {
                // The guided filter's model with the blurred value as the guide:
                // following edges, it leaves them out of the clarity band.
                Some(b) => -b.stops_at(x, y, width, height, small[i]),
                None => small[i],
            };
            let gain = gain_stops(p, log_y[i], small[i], base_log, sharp[i]).exp2();
            for c in &mut rgb[i * 3..i * 3 + 3] {
                *c *= gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane_of(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Vec<f32> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect()
    }

    /// Renders a grey image whose log2 luminance is `plane` with `p`; returns the
    /// output's log2 luminance.
    fn run(plane: &[f32], w: usize, h: usize, p: DetailParams) -> Vec<f32> {
        let mut rgb: Vec<f32> = plane.iter().flat_map(|l| [l.exp2(); 3]).collect();
        let base = p
            .needs_base()
            .then(|| ToneBase::from_log_luminance(w, h, plane));
        apply_reference(&mut rgb, plane, w, h, base.as_ref(), &p);
        rgb.chunks(3).map(|px| px[1].log2()).collect()
    }

    fn spread(v: &[f32]) -> f32 {
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn fast_log2_is_accurate() {
        for i in 1..2000 {
            let x = i as f32 / 1000.0;
            assert!((fast_log2(x) - x.log2()).abs() < 2e-4, "{x}");
        }
        assert!((fast_log2(1.0e-3) - 1.0e-3f32.log2()).abs() < 2e-4);
    }

    #[test]
    fn zero_and_flat_images_are_unchanged() {
        let (w, h) = (64, 48);
        let flat = plane_of(w, h, |_, _| -2.5);
        let p = DetailParams {
            texture: 100.0,
            clarity: 100.0,
            sharpening: 0.0,
        };
        for (a, b) in run(&flat, w, h, p).iter().zip(&flat) {
            assert!((a - b).abs() < 1e-4);
        }
        let noisy = plane_of(w, h, |x, y| -3.0 + ((x * 7 + y * 13) % 5) as f32 * 0.1);
        let zero = DetailParams {
            texture: 0.0,
            clarity: 0.0,
            sharpening: 0.0,
        };
        assert_eq!(run(&noisy, w, h, zero), noisy);
    }

    #[test]
    fn texture_raises_and_lowers_fine_detail() {
        let (w, h) = (400, 300); // radius 1: a 1-px checker is pure texture
        let fine = plane_of(w, h, |x, y| {
            -3.0 + if (x + y) % 2 == 0 { 0.2 } else { -0.2 }
        });
        let more = run(
            &fine,
            w,
            h,
            DetailParams {
                texture: 100.0,
                clarity: 0.0,
                sharpening: 0.0,
            },
        );
        let less = run(
            &fine,
            w,
            h,
            DetailParams {
                texture: -100.0,
                clarity: 0.0,
                sharpening: 0.0,
            },
        );
        assert!(spread(&more) > 1.6 * spread(&fine), "{}", spread(&more));
        assert!(spread(&less) < 0.4 * spread(&fine), "{}", spread(&less));
    }

    #[test]
    fn deep_shadows_keep_their_noise_level() {
        let (w, h) = (400, 300);
        let noise = plane_of(w, h, |x, y| {
            -12.0 + if (x + y) % 2 == 0 { 0.2 } else { -0.2 }
        });
        let out = run(
            &noise,
            w,
            h,
            DetailParams {
                texture: 100.0,
                clarity: 0.0,
                sharpening: 0.0,
            },
        );
        assert!((spread(&out) / spread(&noise) - 1.0).abs() < 0.05);
    }

    #[test]
    fn clarity_raises_medium_structure_without_halos_at_strong_edges() {
        let (w, h) = (256, 160);
        // Gentle medium-scale waves on a mid-grey field.
        let waves = plane_of(w, h, |x, _| -3.0 + 0.15 * (x as f32 / 12.0).sin());
        let out = run(
            &waves,
            w,
            h,
            DetailParams {
                texture: 0.0,
                clarity: 100.0,
                sharpening: 0.0,
            },
        );
        let centre = |v: &[f32]| spread(&v[40 * w..120 * w]);
        assert!(
            centre(&out) > 1.3 * centre(&waves),
            "{} vs {}",
            centre(&out),
            centre(&waves)
        );

        // A hard 5-stop step: pixels either side keep their level (no glow or dark
        // band along the edge).
        let step = plane_of(w, h, |x, _| if x < w / 2 { -6.0 } else { -1.0 });
        let out = run(
            &step,
            w,
            h,
            DetailParams {
                texture: 0.0,
                clarity: 100.0,
                sharpening: 0.0,
            },
        );
        for x in 0..w {
            let (i, want) = (80 * w + x, step[80 * w + x]);
            assert!((out[i] - want).abs() < 0.25, "x {x}: {} vs {want}", out[i]);
        }
    }

    #[test]
    fn a_band_of_rows_blurs_like_the_whole_image() {
        let (w, h) = (50, 60);
        let r = 3;
        let full_src = plane_of(w, h, |x, y| ((x * 31 + y * 17) % 23) as f32 * 0.1);
        let mut full = full_src.clone();
        blur_plane(&mut full, w, h, r, &mut BlurScratch::default());
        // Rows 20..30 from a band with a 2r apron either side.
        let (top, bottom) = (20 - 2 * r, 30 + 2 * r);
        let mut band = full_src[top * w..bottom * w].to_vec();
        blur_plane(&mut band, w, bottom - top, r, &mut BlurScratch::default());
        for y in 20..30 {
            for x in 0..w {
                let (a, b) = (band[(y - top) * w + x], full[y * w + x]);
                assert!((a - b).abs() < 1e-4, "({x}, {y}): {a} vs {b}");
            }
        }
    }

    #[test]
    fn sharpening_crisps_one_pixel_detail_with_limited_halos() {
        let (w, h) = (200, 120);
        let fine = plane_of(w, h, |x, y| {
            -3.0 + if (x + y) % 2 == 0 { 0.1 } else { -0.1 }
        });
        let p = |s| DetailParams {
            texture: 0.0,
            clarity: 0.0,
            sharpening: s,
        };
        let sharper = run(&fine, w, h, p(100.0));
        assert!(
            spread(&sharper) > 1.5 * spread(&fine),
            "{}",
            spread(&sharper)
        );
        // A hard 4-stop edge: the overshoot either side stays within half a stop.
        let step = plane_of(w, h, |x, _| if x < w / 2 { -5.0 } else { -1.0 });
        let out = run(&step, w, h, p(150.0));
        for x in 0..w {
            let (got, want) = (out[60 * w + x], step[60 * w + x]);
            assert!((got - want).abs() <= 0.5 + 1e-4, "x {x}: {got} vs {want}");
        }
        // Far from the edge nothing changes.
        assert!((out[60 * w + 10] - step[60 * w + 10]).abs() < 1e-4);
    }

    #[test]
    fn reach_covers_the_blurs_in_use() {
        let p = |t, s| DetailParams {
            texture: t,
            clarity: 0.0,
            sharpening: s,
        };
        assert_eq!(p(0.0, 40.0).reach(6064, 4040), 1);
        assert_eq!(p(10.0, 40.0).reach(6064, 4040), 18);
        assert_eq!(p(0.0, 0.0).reach(6064, 4040), 0);
    }

    #[test]
    fn radius_scales_with_the_image() {
        assert_eq!(radius_for(1516, 1010), 2);
        assert_eq!(radius_for(6064, 4040), 9);
        assert_eq!(radius_for(200, 100), 1);
    }
}
