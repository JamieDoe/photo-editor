//! Retouching (ADR 0054): heal and clone spots.
//!
//! A spot replaces a disc of the photo with a disc from elsewhere in it (its source).
//! Clone copies the source exactly. Heal copies the source's texture but takes its
//! colour and brightness from the spot's surroundings: the difference between the
//! surroundings and the source's around the edge is spread smoothly over the disc (a
//! harmonic membrane, the smoothest surface with those edge values) and added. Either
//! is blended in with a feathered edge.
//!
//! Spots are in the source photo's own coordinates (fractions of its width and height;
//! the radius a fraction of its long edge), so they stay on what they cover whatever
//! the crop, straighten or turn, and apply alike at any preview size. They are applied
//! to the source, in order, before anything else; the renderer caches the result.

use image_core::LinearImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SpotKind {
    /// The source's texture, in the surroundings' colour and brightness.
    #[default]
    Heal,
    /// The source exactly.
    Clone,
}

/// One spot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Spot {
    pub kind: SpotKind,
    /// The spot's centre: fractions of the photo's width and height.
    pub x: f32,
    pub y: f32,
    /// The source's centre, likewise.
    pub source_x: f32,
    pub source_y: f32,
    /// A fraction of the photo's long edge.
    pub radius: f32,
    /// How much of the radius fades out at the edge, 0..100.
    pub feather: f32,
    /// 0..100.
    pub opacity: f32,
}

impl Default for Spot {
    fn default() -> Self {
        Self {
            kind: SpotKind::Heal,
            x: 0.5,
            y: 0.5,
            source_x: 0.5,
            source_y: 0.5,
            radius: 0.02,
            feather: DEFAULT_FEATHER,
            opacity: 100.0,
        }
    }
}

pub const DEFAULT_FEATHER: f32 = 30.0;
const MIN_RADIUS: f32 = 0.001;
const MAX_RADIUS: f32 = 0.25;

impl Spot {
    pub fn sanitized(self) -> Self {
        let f = |v: f32, lo: f32, hi: f32, or: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { or }
        };
        Self {
            kind: self.kind,
            x: f(self.x, 0.0, 1.0, 0.5),
            y: f(self.y, 0.0, 1.0, 0.5),
            source_x: f(self.source_x, 0.0, 1.0, 0.5),
            source_y: f(self.source_y, 0.0, 1.0, 0.5),
            radius: f(self.radius, MIN_RADIUS, MAX_RADIUS, 0.02),
            feather: f(self.feather, 0.0, 100.0, DEFAULT_FEATHER),
            opacity: f(self.opacity, 0.0, 100.0, 100.0),
        }
    }

    /// Changes nothing: invisible, or copying from where it is.
    pub fn is_noop(&self) -> bool {
        self.opacity == 0.0 || (self.x == self.source_x && self.y == self.source_y)
    }
}

/// A spot in an image's pixels.
struct Placed {
    centre: [f32; 2],
    /// Source minus centre.
    offset: [f32; 2],
    radius: f32,
}

impl Placed {
    fn new(s: &Spot, w: usize, h: usize) -> Self {
        let (w, h) = (w as f32, h as f32);
        Self {
            centre: [s.x * w, s.y * h],
            offset: [(s.source_x - s.x) * w, (s.source_y - s.y) * h],
            // At least a pixel, so small previews still show it.
            radius: (s.radius * w.max(h)).max(1.0),
        }
    }
}

/// `image` with `spots` applied in order (each sees the ones before it).
pub fn retouch(image: &LinearImage, spots: &[Spot]) -> LinearImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut data = image.data().to_vec();
    for spot in spots.iter().map(|s| s.sanitized()).filter(|s| !s.is_noop()) {
        apply_spot(&mut data, w, h, &spot);
    }
    LinearImage::new(image.width(), image.height(), data).expect("same size as the source")
}

fn apply_spot(data: &mut [u16], w: usize, h: usize, spot: &Spot) {
    let p = Placed::new(spot, w, h);
    let [cx, cy] = p.centre;
    let [dx, dy] = p.offset;
    let r = p.radius;
    let x0 = (cx - r).floor().max(0.0) as usize;
    let y0 = (cy - r).floor().max(0.0) as usize;
    let x1 = ((cx + r).ceil() as usize).min(w);
    let y1 = ((cy + r).ceil() as usize).min(h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let image = Pixels { data, w, h };
    let membrane = (spot.kind == SpotKind::Heal).then(|| Membrane::new(&image, &p));
    let inner = 1.0 - spot.feather / 100.0;
    let opacity = spot.opacity / 100.0;
    // Worked out from the image as it was, then written: the source may overlap.
    let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() / r;
            let alpha = opacity * coverage(d, inner);
            if alpha <= 0.0 {
                out.push(None);
                continue;
            }
            let mut v = image.sample(px + dx, py + dy);
            if let Some(m) = &membrane {
                let shift = m.at(px - cx, py - cy);
                for (c, s) in v.iter_mut().zip(shift) {
                    *c += s;
                }
            }
            let here = image.pixel(x, y);
            out.push(Some([0, 1, 2].map(|c| here[c] + (v[c] - here[c]) * alpha)));
        }
    }
    let mut out = out.into_iter();
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(Some(v)) = out.next() {
                let i = (y * w + x) * 3;
                for (c, value) in v.into_iter().enumerate() {
                    data[i + c] = (value * 65535.0).round().clamp(0.0, 65535.0) as u16;
                }
            }
        }
    }
}

/// 1 inside `inner`, fading smoothly to 0 at the edge (`d` = 1).
fn coverage(d: f32, inner: f32) -> f32 {
    if d >= 1.0 {
        0.0
    } else if d <= inner {
        1.0
    } else {
        let t = (d - inner) / (1.0 - inner);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

/// An image's pixels as 0..1, read with clamping at the edges.
struct Pixels<'a> {
    data: &'a [u16],
    w: usize,
    h: usize,
}

impl Pixels<'_> {
    fn pixel(&self, x: usize, y: usize) -> [f32; 3] {
        let i = (y * self.w + x) * 3;
        [0, 1, 2].map(|c| f32::from(self.data[i + c]) / 65535.0)
    }

    /// Bilinear, at pixel-centre coordinates (`x + 0.5` is pixel `x`'s centre).
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let fx = (x - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = (y - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (ix, iy) = (fx as usize, fy as usize);
        let (ix1, iy1) = ((ix + 1).min(self.w - 1), (iy + 1).min(self.h - 1));
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let [a, b, c, d] = [
            self.pixel(ix, iy),
            self.pixel(ix1, iy),
            self.pixel(ix, iy1),
            self.pixel(ix1, iy1),
        ];
        [0, 1, 2].map(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    }
}

/// Heal's correction: the surroundings less the source around the edge, spread over
/// the disc. Evaluated on a coarse grid (it is smooth) and interpolated.
struct Membrane {
    /// Grid points per side, the grid's step and its first point (relative to the
    /// centre), and the correction at each point.
    n: usize,
    step: f32,
    start: f32,
    values: Vec<[f32; 3]>,
}

impl Membrane {
    fn new(image: &Pixels<'_>, p: &Placed) -> Self {
        let r = p.radius;
        let [cx, cy] = p.centre;
        let [dx, dy] = p.offset;
        // Edge samples about two pixels apart, each the difference averaged over a
        // short radial run just outside the edge, then smoothed around the ring, so
        // grain at the edge does not show as streaks.
        let count = ((std::f32::consts::TAU * r / 2.0).ceil() as usize).clamp(12, 128);
        let ring: Vec<([f32; 2], [f32; 3])> = (0..count)
            .map(|k| {
                let (sin, cos) = (k as f32 / count as f32 * std::f32::consts::TAU).sin_cos();
                let mut diff = [0.0f32; 3];
                for out in [0.0, 1.0, 2.0] {
                    let (x, y) = (cx + cos * (r + out), cy + sin * (r + out));
                    let here = image.sample(x, y);
                    let there = image.sample(x + dx, y + dy);
                    for c in 0..3 {
                        diff[c] += (here[c] - there[c]) / 3.0;
                    }
                }
                ([cos * r, sin * r], diff)
            })
            .collect();
        let width = (count / 16).max(1);
        let ring: Vec<([f32; 2], [f32; 3])> = (0..count)
            .map(|k| {
                let mut sum = [0.0f32; 3];
                for j in 0..=2 * width {
                    let d = ring[(k + count + j - width) % count].1;
                    for c in 0..3 {
                        sum[c] += d[c] / (2 * width + 1) as f32;
                    }
                }
                (ring[k].0, sum)
            })
            .collect();
        let step = (r / 24.0).max(1.0);
        let n = (2.0 * r / step).ceil() as usize + 2;
        let start = -(n as f32 - 1.0) * step / 2.0;
        let values = (0..n * n)
            .map(|i| {
                let (gx, gy) = (start + (i % n) as f32 * step, start + (i / n) as f32 * step);
                harmonic(&ring, gx, gy)
            })
            .collect();
        Self {
            n,
            step,
            start,
            values,
        }
    }

    /// The correction at (`x`, `y`) from the centre.
    fn at(&self, x: f32, y: f32) -> [f32; 3] {
        let last = (self.n - 1) as f32;
        let fx = ((x - self.start) / self.step).clamp(0.0, last);
        let fy = ((y - self.start) / self.step).clamp(0.0, last);
        let (ix, iy) = ((fx as usize).min(self.n - 2), (fy as usize).min(self.n - 2));
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let v = |x: usize, y: usize| self.values[y * self.n + x];
        let [a, b, c, d] = [v(ix, iy), v(ix + 1, iy), v(ix, iy + 1), v(ix + 1, iy + 1)];
        [0, 1, 2].map(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    }
}

/// The harmonic interpolation of the ring's values at (`x`, `y`) inside it: the
/// Poisson integral for a disc, whose `(R^2 - |p|^2)` factor cancels when the weights
/// are normalised, leaving inverse squared distances to the edge samples. Outside the
/// ring (grid corners), the nearest edge's value.
fn harmonic(ring: &[([f32; 2], [f32; 3])], x: f32, y: f32) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    let mut total = 0.0f32;
    for ([bx, by], v) in ring {
        let w = 1.0 / ((x - bx).powi(2) + (y - by).powi(2)).max(1e-3);
        total += w;
        for c in 0..3 {
            sum[c] += w * v[c];
        }
    }
    sum.map(|s| s / total)
}

/// A source for a new spot at (`x`, `y`) with `radius` (as in [`Spot`]): a nearby
/// disc, wholly inside the photo and clear of `avoid`'s spots, whose surroundings
/// look most like the spot's, so a heal or clone blends in. `None` when no disc fits.
pub fn find_source(
    image: &LinearImage,
    x: f32,
    y: f32,
    radius: f32,
    avoid: &[Spot],
) -> Option<[f32; 2]> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let pixels = Pixels {
        data: image.data(),
        w,
        h,
    };
    let spot = Spot {
        x,
        y,
        source_x: x,
        source_y: y,
        radius,
        ..Default::default()
    }
    .sanitized();
    let p = Placed::new(&spot, w, h);
    let r = p.radius;
    let [cx, cy] = p.centre;
    // Compared on square roots, closer to how differences look than linear light.
    let look = |x: f32, y: f32| pixels.sample(x, y).map(f32::sqrt);
    // The spot's surroundings, and its brightness there.
    let ring: Vec<[f32; 2]> = [1.2f32, 1.5]
        .iter()
        .flat_map(|k| {
            (0..24).map(move |i| {
                let (sin, cos) = (i as f32 / 24.0 * std::f32::consts::TAU).sin_cos();
                [cos * r * k, sin * r * k]
            })
        })
        .collect();
    let around: Vec<[f32; 3]> = ring.iter().map(|[ox, oy]| look(cx + ox, cy + oy)).collect();
    let mean = |v: &[[f32; 3]]| {
        let n = v.len() as f32;
        v.iter().fold([0.0f32; 3], |a, p| {
            [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]
        })
    };
    let around_mean = mean(&around);
    let inside: Vec<[f32; 2]> = (0..12)
        .map(|i| {
            let (sin, cos) = (i as f32 / 12.0 * std::f32::consts::TAU).sin_cos();
            [cos * r * 0.6, sin * r * 0.6]
        })
        .chain(std::iter::once([0.0, 0.0]))
        .collect();
    let avoid: Vec<Placed> = avoid
        .iter()
        .map(|s| Placed::new(&s.sanitized(), w, h))
        .collect();
    let mut best: Option<([f32; 2], f32)> = None;
    for distance in [2.5f32, 3.5, 5.0] {
        for i in 0..24 {
            let (sin, cos) = (i as f32 / 24.0 * std::f32::consts::TAU).sin_cos();
            let (sx, sy) = (cx + cos * r * distance, cy + sin * r * distance);
            let margin = r * 1.6;
            if sx < margin || sy < margin || sx > w as f32 - margin || sy > h as f32 - margin {
                continue;
            }
            if avoid
                .iter()
                .any(|a| (a.centre[0] - sx).hypot(a.centre[1] - sy) < a.radius + r)
            {
                continue;
            }
            // Surroundings that match, and an inside that looks like the spot's
            // surroundings (no blemish of its own); nearer is a little better.
            let mut cost = 0.0;
            for ([ox, oy], a) in ring.iter().zip(&around) {
                let b = look(sx + ox, sy + oy);
                cost += (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>() / ring.len() as f32;
            }
            for [ox, oy] in &inside {
                let b = look(sx + ox, sy + oy);
                cost += (0..3).map(|c| (around_mean[c] - b[c]).powi(2)).sum::<f32>()
                    / inside.len() as f32;
            }
            cost *= 1.0 + 0.05 * distance;
            if best.is_none_or(|(_, c)| cost < c) {
                best = Some(([sx / w as f32, sy / h as f32], cost));
            }
        }
    }
    best.map(|(at, _)| at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w` x `h` image from a function of the pixel centre (fractions of the long
    /// edge) to linear RGB.
    fn image(w: u32, h: u32, f: impl Fn(f32, f32) -> [f32; 3]) -> LinearImage {
        let long = w.max(h) as f32;
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f((x as f32 + 0.5) / long, (y as f32 + 0.5) / long))
            .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
            .collect();
        LinearImage::new(w, h, data).unwrap()
    }

    fn at(img: &LinearImage, x: usize, y: usize) -> [f32; 3] {
        let i = (y * img.width() as usize + x) * 3;
        [0, 1, 2].map(|c| f32::from(img.data()[i + c]) / 65535.0)
    }

    /// A smooth gradient with a dark blemish of radius 0.03 at (0.3, 0.5).
    fn blemished() -> LinearImage {
        image(200, 200, |x, y| {
            let base = 0.2 + 0.4 * x + 0.1 * y;
            if (x - 0.3).hypot(y - 0.5) < 0.03 {
                [0.02, 0.02, 0.02]
            } else {
                [base, base * 0.9, base * 0.8]
            }
        })
    }

    fn spot(kind: SpotKind, source_x: f32) -> Spot {
        Spot {
            kind,
            x: 0.3,
            y: 0.5,
            source_x,
            source_y: 0.5,
            radius: 0.045,
            feather: 30.0,
            opacity: 100.0,
        }
    }

    #[test]
    fn no_spots_changes_nothing() {
        let img = blemished();
        assert!(retouch(&img, &[]) == img);
        // Copying from itself, or invisible, does nothing either.
        let still = Spot {
            opacity: 0.0,
            ..spot(SpotKind::Clone, 0.6)
        };
        assert!(retouch(&img, &[spot(SpotKind::Heal, 0.3), still]) == img);
    }

    #[test]
    fn heal_takes_the_surroundings_tone_and_removes_the_blemish() {
        let img = blemished();
        // From further right, where the gradient is brighter.
        let healed = retouch(&img, &[spot(SpotKind::Heal, 0.6)]);
        let centre = at(&healed, 60, 100);
        // What the gradient would be there without the blemish.
        let expected = 0.2 + 0.4 * 0.3025 + 0.1 * 0.5025;
        assert!(
            (centre[0] - expected).abs() < 0.01,
            "{centre:?} vs {expected}"
        );
        // Outside the spot, untouched.
        assert_eq!(at(&healed, 10, 10), at(&img, 10, 10));
    }

    #[test]
    fn clone_copies_the_source_exactly() {
        let img = blemished();
        let cloned = retouch(&img, &[spot(SpotKind::Clone, 0.6)]);
        // The centre is the source's centre, brighter than the gradient here.
        let (centre, source) = (at(&cloned, 60, 100), at(&img, 120, 100));
        for c in 0..3 {
            assert!((centre[c] - source[c]).abs() < 1.0 / 65535.0 * 2.0);
        }
    }

    #[test]
    fn heal_keeps_the_source_texture() {
        // Stripes everywhere: healing from elsewhere keeps stripes, not a smear.
        let img = image(200, 200, |x, _| {
            let v = if (x * 200.0) as u32 % 4 < 2 { 0.3 } else { 0.5 };
            [v; 3]
        });
        let healed = retouch(&img, &[spot(SpotKind::Heal, 0.62)]);
        let row: Vec<f32> = (56..64).map(|x| at(&healed, x, 100)[0]).collect();
        let spread = row.iter().cloned().fold(f32::MIN, f32::max)
            - row.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.15, "{row:?}");
    }

    #[test]
    fn the_edge_blends_in() {
        let img = blemished();
        let healed = retouch(&img, &[spot(SpotKind::Heal, 0.6)]);
        // Just inside the edge (radius 9 px), within a level or two of the original.
        let (x, y) = (60 + 8, 100);
        for c in 0..3 {
            assert!((at(&healed, x, y)[c] - at(&img, x, y)[c]).abs() < 0.01);
        }
    }

    #[test]
    fn spots_apply_alike_at_any_size() {
        // The same spot on a half-size image lands on the same place.
        let small = image(100, 100, |x, y| {
            if (x - 0.3).hypot(y - 0.5) < 0.03 {
                [0.02; 3]
            } else {
                [0.4; 3]
            }
        });
        let healed = retouch(&small, &[spot(SpotKind::Heal, 0.6)]);
        assert!(at(&healed, 30, 50)[0] > 0.35);
    }

    #[test]
    fn finds_a_clean_source_nearby() {
        let img = blemished();
        let [sx, sy] = find_source(&img, 0.3, 0.5, 0.045, &[]).unwrap();
        let d = (sx - 0.3).hypot(sy - 0.5);
        assert!(d > 0.1 && d < 0.25, "{sx} {sy}");
        // Its surroundings match: on the gradient, similar brightness to the spot's,
        // which varies mostly across x.
        assert!((sx - 0.3).abs() < 0.12, "{sx} {sy}");
        // Never on a spot already there.
        let taken = Spot {
            x: sx,
            y: sy,
            ..spot(SpotKind::Heal, 0.0)
        };
        let [ax, ay] = find_source(&img, 0.3, 0.5, 0.045, &[taken]).unwrap();
        assert!((ax - sx).hypot(ay - sy) > 0.09);
        // Too big to fit: none.
        assert_eq!(
            find_source(&image(20, 20, |_, _| [0.5; 3]), 0.5, 0.5, 0.25, &[]),
            None
        );
    }

    #[test]
    fn sanitising_keeps_it_in_range() {
        let s = Spot {
            x: 2.0,
            radius: f32::NAN,
            feather: -5.0,
            opacity: 300.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (s.x, s.radius, s.feather, s.opacity),
            (1.0, 0.02, 0.0, 100.0)
        );
    }
}
