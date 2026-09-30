//! Relative white balance: Temperature and Tint (ADR 0024).
//!
//! The decoder has already balanced the image for the *as-shot* light. The sliders
//! say the light was different: Temperature moves it along the Planckian locus (in
//! mired, so a step feels alike at any colour temperature) and Tint moves it across
//! the locus (Duv, green–magenta). The image is re-balanced from the as-shot light to
//! that light: gains `as_shot / assumed` in linear sRGB, normalised so neutrals keep
//! their luminance.
//!
//! Simplification (unchanged since Phase 0): gains are applied in linear sRGB after
//! the camera matrix, not in camera space. See docs/RENDERING.md.

use image_core::Chromaticity;
use image_core::color::REC709_LUMA;

/// Mired shift of the assumed light at Temperature ±100 (positive = warmer image).
pub const MIRED_PER_UNIT: f32 = 1.2;
/// Duv shift at Tint ±100 (positive = more magenta image), as in Adobe's scale.
pub const DUV_PER_UNIT: f32 = 1.0 / 3000.0;
pub const MIN_KELVIN: f32 = 1667.0;
pub const MAX_KELVIN: f32 = 25_000.0;
const MAX_DUV: f32 = 0.05;

/// A light's colour: correlated colour temperature and distance from the Planckian
/// locus (positive = greener than a blackbody).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WhitePoint {
    pub kelvin: f32,
    pub duv: f32,
}

impl WhitePoint {
    /// The white point of a chromaticity (nearest point on the Planckian locus in
    /// CIE 1960 uv).
    pub fn from_chromaticity(c: Chromaticity) -> Self {
        let p = uv_of(f64::from(c.x), f64::from(c.y));
        // Distance to the locus is unimodal in mired over this range: golden section.
        let dist = |m: f64| {
            let q = locus_uv(1.0e6 / m);
            (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)
        };
        let (mut lo, mut hi) = (1.0e6 / f64::from(MAX_KELVIN), 1.0e6 / f64::from(MIN_KELVIN));
        let g = (5f64.sqrt() - 1.0) / 2.0;
        let (mut a, mut b) = (hi - g * (hi - lo), lo + g * (hi - lo));
        let (mut fa, mut fb) = (dist(a), dist(b));
        for _ in 0..60 {
            if fa < fb {
                hi = b;
                (b, fb) = (a, fa);
                a = hi - g * (hi - lo);
                fa = dist(a);
            } else {
                lo = a;
                (a, fa) = (b, fb);
                b = lo + g * (hi - lo);
                fb = dist(b);
            }
        }
        let kelvin = 1.0e6 / ((lo + hi) / 2.0);
        let q = locus_uv(kelvin);
        let n = locus_normal(kelvin);
        let duv = (p.0 - q.0) * n.0 + (p.1 - q.1) * n.1;
        Self {
            kelvin: kelvin as f32,
            duv: duv as f32,
        }
    }

    pub fn chromaticity(self) -> Chromaticity {
        let t = f64::from(self.kelvin.clamp(MIN_KELVIN, MAX_KELVIN));
        let duv = f64::from(self.duv.clamp(-MAX_DUV, MAX_DUV));
        let q = locus_uv(t);
        let n = locus_normal(t);
        let (u, v) = (q.0 + duv * n.0, q.1 + duv * n.1);
        // CIE 1960 uv -> xy.
        let d = 2.0 * u - 8.0 * v + 4.0;
        Chromaticity {
            x: (3.0 * u / d) as f32,
            y: (2.0 * v / d) as f32,
        }
    }

    /// The light the sliders assume, from the as-shot light.
    pub fn adjusted(self, temperature: f32, tint: f32) -> Self {
        let mired = 1.0e6 / self.kelvin - temperature * MIRED_PER_UNIT;
        Self {
            kelvin: (1.0e6 / mired.max(1.0)).clamp(MIN_KELVIN, MAX_KELVIN),
            duv: (self.duv + tint * DUV_PER_UNIT).clamp(-MAX_DUV, MAX_DUV),
        }
    }
}

/// White balance set as the light itself (ADR 0051): a colour temperature and a tint on
/// Adobe's scale (the light's Duv x 3000; positive is greener light, so a more magenta
/// photo). Lightroom presets made on raw files set white balance this way; each photo
/// is balanced from its own as-shot light to this one.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AbsoluteWhiteBalance {
    pub kelvin: f32,
    pub tint: f32,
}

impl AbsoluteWhiteBalance {
    /// Tint's range, as Lightroom's.
    pub const TINT_RANGE: f32 = 150.0;

    pub fn sanitized(self) -> Self {
        let finite = |v: f32, or: f32| if v.is_finite() { v } else { or };
        Self {
            kelvin: finite(self.kelvin, 5500.0).clamp(MIN_KELVIN, MAX_KELVIN),
            tint: finite(self.tint, 0.0).clamp(-Self::TINT_RANGE, Self::TINT_RANGE),
        }
    }

    /// The light it describes.
    pub fn white_point(self) -> WhitePoint {
        WhitePoint {
            kelvin: self.kelvin,
            duv: self.tint * DUV_PER_UNIT,
        }
    }
}

/// How to show Temperature in kelvin for a photo whose as-shot light is known: the
/// light the slider assumes is `1e6 / (1e6 / as_shot_kelvin - amount * mired_per_unit)`,
/// clamped to `min_kelvin..=max_kelvin` (see [`TemperatureScale::kelvin_at`]).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TemperatureScale {
    pub as_shot_kelvin: f32,
    /// The as-shot light's tint on Adobe's scale (its Duv x 3000), to show a white
    /// balance set as the light (ADR 0051) on the relative sliders.
    pub as_shot_tint: f32,
    pub mired_per_unit: f32,
    pub min_kelvin: f32,
    pub max_kelvin: f32,
}

impl TemperatureScale {
    /// `None` without an as-shot light (display-referred images): the slider then
    /// shows its relative value.
    pub fn for_source(as_shot: Option<Chromaticity>) -> Option<Self> {
        as_shot.map(|c| Self {
            as_shot_kelvin: WhitePoint::from_chromaticity(c).kelvin,
            as_shot_tint: WhitePoint::from_chromaticity(c).duv / DUV_PER_UNIT,
            mired_per_unit: MIRED_PER_UNIT,
            min_kelvin: MIN_KELVIN,
            max_kelvin: MAX_KELVIN,
        })
    }

    /// The colour temperature the slider value `amount` assumes.
    pub fn kelvin_at(&self, amount: f32) -> f32 {
        WhitePoint {
            kelvin: self.as_shot_kelvin,
            duv: 0.0,
        }
        .adjusted(amount, 0.0)
        .kelvin
    }
}

/// Channel gains for Temperature and Tint (each -100..100) relative to the as-shot
/// light (`None`: the image is already display-referred, so its white is D65).
pub fn gains(as_shot: Option<Chromaticity>, temperature: f32, tint: f32) -> [f32; 3] {
    let shot = WhitePoint::from_chromaticity(as_shot.unwrap_or(Chromaticity::D65));
    gains_between(shot, shot.adjusted(temperature, tint))
}

/// Channel gains balancing the photo for `light` instead of its as-shot light (ADR
/// 0051).
pub fn gains_for(as_shot: Option<Chromaticity>, light: AbsoluteWhiteBalance) -> [f32; 3] {
    let shot = WhitePoint::from_chromaticity(as_shot.unwrap_or(Chromaticity::D65));
    gains_between(shot, light.sanitized().white_point())
}

fn gains_between(shot: WhitePoint, assumed: WhitePoint) -> [f32; 3] {
    // Both lights go through the same conversion, so the same light gives exactly 1.
    let from = shot.chromaticity().to_linear_srgb();
    let to = assumed.chromaticity().to_linear_srgb();
    let mut g = [from[0] / to[0], from[1] / to[1], from[2] / to[2]];
    let luma: f32 = g.iter().zip(REC709_LUMA).map(|(g, w)| g * w).sum();
    for c in &mut g {
        *c /= luma;
    }
    g
}

/// CIE 1960 uv of a CIE 1931 xy chromaticity.
fn uv_of(x: f64, y: f64) -> (f64, f64) {
    let d = -2.0 * x + 12.0 * y + 3.0;
    (4.0 * x / d, 6.0 * y / d)
}

/// The Planckian locus in CIE 1960 uv (Kim et al. cubic approximation, 1667..25000 K).
fn locus_uv(kelvin: f64) -> (f64, f64) {
    let t = kelvin.clamp(f64::from(MIN_KELVIN), f64::from(MAX_KELVIN));
    let (t2, t3) = (t * t, t * t * t);
    let x = if t <= 4000.0 {
        -0.266_123_9e9 / t3 - 0.234_358_9e6 / t2 + 0.877_695_6e3 / t + 0.179_910
    } else {
        -3.025_846_9e9 / t3 + 2.107_037_9e6 / t2 + 0.222_634_7e3 / t + 0.240_390
    };
    let (x2, x3) = (x * x, x * x * x);
    let y = if t <= 2222.0 {
        -1.106_381_4 * x3 - 1.348_110_20 * x2 + 2.185_558_32 * x - 0.202_196_83
    } else if t <= 4000.0 {
        -0.954_947_6 * x3 - 1.374_185_93 * x2 + 2.091_370_15 * x - 0.167_488_67
    } else {
        3.081_758_0 * x3 - 5.873_386_70 * x2 + 3.751_129_97 * x - 0.370_014_83
    };
    uv_of(x, y)
}

/// Unit normal to the locus at `kelvin`, pointing to the green side (+v).
fn locus_normal(kelvin: f64) -> (f64, f64) {
    let m = 1.0e6 / kelvin;
    let a = locus_uv(1.0e6 / (m + 0.5));
    let b = locus_uv(1.0e6 / (m - 0.5));
    let (du, dv) = (b.0 - a.0, b.1 - a.1);
    let len = du.hypot(dv);
    let n = (-dv / len, du / len);
    if n.1 < 0.0 { (-n.0, -n.1) } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(g: [f32; 3]) -> f32 {
        g.iter().zip(REC709_LUMA).map(|(g, w)| g * w).sum()
    }

    #[test]
    fn zero_is_identity() {
        for shot in [None, Some(Chromaticity { x: 0.44, y: 0.40 })] {
            for g in gains(shot, 0.0, 0.0) {
                assert!((g - 1.0).abs() < 1e-5, "{g}");
            }
        }
    }

    #[test]
    fn d65_is_about_6500_k_and_slightly_green_of_the_locus() {
        let w = WhitePoint::from_chromaticity(Chromaticity::D65);
        assert!((w.kelvin - 6504.0).abs() < 25.0, "{w:?}");
        // D65's published Duv is +0.0032.
        assert!((w.duv - 0.0032).abs() < 0.0006, "{w:?}");
        // Illuminant A (tungsten): 2856 K on the locus.
        let a = WhitePoint::from_chromaticity(Chromaticity {
            x: 0.447_58,
            y: 0.407_45,
        });
        assert!(
            (a.kelvin - 2856.0).abs() < 15.0 && a.duv.abs() < 0.0005,
            "{a:?}"
        );
    }

    #[test]
    fn white_point_round_trips() {
        for (kelvin, duv) in [
            (2200.0, 0.0),
            (3200.0, -0.01),
            (5000.0, 0.012),
            (9000.0, 0.0),
        ] {
            let w = WhitePoint { kelvin, duv };
            let back = WhitePoint::from_chromaticity(w.chromaticity());
            assert!(
                (back.kelvin / kelvin - 1.0).abs() < 2e-3,
                "{w:?} -> {back:?}"
            );
            assert!((back.duv - duv).abs() < 2e-4, "{w:?} -> {back:?}");
        }
    }

    #[test]
    fn temperature_warms_and_cools() {
        let warm = gains(None, 50.0, 0.0);
        assert!(warm[0] > 1.0 && warm[2] < 1.0, "{warm:?}");
        let cool = gains(None, -50.0, 0.0);
        assert!(cool[0] < 1.0 && cool[2] > 1.0, "{cool:?}");
    }

    #[test]
    fn tint_moves_between_green_and_magenta() {
        let magenta = gains(None, 0.0, 50.0);
        assert!(
            magenta[1] < magenta[0] && magenta[1] < magenta[2],
            "{magenta:?}"
        );
        let green = gains(None, 0.0, -50.0);
        assert!(green[1] > green[0] && green[1] > green[2], "{green:?}");
    }

    #[test]
    fn preserves_neutral_luminance() {
        for (t, n) in [(-100.0, 0.0), (40.0, 30.0), (100.0, -100.0), (0.0, 100.0)] {
            assert!((luma(gains(None, t, n)) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn gains_are_monotonic_in_temperature() {
        let shot = Some(Chromaticity { x: 0.40, y: 0.39 });
        let mut last = gains(shot, -100.0, 0.0);
        for i in -99..=100 {
            let g = gains(shot, i as f32, 0.0);
            assert!(g[0] >= last[0] && g[2] <= last[2], "at {i}");
            last = g;
        }
    }

    #[test]
    fn temperature_scale_matches_the_rendered_light() {
        assert_eq!(TemperatureScale::for_source(None), None);
        let scale = TemperatureScale::for_source(Some(Chromaticity::D65)).unwrap();
        assert!((scale.as_shot_kelvin - 6504.0).abs() < 25.0);
        // The UI computes this itself from the scale (apps/desktop/src/whiteBalance.ts,
        // whose test uses the same numbers): 5000 K at +50 is 1e6 / (200 - 60).
        let s = TemperatureScale {
            as_shot_kelvin: 5000.0,
            ..scale
        };
        assert!((s.kelvin_at(50.0) - 7142.857).abs() < 0.01);
        assert!((s.kelvin_at(-100.0) - 3125.0).abs() < 0.01);
        assert_eq!(s.kelvin_at(0.0), 5000.0);
    }

    #[test]
    fn a_step_is_the_same_mired_shift_under_any_light() {
        // Relative sliders: the same edit means the same visual shift on a tungsten
        // shot and a daylight shot, which is what makes presets portable.
        let daylight = WhitePoint::from_chromaticity(Chromaticity::D65);
        let tungsten = WhitePoint {
            kelvin: 3000.0,
            duv: 0.0,
        };
        for w in [daylight, tungsten] {
            let a = w.adjusted(25.0, 0.0);
            let shift = 1.0e6 / w.kelvin - 1.0e6 / a.kelvin;
            assert!((shift - 30.0).abs() < 0.01, "{shift}");
        }
    }

    #[test]
    fn a_light_set_in_kelvin_matches_the_same_shift_on_the_sliders() {
        let shot = Chromaticity { x: 0.44, y: 0.40 };
        let scale = TemperatureScale::for_source(Some(shot)).unwrap();
        // The as-shot light itself: no change.
        let same = AbsoluteWhiteBalance {
            kelvin: scale.as_shot_kelvin,
            tint: scale.as_shot_tint,
        };
        for g in gains_for(Some(shot), same) {
            assert!((g - 1.0).abs() < 1e-3, "{g}");
        }
        // 500 K warmer than as shot, in mired: the same as the equivalent slider.
        let kelvin = scale.as_shot_kelvin + 500.0;
        let temperature = (1.0e6 / scale.as_shot_kelvin - 1.0e6 / kelvin) / MIRED_PER_UNIT;
        let tint = 12.0;
        let set = gains_for(
            Some(shot),
            AbsoluteWhiteBalance {
                kelvin,
                tint: scale.as_shot_tint + tint,
            },
        );
        let slid = gains(Some(shot), temperature, tint);
        for (a, b) in set.iter().zip(slid) {
            assert!((a - b).abs() < 1e-3, "{set:?} vs {slid:?}");
        }
    }
}
