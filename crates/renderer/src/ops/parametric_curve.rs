//! The parametric tone curve (ADR 0051): Lightroom's region sliders. Four regions of
//! the tonal range (Shadows, Darks, Lights, Highlights), split at three points, each
//! with a slider that lifts or lowers its region. It runs before the point curve, as
//! in Lightroom.
//!
//! Each slider moves its region's middle up or down by up to 45 % of the region's
//! width; the curve through black, those four points and white is the point curve's
//! monotone cubic, so it stays smooth, keeps black and white, and never folds back.
//! The shape is this app's own: it comes close to Lightroom's, not exactly.

use serde::{Deserialize, Serialize};

use crate::ops::point_curve::PointCurve;

/// How far a slider at ±100 moves its region's middle, as a share of the region.
const REACH: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ParametricCurve {
    /// Each region's slider, -100..100.
    pub shadows: f32,
    pub darks: f32,
    pub lights: f32,
    pub highlights: f32,
    /// Where the regions meet, as tones 0..100 (Lightroom's defaults 25, 50, 75).
    pub shadow_split: f32,
    pub midtone_split: f32,
    pub highlight_split: f32,
}

impl Default for ParametricCurve {
    fn default() -> Self {
        Self {
            shadows: 0.0,
            darks: 0.0,
            lights: 0.0,
            highlights: 0.0,
            shadow_split: 25.0,
            midtone_split: 50.0,
            highlight_split: 75.0,
        }
    }
}

impl ParametricCurve {
    pub fn is_identity(&self) -> bool {
        self.shadows == 0.0 && self.darks == 0.0 && self.lights == 0.0 && self.highlights == 0.0
    }

    /// Sliders within -100..100; splits in order, at least 5 apart, within 5..95.
    pub fn sanitized(self) -> Self {
        let slider = |v: f32| {
            if v.is_finite() {
                v.clamp(-100.0, 100.0)
            } else {
                0.0
            }
        };
        let split = |v: f32, or: f32| if v.is_finite() { v } else { or };
        let s1 = split(self.shadow_split, 25.0).clamp(5.0, 85.0);
        let s2 = split(self.midtone_split, 50.0).clamp(s1 + 5.0, 90.0);
        let s3 = split(self.highlight_split, 75.0).clamp(s2 + 5.0, 95.0);
        Self {
            shadows: slider(self.shadows),
            darks: slider(self.darks),
            lights: slider(self.lights),
            highlights: slider(self.highlights),
            shadow_split: s1,
            midtone_split: s2,
            highlight_split: s3,
        }
    }

    /// The curve as points through black, the four regions' middles and white.
    pub fn to_point_curve(&self) -> PointCurve {
        let c = self.sanitized();
        let edges = [
            0.0,
            c.shadow_split / 100.0,
            c.midtone_split / 100.0,
            c.highlight_split / 100.0,
            1.0,
        ];
        let sliders = [c.shadows, c.darks, c.lights, c.highlights];
        let mut points = vec![[0.0, 0.0]];
        for (i, amount) in sliders.iter().enumerate() {
            let (start, end) = (edges[i], edges[i + 1]);
            let middle = (start + end) / 2.0;
            points.push([middle, middle + amount / 100.0 * REACH * (end - start)]);
        }
        points.push([1.0, 1.0]);
        PointCurve::new(&points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(shadows: f32, darks: f32, lights: f32, highlights: f32) -> PointCurve {
        ParametricCurve {
            shadows,
            darks,
            lights,
            highlights,
            ..Default::default()
        }
        .to_point_curve()
    }

    #[test]
    fn at_zero_it_is_the_diagonal() {
        assert!(ParametricCurve::default().is_identity());
        assert!(curve(0.0, 0.0, 0.0, 0.0).is_identity());
    }

    #[test]
    fn each_slider_moves_mostly_its_own_region() {
        let lifted = curve(0.0, 0.0, 60.0, 0.0);
        // Lights (50..75) rise most in their middle; deep shadows barely move.
        let d = |x: f32| lifted.eval(x) - x;
        assert!(d(0.625) > 0.05, "{}", d(0.625));
        assert!(d(0.625) > d(0.3) && d(0.625) > d(0.9));
        assert!(d(0.1).abs() < 0.01);
        // Darkening highlights pulls them down; black and white stay put.
        let darker = curve(0.0, 0.0, 0.0, -80.0);
        assert!(darker.eval(0.875) < 0.875 - 0.05);
        assert_eq!((darker.eval(0.0), darker.eval(1.0)), (0.0, 1.0));
    }

    #[test]
    fn even_at_the_ends_it_never_folds_back() {
        for a in [-100.0, 100.0] {
            for b in [-100.0, 100.0] {
                let c = curve(a, b, -a, -b);
                let mut last = -1.0;
                for i in 0..=200 {
                    let y = c.eval(i as f32 / 200.0);
                    assert!(y >= last - 1e-6, "folds at {i}");
                    last = y;
                }
            }
        }
    }

    #[test]
    fn splits_stay_ordered() {
        let c = ParametricCurve {
            shadow_split: 70.0,
            midtone_split: 20.0,
            highlight_split: f32::NAN,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (c.shadow_split, c.midtone_split, c.highlight_split),
            (70.0, 75.0, 80.0)
        );
    }
}
