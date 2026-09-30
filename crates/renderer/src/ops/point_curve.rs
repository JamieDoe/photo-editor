//! The tone curve (ADR 0037): points the photographer places and drags, as in
//! Lightroom's point curve.
//!
//! Coordinates are display tones (sRGB-encoded, 0..1) in and out. The curve runs after
//! the base look, per channel, so it shapes the picture as it appears. Between points
//! it is a monotone cubic (PCHIP: Fritsch–Butland tangents), which never overshoots
//! the points, so a gentle S stays gentle. Outside the first and last points it is
//! flat at their values.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Most points a curve keeps, the two ends included.
pub const MAX_POINTS: usize = 16;
/// Smallest horizontal gap between neighbouring points.
pub const MIN_GAP: f32 = 0.01;

/// The curve's points, sorted by input, at least two. Stored inline so recipes stay
/// `Copy`; serialised as a list of `[input, output]` pairs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointCurve {
    points: [[f32; 2]; MAX_POINTS],
    len: usize,
}

impl Default for PointCurve {
    /// The diagonal: no change.
    fn default() -> Self {
        Self::new(&[[0.0, 0.0], [1.0, 1.0]])
    }
}

impl PointCurve {
    /// A curve through `points`, cleaned: coordinates within 0..1 (non-finite ones
    /// dropped) and rounded to 1/10000, sorted, points closer than [`MIN_GAP`] to the
    /// previous one dropped, at most [`MAX_POINTS`]. Fewer than two points give the
    /// diagonal.
    pub fn new(points: &[[f32; 2]]) -> Self {
        let round = |v: f32| {
            let v = (v.clamp(0.0, 1.0) * 10_000.0).round() / 10_000.0;
            if v == 0.0 { 0.0 } else { v }
        };
        let mut clean: Vec<[f32; 2]> = points
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| [round(p[0]), round(p[1])])
            .collect();
        clean.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut kept: Vec<[f32; 2]> = Vec::with_capacity(MAX_POINTS);
        for p in clean {
            if kept.last().is_none_or(|q| p[0] - q[0] >= MIN_GAP - 1e-6) && kept.len() < MAX_POINTS
            {
                kept.push(p);
            }
        }
        if kept.len() < 2 {
            kept = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        let mut stored = [[0.0; 2]; MAX_POINTS];
        stored[..kept.len()].copy_from_slice(&kept);
        Self {
            points: stored,
            len: kept.len(),
        }
    }

    pub fn points(&self) -> &[[f32; 2]] {
        &self.points[..self.len]
    }

    /// The diagonal from (0, 0) to (1, 1): every point on it, so the curve is too.
    pub fn is_identity(&self) -> bool {
        let p = self.points();
        p[0] == [0.0, 0.0]
            && p[p.len() - 1] == [1.0, 1.0]
            && p.iter().all(|q| (q[0] - q[1]).abs() < 1e-6)
    }

    /// The output tone for input tone `x` (both 0..1).
    pub fn eval(&self, x: f32) -> f32 {
        let p = self.points();
        let n = p.len();
        if x <= p[0][0] {
            return p[0][1];
        }
        if x >= p[n - 1][0] {
            return p[n - 1][1];
        }
        let i = p.partition_point(|q| q[0] <= x) - 1;
        let m = tangents(p);
        let h = p[i + 1][0] - p[i][0];
        let t = (x - p[i][0]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * p[i][1]
            + (t3 - 2.0 * t2 + t) * h * m[i]
            + (-2.0 * t3 + 3.0 * t2) * p[i + 1][1]
            + (t3 - t2) * h * m[i + 1];
        y.clamp(0.0, 1.0)
    }

    /// Values at `n + 1` evenly spaced inputs from 0 to 1, for lookup tables.
    pub fn sample(&self, n: usize) -> Vec<f32> {
        let p = self.points();
        let m = tangents(p);
        let mut seg = 0;
        (0..=n)
            .map(|k| {
                let x = k as f32 / n as f32;
                if x <= p[0][0] {
                    return p[0][1];
                }
                if x >= p[p.len() - 1][0] {
                    return p[p.len() - 1][1];
                }
                while p[seg + 1][0] <= x {
                    seg += 1;
                }
                let h = p[seg + 1][0] - p[seg][0];
                let t = (x - p[seg][0]) / h;
                let (t2, t3) = (t * t, t * t * t);
                ((2.0 * t3 - 3.0 * t2 + 1.0) * p[seg][1]
                    + (t3 - 2.0 * t2 + t) * h * m[seg]
                    + (-2.0 * t3 + 3.0 * t2) * p[seg + 1][1]
                    + (t3 - t2) * h * m[seg + 1])
                    .clamp(0.0, 1.0)
            })
            .collect()
    }
}

/// PCHIP tangents: at interior points the weighted harmonic mean of the neighbouring
/// slopes (0 where they differ in sign, so extremes stay flat); at the ends the
/// neighbouring slope.
fn tangents(p: &[[f32; 2]]) -> [f32; MAX_POINTS] {
    let n = p.len();
    let mut m = [0.0f32; MAX_POINTS];
    let h = |i: usize| p[i + 1][0] - p[i][0];
    let d = |i: usize| (p[i + 1][1] - p[i][1]) / h(i);
    m[0] = d(0);
    m[n - 1] = d(n - 2);
    for (i, mi) in m.iter_mut().enumerate().take(n - 1).skip(1) {
        let (d0, d1) = (d(i - 1), d(i));
        *mi = if d0 * d1 <= 0.0 {
            0.0
        } else {
            let (h0, h1) = (h(i - 1), h(i));
            3.0 * (h0 + h1) / ((2.0 * h1 + h0) / d0 + (h1 + 2.0 * h0) / d1)
        };
    }
    m
}

impl Serialize for PointCurve {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.points().serialize(s)
    }
}

impl<'de> Deserialize<'de> for PointCurve {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let points = Vec::<[f32; 2]>::deserialize(d)?;
        Ok(Self::new(&points))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_diagonal_changes_nothing() {
        let c = PointCurve::default();
        assert!(c.is_identity());
        for x in [0.0, 0.1, 0.5, 0.93, 1.0] {
            assert!((c.eval(x) - x).abs() < 1e-6);
        }
        // Extra points on the diagonal keep it straight.
        let c = PointCurve::new(&[[0.0, 0.0], [0.3, 0.3], [0.7, 0.7], [1.0, 1.0]]);
        assert!(c.is_identity());
        assert!((c.eval(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn passes_through_its_points_without_overshoot() {
        let pts = [[0.0, 0.0], [0.25, 0.18], [0.75, 0.84], [1.0, 1.0]];
        let c = PointCurve::new(&pts);
        for p in pts {
            assert!((c.eval(p[0]) - p[1]).abs() < 1e-5);
        }
        // Monotone data give a monotone curve.
        let s = c.sample(1000);
        assert!(s.windows(2).all(|w| w[1] >= w[0] - 1e-6));
        // A peak stays at its point: nothing above it.
        let peak = PointCurve::new(&[[0.0, 0.0], [0.5, 0.8], [1.0, 0.2]]);
        assert!(peak.sample(1000).iter().all(|&v| v <= 0.8 + 1e-6));
    }

    #[test]
    fn is_flat_beyond_its_end_points() {
        // Black point lifted and white point pulled in (a matte, faded end).
        let c = PointCurve::new(&[[0.1, 0.08], [0.9, 0.95]]);
        assert_eq!(c.eval(0.0), 0.08);
        assert_eq!(c.eval(0.05), 0.08);
        assert_eq!(c.eval(1.0), 0.95);
        assert!(!c.is_identity());
    }

    #[test]
    fn cleans_its_points() {
        let c = PointCurve::new(&[
            [0.5, 0.6],
            [f32::NAN, 0.2],
            [1.4, 1.0],
            [0.0, -0.3],
            [0.505, 0.9],
        ]);
        // Sorted, clamped, the point too close to 0.5 dropped.
        assert_eq!(c.points(), &[[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]);
        assert!(PointCurve::new(&[[0.3, 0.3]]).is_identity());
        let many: Vec<[f32; 2]> = (0..30).map(|i| [i as f32 / 29.0, 0.5]).collect();
        assert_eq!(PointCurve::new(&many).points().len(), MAX_POINTS);
        assert_eq!(
            PointCurve::new(&[[0.12345678, 0.5], [1.0, 1.0]]).points()[0][0],
            0.1235
        );
    }

    #[test]
    fn samples_match_evaluation() {
        let c = PointCurve::new(&[[0.0, 0.05], [0.3, 0.2], [0.6, 0.7], [1.0, 0.97]]);
        let s = c.sample(256);
        for (k, v) in s.iter().enumerate() {
            assert!((v - c.eval(k as f32 / 256.0)).abs() < 1e-6, "{k}");
        }
    }

    #[test]
    fn matches_the_ui() {
        // The same case is in pointCurve.test.ts.
        let c = PointCurve::new(&[[0.0, 0.0], [0.25, 0.18], [0.75, 0.84], [1.0, 1.0]]);
        let got = [0.1, 0.5, 0.9].map(|x| c.eval(x));
        let want = [0.067625, 0.514515, 0.940561];
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-5),
            "{got:?}"
        );
    }

    #[test]
    fn serialises_as_pairs() {
        let c = PointCurve::new(&[[0.0, 0.1], [1.0, 0.9]]);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "[[0.0,0.1],[1.0,0.9]]");
        assert_eq!(serde_json::from_str::<PointCurve>(&json).unwrap(), c);
    }
}
