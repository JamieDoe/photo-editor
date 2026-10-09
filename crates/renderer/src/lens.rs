//! Lens corrections from a profile (ADR 0075): the lens's distortion undone in the
//! geometry mapping ([`crate::geometry::Mapping`]), and its vignetting as a gain.
//!
//! A profile gives curves of the radius from the image's centre, as fractions of its
//! half-diagonal: where the lens really puts a point (as a multiple of the ideal
//! radius), and how bright it leaves it (relative to the centre). The corrected photo
//! is scaled, when it has to be, so it still fills the frame: pincushion correction
//! would otherwise leave gaps at the edges.

/// Knots a profile may have.
pub const MAX_KNOTS: usize = 32;

/// A lens profile for one photo, ready to apply at any size ([`LensCorrection::at`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensCorrection {
    n: usize,
    knots: [f32; MAX_KNOTS],
    /// Real radius over ideal radius at each knot (1: none).
    distortion: [f32; MAX_KNOTS],
    /// Brightness relative to the centre at each knot (1: none).
    vignetting: [f32; MAX_KNOTS],
    has_distortion: bool,
    has_vignetting: bool,
    /// Whether the knots are evenly spaced from 0 to 1 (as Sony's are), so a radius's
    /// knot is found directly.
    uniform: bool,
    /// The ideal image's scale that keeps the corrected photo filling the frame.
    fill: f32,
}

/// A [`LensCorrection`] for a source of a given size: its centre and half-diagonal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensAt {
    lens: LensCorrection,
    centre: (f32, f32),
    half_diagonal: f32,
}

/// The distortion a profile may undo, either way (a quarter of the radius is far
/// beyond any real lens).
const MAX_DISTORTION: f32 = 0.25;
/// The darkest vignetting a profile may lift: four stops.
const MIN_BRIGHTNESS: f32 = 1.0 / 16.0;

impl LensCorrection {
    /// A profile's curves at `knots` (ascending, 0..=1 of the half-diagonal). `None`
    /// when the curves don't fit together or correct nothing.
    pub fn new(
        knots: &[f32],
        distortion: Option<&[f32]>,
        vignetting: Option<&[f32]>,
    ) -> Option<Self> {
        let n = knots.len();
        let ascending = knots.windows(2).all(|k| k[1] > k[0]);
        let fits =
            |c: Option<&[f32]>| c.is_none_or(|c| c.len() == n && c.iter().all(|v| v.is_finite()));
        if !(2..=MAX_KNOTS).contains(&n)
            || !ascending
            || !knots.iter().all(|k| k.is_finite())
            || !fits(distortion)
            || !fits(vignetting)
        {
            return None;
        }
        let curve = |values: Option<&[f32]>, lo: f32, hi: f32| {
            let mut out = [1.0; MAX_KNOTS];
            if let Some(values) = values {
                for (o, v) in out.iter_mut().zip(values) {
                    *o = v.clamp(lo, hi);
                }
            }
            out
        };
        let mut lens = Self {
            n,
            knots: {
                let mut k = [0.0; MAX_KNOTS];
                k[..n].copy_from_slice(knots);
                k
            },
            distortion: curve(distortion, 1.0 - MAX_DISTORTION, 1.0 + MAX_DISTORTION),
            vignetting: curve(vignetting, MIN_BRIGHTNESS, 1.0),
            has_distortion: distortion.is_some_and(|d| d.iter().any(|&v| v != 1.0)),
            has_vignetting: vignetting.is_some_and(|v| v.iter().any(|&b| b < 1.0)),
            uniform: knots
                .iter()
                .enumerate()
                .all(|(i, &k)| (k - i as f32 / (n - 1) as f32).abs() < 1e-6),
            fill: 1.0,
        };
        if !lens.has_distortion && !lens.has_vignetting {
            return None;
        }
        lens.fill = lens.fill_scale();
        Some(lens)
    }

    pub fn has_distortion(&self) -> bool {
        self.has_distortion
    }

    pub fn has_vignetting(&self) -> bool {
        self.has_vignetting
    }

    /// The ideal image's scale (at most 1) at which every point of the frame comes
    /// from the photo: the largest `s` with `s · d(s · r) ≤ 1` for every radius (where
    /// a frame point at radius `r` is sampled at `r · s · d(s · r)`, inside the frame
    /// whenever that factor is at most 1).
    fn fill_scale(&self) -> f32 {
        if !self.has_distortion {
            return 1.0;
        }
        let fits =
            |s: f32| (0..=64).all(|i| s * self.distortion_at(s * i as f32 / 64.0) <= 1.0 + 1e-6);
        if fits(1.0) {
            return 1.0;
        }
        let (mut lo, mut hi) = (1.0 - MAX_DISTORTION, 1.0);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// It for a source of `width` × `height` pixels.
    pub fn at(&self, width: f32, height: f32) -> LensAt {
        LensAt {
            lens: *self,
            centre: (width / 2.0, height / 2.0),
            half_diagonal: (width.hypot(height) / 2.0).max(1e-3),
        }
    }

    /// The largest gain [`LensAt::vignetting_gain`] gives (1 without
    /// vignetting): framed images are stored divided by it, as they have no headroom
    /// above the sensor's white, and the plan multiplies it back.
    pub fn headroom(&self) -> f32 {
        if !self.has_vignetting {
            return 1.0;
        }
        self.vignetting[..self.n]
            .iter()
            .fold(1.0f32, |m, &b| m.max(1.0 / b))
    }

    fn distortion_at(&self, r: f32) -> f32 {
        self.curve(&self.distortion, r)
    }

    #[inline]
    fn curve(&self, values: &[f32; MAX_KNOTS], r: f32) -> f32 {
        let (knots, values) = (&self.knots[..self.n], &values[..self.n]);
        if self.uniform && r > 0.0 && r < 1.0 {
            let i = ((r * (self.n - 1) as f32) as usize).min(self.n - 2);
            hermite(knots, values, i, r)
        } else {
            interpolate(knots, values, r)
        }
    }

    /// Bits identifying it, for cache keys.
    pub fn key(&self) -> [u32; 4] {
        let sum = |v: &[f32]| v.iter().fold(0u32, |h, x| h.rotate_left(5) ^ x.to_bits());
        [
            self.n as u32
                ^ (u32::from(self.has_distortion) << 8)
                ^ (u32::from(self.has_vignetting) << 9),
            self.fill.to_bits(),
            sum(&self.distortion[..self.n]) ^ sum(&self.knots[..self.n]).rotate_left(11),
            sum(&self.vignetting[..self.n]),
        ]
    }
}

impl LensAt {
    pub fn lens(&self) -> &LensCorrection {
        &self.lens
    }

    /// Where the lens put the scene's point that an ideal lens would put at source
    /// pixel (`x`, `y`): its sensor position, in the same pixels.
    #[inline]
    pub fn distorted(&self, x: f32, y: f32) -> (f32, f32) {
        let lens = &self.lens;
        if !lens.has_distortion {
            return (x, y);
        }
        let (dx, dy) = (x - self.centre.0, y - self.centre.1);
        let r = (dx * dx + dy * dy).sqrt() / self.half_diagonal * lens.fill;
        let k = lens.fill * lens.distortion_at(r);
        (self.centre.0 + dx * k, self.centre.1 + dy * k)
    }

    /// The gain that undoes the lens's vignetting at sensor pixel (`x`, `y`).
    #[inline]
    pub fn vignetting_gain(&self, x: f32, y: f32) -> f32 {
        let lens = &self.lens;
        let (dx, dy) = (x - self.centre.0, y - self.centre.1);
        let r = (dx * dx + dy * dy).sqrt() / self.half_diagonal;
        1.0 / lens.curve(&lens.vignetting, r)
    }
}

/// `values` at `x` between `knots`: a cubic Hermite curve through them, its slopes
/// from the neighbouring knots, so it is smooth (straight lines stay smooth curves);
/// beyond the ends, the end values.
fn interpolate(knots: &[f32], values: &[f32], x: f32) -> f32 {
    let n = knots.len();
    if x <= knots[0] {
        return values[0];
    }
    if x >= knots[n - 1] {
        return values[n - 1];
    }
    let i = knots
        .partition_point(|&k| k <= x)
        .saturating_sub(1)
        .min(n - 2);
    hermite(knots, values, i, x)
}

/// The cubic Hermite curve through `values` between knots `i` and `i + 1`, at `x`.
#[inline]
fn hermite(knots: &[f32], values: &[f32], i: usize, x: f32) -> f32 {
    let n = knots.len();
    let (x0, x1) = (knots[i], knots[i + 1]);
    let (y0, y1) = (values[i], values[i + 1]);
    let slope = |j: usize| {
        let (a, b) = (j.saturating_sub(1), (j + 1).min(n - 1));
        (values[b] - values[a]) / (knots[b] - knots[a])
    };
    let h = x1 - x0;
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0
        + (t3 - 2.0 * t2 + t) * h * slope(i)
        + (-2.0 * t3 + 3.0 * t2) * y1
        + (t3 - t2) * h * slope(i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knots(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32 / (n - 1) as f32).collect()
    }

    #[test]
    fn interpolation_passes_through_the_knots_and_stays_smooth() {
        let k = knots(5);
        let v = [1.0, 0.99, 0.97, 0.94, 0.9];
        for (x, y) in k.iter().zip(v) {
            assert!((interpolate(&k, &v, *x) - y).abs() < 1e-6);
        }
        // Between knots, between their values; beyond the ends, the ends.
        let mid = interpolate(&k, &v, 0.6);
        assert!(mid < 0.97 && mid > 0.94, "{mid}");
        assert_eq!(interpolate(&k, &v, 1.3), 0.9);
        assert_eq!(interpolate(&k, &v, -0.1), 1.0);
    }

    #[test]
    fn evenly_spaced_knots_are_found_directly_with_the_same_curve() {
        let even = knots(16);
        let v: Vec<f32> = even.iter().map(|r| 1.0 - 0.3 * r * r * r).collect();
        let lens = LensCorrection::new(&even, None, Some(&v)).unwrap();
        assert!(lens.uniform);
        let mut padded = [1.0; MAX_KNOTS];
        padded[..16].copy_from_slice(&v);
        for i in 0..=200 {
            let r = i as f32 / 200.0;
            assert!(
                (lens.curve(&padded, r) - interpolate(&even, &v, r)).abs() < 1e-6,
                "{r}"
            );
        }
    }

    #[test]
    fn barrel_is_undone_by_sampling_further_in_and_fills_without_scaling() {
        // Barrel: the corner really at 0.95 of its ideal radius.
        let d: Vec<f32> = knots(16).iter().map(|r| 1.0 - 0.05 * r * r).collect();
        let profile = LensCorrection::new(&knots(16), Some(&d), None).unwrap();
        assert_eq!(profile.fill, 1.0);
        let lens = profile.at(600.0, 400.0);
        // The corner comes from 0.95 of the way out; the centre stays.
        let (x, y) = lens.distorted(600.0, 400.0);
        assert!(
            (x - (300.0 + 300.0 * 0.95)).abs() < 0.01 && (y - (200.0 + 200.0 * 0.95)).abs() < 0.01
        );
        assert_eq!(lens.distorted(300.0, 200.0), (300.0, 200.0));
    }

    #[test]
    fn pincushion_is_scaled_to_fill_the_frame() {
        let d: Vec<f32> = knots(16).iter().map(|r| 1.0 + 0.04 * r * r).collect();
        let profile = LensCorrection::new(&knots(16), Some(&d), None).unwrap();
        assert!(
            profile.fill < 1.0 && profile.fill > 0.95,
            "{}",
            profile.fill
        );
        let lens = profile.at(600.0, 400.0);
        // Every edge point of the frame comes from inside the photo, and the corner
        // from its very edge.
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            for (x, y) in [
                (t * 600.0, 0.0),
                (t * 600.0, 400.0),
                (0.0, t * 400.0),
                (600.0, t * 400.0),
            ] {
                let (sx, sy) = lens.distorted(x, y);
                assert!(
                    (-0.01..=600.01).contains(&sx) && (-0.01..=400.01).contains(&sy),
                    "{x},{y} -> {sx},{sy}"
                );
            }
        }
        let (cx, _) = lens.distorted(600.0, 400.0);
        assert!(cx > 599.0, "{cx}");
    }

    #[test]
    fn vignetting_is_lifted_towards_the_corners() {
        let v: Vec<f32> = knots(16).iter().map(|r| 1.0 - 0.5 * r * r).collect();
        let profile = LensCorrection::new(&knots(16), None, Some(&v)).unwrap();
        assert!(!profile.has_distortion() && profile.has_vignetting());
        assert!((profile.headroom() - 2.0).abs() < 1e-4);
        let lens = profile.at(600.0, 400.0);
        assert!((lens.vignetting_gain(300.0, 200.0) - 1.0).abs() < 1e-6);
        assert!((lens.vignetting_gain(0.0, 0.0) - 2.0).abs() < 1e-4);
        // Distortion untouched.
        assert_eq!(lens.distorted(10.0, 20.0), (10.0, 20.0));
    }

    #[test]
    fn profiles_that_dont_fit_or_correct_nothing_are_none() {
        let k = knots(4);
        assert!(LensCorrection::new(&k, Some(&[1.0, 1.0, 1.0]), None).is_none());
        assert!(
            LensCorrection::new(&[0.0, 0.5, 0.4, 1.0], Some(&[1.0, 0.99, 0.98, 0.97]), None,)
                .is_none()
        );
        assert!(LensCorrection::new(&k, Some(&[1.0; 4]), Some(&[1.0; 4])).is_none());
        assert!(LensCorrection::new(&k, Some(&[1.0, f32::NAN, 1.0, 1.0]), None).is_none());
    }
}
