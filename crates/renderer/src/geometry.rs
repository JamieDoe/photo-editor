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
}

impl Geometry {
    /// No rotation and the whole frame (whatever the aspect choice).
    pub fn is_identity(&self) -> bool {
        self.straighten == 0.0 && self.crop == CropRect::FULL
    }

    pub fn sanitized(&self) -> Self {
        let s = if self.straighten.is_finite() {
            self.straighten.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN)
        } else {
            0.0
        };
        Self {
            straighten: if s == 0.0 { 0.0 } else { s },
            crop: self.crop.sanitized(),
            aspect: self.aspect,
        }
    }

    /// The crop actually used for a source of `w` x `h`: the stored crop, pulled
    /// towards the centre and shrunk just enough to lie inside the rotated photo.
    pub fn effective_crop(&self, w: f32, h: f32) -> CropRect {
        let g = self.sanitized();
        let crop = g.crop;
        if g.straighten == 0.0 || inside(&crop, g.straighten, w, h) {
            return crop;
        }
        // Scale about the frame's centre: at 0 it is a point there, which is inside.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if inside(&scaled_about_centre(&crop, mid), g.straighten, w, h) {
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

/// Maps a point of the straightened view (fractions of `w`, `h`) to source pixel
/// coordinates.
#[inline]
fn view_to_source(vx: f32, vy: f32, cos: f32, sin: f32, w: f32, h: f32) -> (f32, f32) {
    let (dx, dy) = ((vx - 0.5) * w, (vy - 0.5) * h);
    // The view is the source turned anticlockwise (on screen) by the angle; undo it.
    (w / 2.0 + dx * cos - dy * sin, h / 2.0 + dx * sin + dy * cos)
}

fn inside(c: &CropRect, straighten: f32, w: f32, h: f32) -> bool {
    let (sin, cos) = straighten.to_radians().sin_cos();
    [
        (c.x, c.y),
        (c.x + c.w, c.y),
        (c.x, c.y + c.h),
        (c.x + c.w, c.y + c.h),
    ]
    .iter()
    .all(|&(vx, vy)| {
        let (sx, sy) = view_to_source(vx, vy, cos, sin, w, h);
        // A little tolerance for rounding.
        sx >= -1e-3 && sx <= w + 1e-3 && sy >= -1e-3 && sy <= h + 1e-3
    })
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
    let angle = geometry.sanitized().straighten;
    let mut data = vec![0u16; ow as usize * oh as usize * 3];
    let row_len = ow as usize * 3;
    if angle == 0.0 {
        let x0 = ((crop.x * w as f32).round() as u32).min(w - ow);
        let y0 = ((crop.y * h as f32).round() as u32).min(h - oh);
        data.par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(r, out)| {
                let row = source.row(y0 + r as u32);
                out.copy_from_slice(&row[x0 as usize * 3..(x0 + ow) as usize * 3]);
            });
    } else {
        let (sin, cos) = angle.to_radians().sin_cos();
        let (wf, hf) = (w as f32, h as f32);
        data.par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(r, out)| {
                let vy = crop.y + (r as f32 + 0.5) / oh as f32 * crop.h;
                for (i, px) in out.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let vx = crop.x + (i as f32 + 0.5) / ow as f32 * crop.w;
                    let (sx, sy) = view_to_source(vx, vy, cos, sin, wf, hf);
                    *px = bilinear(source, sx - 0.5, sy - 0.5);
                }
            });
    }
    LinearImage::new(ow, oh, data).expect("dimensions match the buffer")
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
}
