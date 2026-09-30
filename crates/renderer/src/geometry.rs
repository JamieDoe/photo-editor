//! Crop and straighten (ADR 0032).
//!
//! Geometry is applied first: the source is resampled into the output frame (rotated
//! by the straighten angle about its centre, then cropped), and every other stage
//! runs on that frame. So the Vignette follows the crop, and the surroundings maps see
//! the picture as framed.
//!
//! Coordinates are fractions of the source's width and height, in the *straightened
//! view* (the source rotated about its centre), so they do not depend on render size.
//! A crop is always kept inside the rotated photo ([`Geometry::effective_crop`]), so
//! no empty corners can appear.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use image_core::LinearImage;

/// Largest straighten angle either way, in degrees (as in the design).
pub const MAX_STRAIGHTEN: f32 = 15.0;
/// Smallest crop side, as a fraction of the frame.
const MIN_SIDE: f32 = 0.02;

/// The crop's shape constraint, as the design offers them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum AspectRatio {
    /// The photo's own shape.
    #[default]
    Original,
    Free,
    Square,
    /// 4:5 portrait.
    Portrait4x5,
    /// 16:9 landscape.
    Wide16x9,
}

impl AspectRatio {
    /// Width / height in pixels for a source of `w` x `h`, or `None` (free).
    pub fn ratio(self, w: f32, h: f32) -> Option<f32> {
        match self {
            Self::Original => Some(w / h),
            Self::Free => None,
            Self::Square => Some(1.0),
            Self::Portrait4x5 => Some(0.8),
            Self::Wide16x9 => Some(16.0 / 9.0),
        }
    }
}

/// An axis-aligned rectangle in the straightened view, in fractions of the source's
/// width (`x`, `w`) and height (`y`, `h`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropRect {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    fn sanitized(self) -> Self {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        let w = finite(self.w, 1.0).clamp(MIN_SIDE, 1.0);
        let h = finite(self.h, 1.0).clamp(MIN_SIDE, 1.0);
        Self {
            x: finite(self.x, 0.0).clamp(0.0, 1.0 - w),
            y: finite(self.y, 0.0).clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }
}

impl Default for CropRect {
    fn default() -> Self {
        Self::FULL
    }
}

/// Crop and straighten settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Geometry {
    /// Degrees, -15..15; positive turns the picture anticlockwise.
    pub straighten: f32,
    pub crop: CropRect,
    /// The shape the crop keeps while it is edited.
    pub aspect: AspectRatio,
    /// Vertical perspective (ADR 0034), -100..100: positive widens the top, correcting
    /// buildings that lean back.
    pub vertical: f32,
    /// Horizontal perspective, -100..100: positive widens the right side, correcting a
    /// wall that recedes to the right.
    pub horizontal: f32,
}

impl Geometry {
    /// No rotation or perspective and the whole frame (whatever the aspect choice).
    pub fn is_identity(&self) -> bool {
        self.straighten == 0.0
            && self.vertical == 0.0
            && self.horizontal == 0.0
            && self.crop == CropRect::FULL
    }

    fn has_perspective(&self) -> bool {
        self.vertical != 0.0 || self.horizontal != 0.0
    }

    pub fn sanitized(&self) -> Self {
        let s = if self.straighten.is_finite() {
            self.straighten.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN)
        } else {
            0.0
        };
        let amount = |v: f32| {
            let v = if v.is_finite() {
                v.clamp(-100.0, 100.0)
            } else {
                0.0
            };
            if v == 0.0 { 0.0 } else { v }
        };
        Self {
            straighten: if s == 0.0 { 0.0 } else { s },
            crop: self.crop.sanitized(),
            aspect: self.aspect,
            vertical: amount(self.vertical),
            horizontal: amount(self.horizontal),
        }
    }

    /// The crop actually used for a source of `w` x `h`: the stored crop, pulled
    /// towards the centre and shrunk just enough to lie inside the rotated photo.
    pub fn effective_crop(&self, w: f32, h: f32) -> CropRect {
        let g = self.sanitized();
        let crop = g.crop;
        let map = Mapping::new(&g, w, h);
        if (g.straighten == 0.0 && !g.has_perspective()) || map.contains(&crop) {
            return crop;
        }
        // Scale about the frame's centre: at 0 it is a point there, which is inside.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if map.contains(&scaled_about_centre(&crop, mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        scaled_about_centre(&crop, lo)
    }

    /// Output size in pixels for a source of `w` x `h`.
    pub fn output_size(&self, w: u32, h: u32) -> (u32, u32) {
        let c = self.effective_crop(w as f32, h as f32);
        (
            ((c.w * w as f32).round() as u32).max(1),
            ((c.h * h as f32).round() as u32).max(1),
        )
    }
}

fn scaled_about_centre(c: &CropRect, s: f32) -> CropRect {
    let (cx, cy) = (c.x + c.w / 2.0, c.y + c.h / 2.0);
    let (nx, ny) = (0.5 + (cx - 0.5) * s, 0.5 + (cy - 0.5) * s);
    CropRect {
        x: nx - c.w * s / 2.0,
        y: ny - c.h * s / 2.0,
        w: c.w * s,
        h: c.h * s,
    }
}

/// Perspective at ±100: the virtual camera turns this many degrees.
const MAX_PERSPECTIVE_DEGREES: f32 = 20.0;

/// Where each point of the view (fractions of the source's width and height) comes
/// from in the source (pixels): undo the straighten rotation, then the perspective.
///
/// The perspective is a homography: the source seen by a camera turned about its
/// horizontal and vertical axes, with a focal length of the long edge (about a 50°
/// field of view), so the effect does not depend on render size. It is shifted so the
/// view's centre is the photo's centre.
#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    w: f32,
    h: f32,
    cos: f32,
    sin: f32,
    focal: f32,
    /// Camera rotation, row-major, and where it takes the centre; `None` without
    /// perspective.
    rotation: Option<([f32; 9], (f32, f32))>,
}

impl Mapping {
    pub fn new(g: &Geometry, w: f32, h: f32) -> Self {
        let (sin, cos) = g.straighten.to_radians().sin_cos();
        let rotation = g.has_perspective().then(|| {
            // Negative angles about both axes push the top and the right side away,
            // so they sample less of the photo and are widened.
            let a = (-g.vertical / 100.0 * MAX_PERSPECTIVE_DEGREES).to_radians();
            let b = (-g.horizontal / 100.0 * MAX_PERSPECTIVE_DEGREES).to_radians();
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            // Rx(a) * Ry(b)
            let r = [cb, 0.0, sb, sa * sb, ca, -sa * cb, -ca * sb, sa, ca * cb];
            (r, (r[2] / r[8], r[5] / r[8]))
        });
        Self {
            w,
            h,
            cos,
            sin,
            focal: w.max(h),
            rotation,
        }
    }

    /// Source pixel coordinates of view point (`vx`, `vy`).
    #[inline]
    pub fn source(&self, vx: f32, vy: f32) -> (f32, f32) {
        let (dx, dy) = ((vx - 0.5) * self.w, (vy - 0.5) * self.h);
        // The view is turned anticlockwise (on screen) by the angle; undo it.
        let (px, py) = (dx * self.cos - dy * self.sin, dx * self.sin + dy * self.cos);
        let (px, py) = match &self.rotation {
            None => (px, py),
            Some((r, (cx, cy))) => {
                let (u, v) = (px / self.focal, py / self.focal);
                let x = r[0] * u + r[1] * v + r[2];
                let y = r[3] * u + r[4] * v + r[5];
                let z = (r[6] * u + r[7] * v + r[8]).max(1.0e-3);
                ((x / z - cx) * self.focal, (y / z - cy) * self.focal)
            }
        };
        (self.w / 2.0 + px, self.h / 2.0 + py)
    }

    /// Whether the crop lies inside the photo. Straight lines stay straight under the
    /// mapping, so its corners inside the (convex) photo are enough.
    pub fn contains(&self, c: &CropRect) -> bool {
        [
            (c.x, c.y),
            (c.x + c.w, c.y),
            (c.x, c.y + c.h),
            (c.x + c.w, c.y + c.h),
        ]
        .iter()
        .all(|&(vx, vy)| {
            let (sx, sy) = self.source(vx, vy);
            // A little tolerance for rounding.
            sx >= -1e-3 && sx <= self.w + 1e-3 && sy >= -1e-3 && sy <= self.h + 1e-3
        })
    }
}

#[cfg(test)]
fn inside(c: &CropRect, straighten: f32, w: f32, h: f32) -> bool {
    let g = Geometry {
        straighten,
        ..Geometry::default()
    };
    Mapping::new(&g, w, h).contains(c)
}

/// The largest crop of `aspect`, centred in the view, inside the photo after all of
/// `geometry`'s straighten and perspective (its own crop is ignored).
pub fn fit_crop_for(aspect: AspectRatio, geometry: &Geometry, w: f32, h: f32) -> CropRect {
    let g = geometry.sanitized();
    if !g.has_perspective() {
        return fit_crop(aspect, g.straighten, w, h);
    }
    let ratio = aspect.ratio(w, h).unwrap_or(w / h);
    let map = Mapping::new(&g, w, h);
    // Half-width in pixels, searched: the rectangle grows until a corner leaves.
    let rect = |a: f32| {
        let (cw, ch) = (2.0 * a / w, 2.0 * a / ratio / h);
        CropRect {
            x: 0.5 - cw / 2.0,
            y: 0.5 - ch / 2.0,
            w: cw,
            h: ch,
        }
    };
    let (mut lo, mut hi) = (0.0f32, 0.5 * w.max(h * ratio));
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if map.contains(&rect(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    rect(lo)
}

/// The largest crop of `aspect` (the source's own shape when `None`... see
/// [`AspectRatio::ratio`]) centred in the view that fits inside the photo rotated by
/// `straighten`, for a source of `w` x `h`.
pub fn fit_crop(aspect: AspectRatio, straighten: f32, w: f32, h: f32) -> CropRect {
    let ratio = aspect.ratio(w, h).unwrap_or(w / h);
    let t = straighten
        .clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN)
        .to_radians()
        .abs();
    let (s, c) = t.sin_cos();
    // Half-width `a`, half-height `a / ratio`: the rotated corners must stay within the
    // source's half-extents.
    let a = (0.5 * w / (c + s / ratio)).min(0.5 * h / (s + c / ratio));
    let (cw, ch) = (2.0 * a / w, 2.0 * a / ratio / h);
    CropRect {
        x: 0.5 - cw / 2.0,
        y: 0.5 - ch / 2.0,
        w: cw,
        h: ch,
    }
}

/// `source` resampled into the output frame of `geometry` (see [`Geometry::output_size`]).
/// Without rotation, whole pixels are copied; with it, samples are bilinear, and
/// points outside the photo (only possible within rounding of its edge) clamp to it.
pub fn resample(source: &LinearImage, geometry: &Geometry) -> LinearImage {
    let (w, h) = (source.width(), source.height());
    let (ow, oh) = geometry.output_size(w, h);
    let crop = geometry.effective_crop(w as f32, h as f32);
    let g = geometry.sanitized();
    let mut data = vec![0u16; ow as usize * oh as usize * 3];
    let row_len = ow as usize * 3;
    if g.straighten == 0.0 && !g.has_perspective() {
        let x0 = ((crop.x * w as f32).round() as u32).min(w - ow);
        let y0 = ((crop.y * h as f32).round() as u32).min(h - oh);
        data.par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(r, out)| {
                let row = source.row(y0 + r as u32);
                out.copy_from_slice(&row[x0 as usize * 3..(x0 + ow) as usize * 3]);
            });
    } else {
        let map = Mapping::new(&g, w as f32, h as f32);
        data.par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(r, out)| {
                let vy = crop.y + (r as f32 + 0.5) / oh as f32 * crop.h;
                for (i, px) in out.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let vx = crop.x + (i as f32 + 0.5) / ow as f32 * crop.w;
                    let (sx, sy) = map.source(vx, vy);
                    *px = bilinear(source, sx - 0.5, sy - 0.5);
                }
            });
    }
    LinearImage::new(ow, oh, data).expect("dimensions match the buffer")
}

/// Auto level (ADR 0033): the straighten angle that levels the photo, or `None` when
/// there is no clear horizon or vertical to go by.
///
/// Line-like edges are measured with a structure tensor (gradients of log luminance,
/// averaged over 7x7 pixels): its orientation gives each edge's direction, and its
/// coherence how line-like the neighbourhood is. Edges within 15° of horizontal or
/// vertical vote for the angle that would level them, weighted by strength and
/// coherence, so texture (grass, foliage, noise), which points every way, adds no
/// preferred angle. The brightness is lightly blurred first, so pixel noise does not
/// count as edges. The answer is the histogram's peak, if it stands out.
pub fn auto_level(image: &LinearImage) -> Option<f32> {
    level_estimate(image)
        .filter(|e| e.confidence >= MIN_LEVEL_CONFIDENCE)
        .map(|e| e.angle)
}

/// Share of the edge evidence that must agree (within ±0.5°) for Auto level to act:
/// between pure noise and organic scenes (0.04–0.05 on the samples) and a real horizon
/// (0.086 on the Ricoh beach; ADR 0033).
const MIN_LEVEL_CONFIDENCE: f32 = 0.075;

/// Auto level's best angle and how much of the edge evidence agrees with it (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelEstimate {
    pub angle: f32,
    pub confidence: f32,
}

/// See [`auto_level`]; the estimate before the confidence threshold.
pub fn level_estimate(image: &LinearImage) -> Option<LevelEstimate> {
    const BINS_PER_DEGREE: f32 = 10.0;
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 16 || h < 16 {
        return None;
    }
    // log2 luminance.
    let l: Vec<f32> = image
        .data()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| {
            let y = 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
            (y / 65535.0).max(1.0e-4).log2()
        })
        .collect();
    // A light blur first: pixel noise averages out, long edges do not.
    let mut l = l;
    let mut scratch = crate::ops::detail::BlurScratch::default();
    crate::ops::detail::box_plane(&mut l, w, h, 1, 2, &mut scratch);
    // Structure tensor components, then averaged.
    let mut jxx = vec![0.0f32; w * h];
    let mut jyy = vec![0.0f32; w * h];
    let mut jxy = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let k = y * w + x;
            // Scharr kernels: much less direction bias than central differences, which
            // would pull small tilts towards the pixel axes.
            let (a, b, c) = (l[k - w - 1], l[k - w], l[k - w + 1]);
            let (d, f) = (l[k - 1], l[k + 1]);
            let (g, hh, i) = (l[k + w - 1], l[k + w], l[k + w + 1]);
            let gx = 3.0 * (c - a) + 10.0 * (f - d) + 3.0 * (i - g);
            let gy = 3.0 * (g - a) + 10.0 * (hh - b) + 3.0 * (i - c);
            jxx[k] = gx * gx;
            jyy[k] = gy * gy;
            jxy[k] = gx * gy;
        }
    }
    for plane in [&mut jxx, &mut jyy, &mut jxy] {
        crate::ops::detail::box_plane(plane, w, h, 3, 1, &mut scratch);
    }

    let bins = (2.0 * MAX_STRAIGHTEN * BINS_PER_DEGREE) as usize + 1;
    let mut hist = vec![0.0f64; bins];
    for k in 0..w * h {
        let (a, b, c) = (jxx[k], jyy[k], jxy[k]);
        let trace = a + b;
        if trace < 1.0e-2 {
            continue;
        }
        let spread = ((a - b) * (a - b) + 4.0 * c * c).sqrt();
        let coherence = spread / trace;
        // The dominant gradient's orientation; edges run across it.
        let gradient = 0.5 * (2.0 * c).atan2(a - b);
        let mut line = (gradient + std::f32::consts::FRAC_PI_2).to_degrees();
        // Into (-90, 90].
        if line > 90.0 {
            line -= 180.0;
        }
        if line <= -90.0 {
            line += 180.0;
        }
        // The straighten angle that would level this edge.
        let correction = if line.abs() <= MAX_STRAIGHTEN {
            line
        } else if line.abs() >= 90.0 - MAX_STRAIGHTEN {
            line - 90.0 * line.signum()
        } else {
            continue;
        };
        let bin = ((correction + MAX_STRAIGHTEN) * BINS_PER_DEGREE).round() as usize;
        let c2 = coherence * coherence;
        hist[bin.min(bins - 1)] += f64::from(trace.sqrt() * c2 * c2);
    }

    // Smooth (Gaussian, 0.3°), find the peak, and require it to stand out.
    let sigma = 3.0f64;
    let kernel: Vec<f64> = (-9..=9)
        .map(|i| (-(f64::from(i) * f64::from(i)) / (2.0 * sigma * sigma)).exp())
        .collect();
    let smooth: Vec<f64> = (0..bins)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(j, k)| {
                    let src = i as i64 + j as i64 - 9;
                    if (0..bins as i64).contains(&src) {
                        k * hist[src as usize]
                    } else {
                        0.0
                    }
                })
                .sum()
        })
        .collect();
    let total: f64 = hist.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let peak = (0..bins).max_by(|&a, &b| smooth[a].total_cmp(&smooth[b]))?;
    let near: f64 = hist[peak.saturating_sub(5)..(peak + 6).min(bins)]
        .iter()
        .sum();
    // Sub-bin position from a parabola through the peak and its neighbours.
    let offset = if peak > 0 && peak + 1 < bins {
        let (l, c, r) = (smooth[peak - 1], smooth[peak], smooth[peak + 1]);
        let d = l - 2.0 * c + r;
        if d.abs() > 1e-12 {
            (0.5 * (l - r) / d).clamp(-0.5, 0.5)
        } else {
            0.0
        }
    } else {
        0.0
    };
    let angle = (peak as f64 + offset) as f32 / BINS_PER_DEGREE - MAX_STRAIGHTEN;
    Some(LevelEstimate {
        angle: (angle * 10.0).round() / 10.0,
        confidence: (near / total) as f32,
    })
}

#[inline]
fn bilinear(source: &LinearImage, x: f32, y: f32) -> [u16; 3] {
    let (w, h) = (source.width() as usize, source.height() as usize);
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let data = source.data();
    let at = |xx: usize, yy: usize, c: usize| f32::from(data[(yy * w + xx) * 3 + c]);
    [0, 1, 2].map(|c| {
        let top = at(x0, y0, c) + (at(x1, y0, c) - at(x0, y0, c)) * tx;
        let bottom = at(x0, y1, c) + (at(x1, y1, c) - at(x0, y1, c)) * tx;
        (top + (bottom - top) * ty).round() as u16
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> LinearImage {
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| [(x * 100) as u16, (y * 100) as u16, 1000])
            .collect();
        LinearImage::new(w, h, data).unwrap()
    }

    #[test]
    fn identity_and_sanitising() {
        assert!(Geometry::default().is_identity());
        let g = Geometry {
            straighten: 40.0,
            crop: CropRect {
                x: -0.5,
                y: 0.9,
                w: 2.0,
                h: 0.5,
            },
            aspect: AspectRatio::Free,
            ..Geometry::default()
        }
        .sanitized();
        assert_eq!(g.straighten, MAX_STRAIGHTEN);
        assert_eq!((g.crop.x, g.crop.w), (0.0, 1.0));
        assert!((g.crop.y - 0.5).abs() < 1e-6 && g.crop.h == 0.5);
    }

    #[test]
    fn a_plain_crop_copies_pixels_exactly() {
        let src = gradient(100, 60);
        let g = Geometry {
            crop: CropRect {
                x: 0.2,
                y: 0.5,
                w: 0.5,
                h: 0.25,
            },
            ..Default::default()
        };
        let out = resample(&src, &g);
        assert_eq!((out.width(), out.height()), (50, 15));
        // Top-left output pixel is source (20, 30).
        assert_eq!(&out.data()[..3], &[2000, 3000, 1000]);
    }

    #[test]
    fn fitted_crops_keep_their_shape_and_stay_inside() {
        let (w, h) = (6000.0, 4000.0);
        for aspect in [
            AspectRatio::Original,
            AspectRatio::Square,
            AspectRatio::Wide16x9,
        ] {
            for angle in [0.0, 3.0, -7.5, 15.0] {
                let c = fit_crop(aspect, angle, w, h);
                let ratio = (c.w * w) / (c.h * h);
                let want = aspect.ratio(w, h).unwrap();
                assert!(
                    (ratio / want - 1.0).abs() < 1e-3,
                    "{aspect:?} {angle}: {ratio}"
                );
                assert!(inside(&c, angle, w, h), "{aspect:?} {angle}: {c:?}");
                // Largest: 1 % bigger would not fit.
                let bigger = scaled_about_centre(&c, 1.01);
                assert!(
                    angle == 0.0 && aspect == AspectRatio::Original
                        || !inside(&bigger, angle, w, h)
                );
            }
        }
        // No rotation, original shape: the whole frame.
        assert_eq!(fit_crop(AspectRatio::Original, 0.0, w, h), CropRect::FULL);
    }

    /// Uniform noise in 0..1 for a pixel (splitmix64: no structure, unlike simple
    /// linear hashes, which draw stripes the detector would rightly find).
    fn noise(x: u32, y: u32) -> f32 {
        let mut z = (u64::from(y) << 32 | u64::from(x)).wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A scene with long straight edges tilted by `tilt` degrees (anticlockwise on
    /// screen): a horizon between sky and sea, and a few vertical posts.
    fn tilted_scene(w: u32, h: u32, tilt: f32, texture: bool) -> LinearImage {
        let t = tilt.to_radians();
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                // Scene coordinates: undo the tilt.
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                let (sx, sy) = (dx * t.cos() - dy * t.sin(), dx * t.sin() + dy * t.cos());
                // Soft (anti-aliased) edges, as a lens draws them: a pixel-aligned
                // staircase would read as level at small tilts.
                let ramp = |d: f32| ((d + 1.0) / 2.0).clamp(0.0, 1.0);
                let sea = ramp(sy);
                let mut v = 40000.0 + (9000.0 - 40000.0) * sea;
                // Posts: 5 px wide, every 60 px, below the horizon.
                let m = sx.rem_euclid(60.0);
                let post = ramp(m.min(5.0 - m).min(2.5)) * sea;
                v += (25000.0 - v) * post;
                if texture {
                    v *= 0.8 + 0.4 * noise(x, y);
                }
                [v as u16; 3]
            })
            .collect();
        LinearImage::new(w, h, data).unwrap()
    }

    #[test]
    fn auto_level_finds_the_tilt() {
        for tilt in [-7.5f32, -2.0, 0.0, 3.3, 11.0] {
            let img = tilted_scene(600, 400, tilt, true);
            let a = auto_level(&img).expect("a clear horizon");
            // The correction turns the picture back: the opposite way.
            assert!((a + tilt).abs() <= 0.35, "tilt {tilt}: {a}");
        }
    }

    #[test]
    fn auto_level_levels_what_straighten_turns() {
        // Straightening by the answer levels the horizon (the same convention).
        let img = tilted_scene(600, 400, 4.0, false);
        let a = auto_level(&img).unwrap();
        let g = Geometry {
            straighten: a,
            crop: fit_crop(AspectRatio::Original, a, 600.0, 400.0),
            aspect: AspectRatio::Original,
            ..Geometry::default()
        };
        let levelled = resample(&img, &g);
        let again = auto_level(&levelled).unwrap();
        assert!(again.abs() <= 0.35, "{again}");
    }

    #[test]
    fn auto_level_declines_texture_without_lines() {
        let (w, h) = (400u32, 300u32);
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| [(5000.0 + 40_000.0 * noise(x, y)) as u16; 3])
            .collect();
        let img = LinearImage::new(w, h, data).unwrap();
        assert_eq!(auto_level(&img), None);
    }

    #[test]
    fn fit_crop_matches_the_ui() {
        // The same case as apps/desktop/src/features/editor/cropGeometry.test.ts.
        let c = fit_crop(AspectRatio::Original, 5.0, 6000.0, 4000.0);
        assert!(
            (c.w - 0.887_37).abs() < 1e-4 && (c.h - 0.887_37).abs() < 1e-4,
            "{c:?}"
        );
        assert!((c.x - 0.056_32).abs() < 1e-4, "{c:?}");
    }

    #[test]
    fn crops_outside_the_rotated_photo_are_pulled_in() {
        let g = Geometry {
            straighten: 10.0,
            crop: CropRect::FULL,
            aspect: AspectRatio::Original,
            ..Geometry::default()
        };
        let c = g.effective_crop(3000.0, 2000.0);
        assert!(c.w < 1.0 && inside(&c, 10.0, 3000.0, 2000.0));
        // Kept centred and in shape.
        assert!((c.x + c.w / 2.0 - 0.5).abs() < 1e-4);
        assert!(((c.w * 3000.0) / (c.h * 2000.0) - 1.5).abs() < 1e-3);
    }

    #[test]
    fn straightening_rotates_about_the_centre() {
        // A source with a bright horizontal line through the middle, tilted by the
        // angle, comes out level after straightening by the opposite angle.
        let (w, h) = (400u32, 300u32);
        let tilt = 5.0f32.to_radians();
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let line_y = 150.0 - (x as f32 - 200.0) * tilt.tan();
                let v = if (y as f32 - line_y).abs() < 2.0 {
                    60000
                } else {
                    1000
                };
                [v, v, v]
            })
            .collect();
        let src = LinearImage::new(w, h, data).unwrap();
        let g = Geometry {
            straighten: -5.0,
            crop: fit_crop(AspectRatio::Original, -5.0, w as f32, h as f32),
            aspect: AspectRatio::Original,
            ..Geometry::default()
        };
        let out = resample(&src, &g);
        let (ow, oh) = (out.width() as usize, out.height() as usize);
        // The brightest row is the same at the left and right thirds.
        let brightest_row = |x: usize| {
            (0..oh)
                .max_by_key(|&y| out.data()[(y * ow + x) * 3])
                .unwrap()
        };
        let (l, r) = (brightest_row(ow / 3), brightest_row(2 * ow / 3));
        assert!(l.abs_diff(r) <= 1, "{l} vs {r}");
    }

    fn perspective(vertical: f32, horizontal: f32) -> Geometry {
        Geometry {
            vertical,
            horizontal,
            ..Geometry::default()
        }
    }

    #[test]
    fn perspective_widens_the_named_side_about_the_centre() {
        let (w, h) = (6000.0, 4000.0);
        // Vertical: the top samples a narrower span of the photo, so it is widened.
        let up = Mapping::new(&perspective(50.0, 0.0), w, h);
        let top = up.source(1.0, 0.0).0 - up.source(0.0, 0.0).0;
        let bottom = up.source(1.0, 1.0).0 - up.source(0.0, 1.0).0;
        assert!(top < 0.9 * bottom, "{top} vs {bottom}");
        // Horizontal: the right side samples a shorter span, so it is taller.
        let right = Mapping::new(&perspective(0.0, 50.0), w, h);
        let r = right.source(1.0, 1.0).1 - right.source(1.0, 0.0).1;
        let l = right.source(0.0, 1.0).1 - right.source(0.0, 0.0).1;
        assert!(r < 0.9 * l, "{r} vs {l}");
        // Both keep the centre where it is, and negative values do the opposite.
        for g in [perspective(50.0, 0.0), perspective(-30.0, 70.0)] {
            let (x, y) = Mapping::new(&g, w, h).source(0.5, 0.5);
            assert!((x - w / 2.0).abs() < 0.01 && (y - h / 2.0).abs() < 0.01);
        }
        let down = Mapping::new(&perspective(-50.0, 0.0), w, h);
        assert!(down.source(1.0, 0.0).0 - down.source(0.0, 0.0).0 > top);
    }

    #[test]
    fn perspective_keeps_straight_lines_straight() {
        let map = Mapping::new(
            &Geometry {
                straighten: 4.0,
                ..perspective(60.0, -35.0)
            },
            3000.0,
            2000.0,
        );
        for (a, b) in [((0.1, 0.2), (0.9, 0.7)), ((0.3, 0.0), (0.35, 1.0))] {
            let p = map.source(a.0, a.1);
            let q = map.source(b.0, b.1);
            let m = map.source(0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1));
            // The midpoint's distance from the line through the ends.
            let cross = (q.0 - p.0) * (m.1 - p.1) - (q.1 - p.1) * (m.0 - p.0);
            let len = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
            assert!((cross / len).abs() < 0.05, "{}", cross / len);
        }
    }

    #[test]
    fn fitted_crops_with_perspective_are_the_largest_inside() {
        let (w, h) = (6000.0, 4000.0);
        for aspect in [AspectRatio::Original, AspectRatio::Square] {
            for (v, hz, s) in [(40.0, 0.0, 0.0), (-100.0, 30.0, 5.0), (0.0, 100.0, -15.0)] {
                let g = Geometry {
                    straighten: s,
                    ..perspective(v, hz)
                };
                let map = Mapping::new(&g, w, h);
                let c = fit_crop_for(aspect, &g, w, h);
                let ratio = (c.w * w) / (c.h * h);
                assert!((ratio / aspect.ratio(w, h).unwrap() - 1.0).abs() < 1e-3);
                assert!(map.contains(&c), "{v} {hz} {s}: {c:?}");
                assert!(!map.contains(&scaled_about_centre(&c, 1.01)));
                assert!(c.w > 0.4, "{v} {hz} {s}: {c:?}");
            }
        }
        // Without perspective it is the closed form.
        let g = Geometry {
            straighten: 5.0,
            ..Geometry::default()
        };
        assert_eq!(
            fit_crop_for(AspectRatio::Original, &g, w, h),
            fit_crop(AspectRatio::Original, 5.0, w, h)
        );
    }

    #[test]
    fn perspective_pulls_the_full_frame_in_and_resamples_it() {
        let src = gradient(300, 200);
        let g = perspective(50.0, 0.0);
        let c = g.effective_crop(300.0, 200.0);
        assert!(c.w < 1.0 && Mapping::new(&g, 300.0, 200.0).contains(&c));
        let out = resample(&src, &g);
        let (ow, oh) = g.output_size(300, 200);
        assert_eq!((out.width(), out.height()), (ow, oh));
        assert!(!perspective(0.0, 0.0).has_perspective() && !g.is_identity());
    }

    #[test]
    fn perspective_is_clamped_when_sanitised() {
        let g = perspective(250.0, f32::NAN).sanitized();
        assert_eq!((g.vertical, g.horizontal), (100.0, 0.0));
    }

    #[test]
    fn fit_crop_for_matches_the_ui() {
        // The same case is in cropGeometry.test.ts.
        let g = Geometry {
            straighten: 3.0,
            ..perspective(40.0, -20.0)
        };
        let c = fit_crop_for(AspectRatio::Original, &g, 6000.0, 4000.0);
        assert!(
            (c.w - 0.84362).abs() < 1e-4 && (c.h - 0.84362).abs() < 1e-4,
            "{c:?}"
        );
    }
}
