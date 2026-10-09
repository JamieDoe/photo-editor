//! Red-eye removal (ADR 0080): the red a flash leaves in pupils, made dark and neutral.
//!
//! A correction is a circle over an eye, in the source photo's own coordinates (like
//! heal spots: fractions of its width and height, the radius a fraction of its long
//! edge). Inside it, a pixel counts as red pupil by how far red outweighs green and blue
//! (`redness`, 0 for grey, 1 for pure red). Skin is reddish too, but in linear light its
//! red is under twice its green, where a red pupil's is many times it, so a threshold
//! between them keeps the eyelids and the skin around. Pupil size moves that threshold.
//!
//! Where it counts, the red is replaced by the pupil's own green and blue (so its
//! texture stays), then darkened by Darken where the pixel itself is red; the weight is
//! softened over a few pixels and fades out towards the circle's edge, so nothing looks
//! cut out. A white catchlight isn't red, and keeps its brightness.

use image_core::LinearImage;
use serde::{Deserialize, Serialize};

/// One red-eye correction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RedEye {
    /// The eye's centre: fractions of the photo's width and height.
    pub x: f32,
    pub y: f32,
    /// The circle searched for red: a fraction of the photo's long edge.
    pub radius: f32,
    /// 0..100: how much of the red counts as pupil (more takes paler red).
    pub pupil: f32,
    /// 0..100: how dark the pupil becomes.
    pub darken: f32,
}

pub const DEFAULT_PUPIL: f32 = 50.0;
pub const DEFAULT_DARKEN: f32 = 50.0;
const MIN_RADIUS: f32 = 0.001;
const MAX_RADIUS: f32 = 0.1;

impl Default for RedEye {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            radius: 0.01,
            pupil: DEFAULT_PUPIL,
            darken: DEFAULT_DARKEN,
        }
    }
}

impl RedEye {
    pub fn sanitized(self) -> Self {
        let f =
            |v: f32, lo: f32, hi: f32, or: f32| if v.is_finite() { v.clamp(lo, hi) } else { or };
        Self {
            x: f(self.x, 0.0, 1.0, 0.5),
            y: f(self.y, 0.0, 1.0, 0.5),
            radius: f(self.radius, MIN_RADIUS, MAX_RADIUS, 0.01),
            pupil: f(self.pupil, 0.0, 100.0, DEFAULT_PUPIL),
            darken: f(self.darken, 0.0, 100.0, DEFAULT_DARKEN),
        }
    }

    /// The redness above which a pixel is pupil: 0.65 at Pupil size 0, 0.45 at 100.
    fn threshold(&self) -> f32 {
        0.65 - 0.2 * self.pupil / 100.0
    }
}

/// How far red outweighs green and blue: 0 for grey or anything not red, 1 for pure
/// red. Very dark pixels (noise) aren't red.
#[inline]
fn redness(px: [f32; 3]) -> f32 {
    let [r, g, b] = px;
    if r < 0.002 {
        return 0.0;
    }
    ((r - g.max(b)) / r).max(0.0)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `image` with `eyes` corrected.
pub fn apply(image: &LinearImage, eyes: &[RedEye]) -> LinearImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut data = image.data().to_vec();
    for eye in eyes.iter().map(|e| e.sanitized()) {
        correct(&mut data, w, h, &eye);
    }
    LinearImage::new(w as u32, h as u32, data).expect("same size")
}

/// One eye corrected in `data` (`w` × `h`, three values a pixel).
fn correct(data: &mut [u16], w: usize, h: usize, eye: &RedEye) {
    let long = w.max(h) as f32;
    let (cx, cy, r) = (
        eye.x * w as f32,
        eye.y * h as f32,
        (eye.radius * long).max(1.5),
    );
    let (x0, x1) = (
        ((cx - r).floor().max(0.0)) as usize,
        ((cx + r).ceil() as usize).min(w),
    );
    let (y0, y1) = (
        ((cy - r).floor().max(0.0)) as usize,
        ((cy + r).ceil() as usize).min(h),
    );
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    let px = |data: &[u16], x: usize, y: usize| {
        let i = (y * w + x) * 3;
        [data[i], data[i + 1], data[i + 2]].map(|v| f32::from(v) / 65535.0)
    };
    // How much each pixel of the circle's box is pupil.
    let t = eye.threshold();
    let mut weight = vec![0.0f32; bw * bh];
    for y in 0..bh {
        for x in 0..bw {
            let d = ((x0 + x) as f32 + 0.5 - cx).hypot((y0 + y) as f32 + 0.5 - cy) / r;
            let edge = 1.0 - smoothstep(0.75, 1.0, d);
            if edge > 0.0 {
                weight[y * bw + x] =
                    edge * smoothstep(t - 0.12, t + 0.05, redness(px(data, x0 + x, y0 + y)));
            }
        }
    }
    // Softened over a pixel each way, so the pupil's edge isn't cut out.
    let soft: Vec<f32> = (0..bw * bh)
        .map(|k| {
            let (x, y) = (k % bw, k / bw);
            let (mut sum, mut n) = (0.0, 0.0);
            for yy in y.saturating_sub(1)..(y + 2).min(bh) {
                for xx in x.saturating_sub(1)..(x + 2).min(bw) {
                    sum += weight[yy * bw + xx];
                    n += 1.0;
                }
            }
            (weight[k] + sum / n) * 0.5
        })
        .collect();
    let dark = 1.0 - 0.75 * eye.darken / 100.0;
    for y in 0..bh {
        for x in 0..bw {
            let a = soft[y * bw + x];
            if a <= 0.0 {
                continue;
            }
            let [r0, g, b] = px(data, x0 + x, y0 + y);
            // The red replaced by the pupil's own green and blue (softened, as that
            // changes nothing that isn't red), then darkened where the pixel itself is
            // red: a catchlight beside the pupil keeps its brightness.
            let own = weight[y * bw + x];
            let shade = 1.0 - (1.0 - dark) * own / a.max(1e-6);
            let fixed = [(g + b) / 2.0, g, b].map(|v| v * shade.clamp(dark, 1.0));
            let original = [r0, g, b];
            let i = ((y0 + y) * w + x0 + x) * 3;
            for k in 0..3 {
                let v = original[k] + (fixed[k] - original[k]) * a;
                data[i + k] = (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
            }
        }
    }
}

/// The red eye nearest `at` (fractions of the photo) within `search` (a fraction of
/// its long edge), as a correction sized to it; `None` when there is no red pupil
/// there.
pub fn find(image: &LinearImage, at: [f32; 2], search: f32) -> Option<RedEye> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let long = w.max(h) as f32;
    let (cx, cy, s) = (at[0] * w as f32, at[1] * h as f32, (search * long).max(3.0));
    let (x0, x1) = (
        ((cx - s).floor().max(0.0)) as usize,
        ((cx + s).ceil() as usize).min(w),
    );
    let (y0, y1) = (
        ((cy - s).floor().max(0.0)) as usize,
        ((cy + s).ceil() as usize).min(h),
    );
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    let data = image.data();
    let red: Vec<bool> = (0..bw * bh)
        .map(|k| {
            let (x, y) = (x0 + k % bw, y0 + k / bw);
            let inside = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= s;
            let i = (y * w + x) * 3;
            inside
                && redness([data[i], data[i + 1], data[i + 2]].map(|v| f32::from(v) / 65535.0))
                    >= 0.55
        })
        .collect();
    // The red region whose pixel is nearest the click.
    let start = (0..bw * bh).filter(|&k| red[k]).min_by(|&a, &b| {
        let d = |k: usize| ((x0 + k % bw) as f32 + 0.5 - cx).hypot((y0 + k / bw) as f32 + 0.5 - cy);
        d(a).total_cmp(&d(b))
    })?;
    let mut region = vec![start];
    let mut seen = vec![false; bw * bh];
    seen[start] = true;
    let mut i = 0;
    while i < region.len() {
        let k = region[i];
        i += 1;
        let (x, y) = (k % bw, k / bw);
        let around = [
            (x > 0).then(|| k - 1),
            (x + 1 < bw).then_some(k + 1),
            (y > 0).then(|| k - bw),
            (y + 1 < bh).then_some(k + bw),
        ];
        for n in around.into_iter().flatten() {
            if red[n] && !seen[n] {
                seen[n] = true;
                region.push(n);
            }
        }
    }
    if region.len() < 4 {
        return None;
    }
    let n = region.len() as f32;
    let (sx, sy) = region.iter().fold((0.0, 0.0), |(sx, sy), &k| {
        (
            sx + (x0 + k % bw) as f32 + 0.5,
            sy + (y0 + k / bw) as f32 + 0.5,
        )
    });
    // A circle a little larger than the pupil, so its soft edge clears the rim.
    let pupil = (n / std::f32::consts::PI).sqrt();
    Some(
        RedEye {
            x: sx / n / w as f32,
            y: sy / n / h as f32,
            radius: (pupil * 1.5 + 1.0) / long,
            ..RedEye::default()
        }
        .sanitized(),
    )
}

#[cfg(test)]
mod tests;
