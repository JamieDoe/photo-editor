//! Colour grading (ADR 0052): tinting the shadows, midtones and highlights, and the
//! whole picture, each with a hue, a strength and a brightness, as Lightroom's Color
//! Grading (and its older Split Toning) does.
//!
//! The tints are worked out in Oklab, so a tint's strength looks alike at any hue.
//! Lightness gives weights for the three ranges: shadows `(1 - x)^k`, highlights
//! `x^k`, midtones the rest, where `x` is the lightness bent so that the point where
//! shadows give way to highlights (middle grey, moved by Balance) lands at 0.5, and
//! Blending sets `k` (how much the ranges overlap). At each lightness, the wheels' tints add to a grey's colour (a, b)
//! and their brightnesses to its lightness, by weight; the global wheel applies
//! everywhere.
//!
//! What grading does to a grey of each lightness is tabulated, and applied to a pixel
//! by its lightness: the brightness change as one gain for all three channels (capped,
//! so near-black noise is never amplified much), and the rest, the tint, added. That
//! is exact for greys (toned black and white) and gives colours the same tint while
//! keeping their own colour, without converting each pixel to Oklab and back. It runs
//! after Saturation, so black and white photos can be toned.

use serde::{Deserialize, Serialize};

/// The most a wheel at full strength moves a colour (Oklab chroma).
const MAX_CHROMA: f32 = 0.10;
/// The most a brightness slider at ±100 moves lightness (Oklab L).
const MAX_LIGHTNESS: f32 = 0.12;
/// Where shadows give way to highlights at Balance 0: middle grey (18% luminance), so
/// a photo's typical tones are midtones rather than mostly highlights.
const MIDDLE_GREY_L: f32 = 0.5646;
/// The weights' exponent at Blending 50: steep enough that a range's tint stays mostly
/// in its own tones (a tone halfway from middle grey to white takes about a fifth of
/// the highlights' tint), so a split tone reads as one, not as an overall cast.
const DEFAULT_K: f32 = 3.0;

/// One wheel: a hue (degrees on the colour wheel: 0 red, 120 green, 240 blue), how
/// strongly to tint toward it (0..100), and a brightness change (-100..100).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GradeWheel {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

impl GradeWheel {
    pub fn is_identity(&self) -> bool {
        self.saturation == 0.0 && self.luminance == 0.0
    }

    fn sanitized(self) -> Self {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        Self {
            hue: finite(self.hue).rem_euclid(360.0),
            saturation: finite(self.saturation).clamp(0.0, 100.0),
            luminance: finite(self.luminance).clamp(-100.0, 100.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ColourGrading {
    pub shadows: GradeWheel,
    pub midtones: GradeWheel,
    pub highlights: GradeWheel,
    pub global: GradeWheel,
    /// How much the ranges overlap, 0..100 (50 as Lightroom's default).
    pub blending: f32,
    /// Where shadows give way to highlights, -100..100: positive gives highlights more
    /// of the range.
    pub balance: f32,
}

impl Default for ColourGrading {
    fn default() -> Self {
        Self {
            shadows: GradeWheel::default(),
            midtones: GradeWheel::default(),
            highlights: GradeWheel::default(),
            global: GradeWheel::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

impl ColourGrading {
    pub fn is_identity(&self) -> bool {
        self.wheels().iter().all(GradeWheel::is_identity)
    }

    fn wheels(&self) -> [GradeWheel; 4] {
        [self.shadows, self.midtones, self.highlights, self.global]
    }

    pub fn sanitized(self) -> Self {
        let finite = |v: f32, or: f32| if v.is_finite() { v } else { or };
        Self {
            shadows: self.shadows.sanitized(),
            midtones: self.midtones.sanitized(),
            highlights: self.highlights.sanitized(),
            global: self.global.sanitized(),
            blending: finite(self.blending, 50.0).clamp(0.0, 100.0),
            balance: finite(self.balance, 0.0).clamp(-100.0, 100.0),
        }
    }
}

/// Steps of the shift table over lightness 0..1.
const STEPS: usize = 1024;

/// A grading compiled for rendering: each wheel's colour and lightness shifts, the
/// weights' shape, and (since a pixel's shift depends only on its lightness) the total
/// shift tabulated over lightness, so a pixel needs no powers.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeTable {
    /// Oklab (a, b) shifts of the shadows, midtones, highlights and global wheels.
    chroma: [[f32; 2]; 4],
    lightness: [f32; 4],
    /// Lightness is bent by `x = L^bend` so the balance point lands at 0.5.
    bend: f32,
    /// The weights' exponent (from Blending).
    k: f32,
    /// At `STEPS + 1` lightnesses from 0 to 1: the brightness gain, then the tint added
    /// in each channel, taking a grey to its graded colour.
    gains: Vec<[f32; 4]>,
}

impl GradeTable {
    pub fn new(g: &ColourGrading) -> Self {
        let g = g.sanitized();
        let wheels = g.wheels();
        // Positive balance gives highlights more: the point between moves down.
        let pivot = MIDDLE_GREY_L - g.balance / 100.0 * 0.25;
        let mut t = Self {
            chroma: wheels.map(|w| {
                let d = hue_direction(w.hue);
                let c = w.saturation / 100.0 * MAX_CHROMA;
                [d[0] * c, d[1] * c]
            }),
            lightness: wheels.map(|w| w.luminance / 100.0 * MAX_LIGHTNESS),
            bend: 0.5f32.ln() / pivot.ln(),
            k: DEFAULT_K * 2f32.powf((50.0 - g.blending) / 50.0),
            gains: Vec::new(),
        };
        t.gains = (0..=STEPS)
            .map(|i| t.grey_gains(i as f32 / STEPS as f32))
            .collect();
        t
    }

    /// The gain and tint that grade a grey of Oklab lightness `l`: the brightness gain
    /// (at most `MAX_GAIN`), then what remains to reach the graded colour, added.
    fn grey_gains(&self, l: f32) -> [f32; 4] {
        const MAX_GAIN: f32 = 4.0;
        let [dl, da, db] = self.shift(l);
        let grey = l * l * l;
        let brighter = (l + dl).max(0.0).powi(3);
        let gain = if grey > 0.0 {
            (brighter / grey).min(MAX_GAIN)
        } else {
            1.0
        };
        let graded = linear_srgb([(l + dl).max(0.0), da, db]).map(|c| c.max(0.0));
        [
            gain,
            graded[0] - gain * grey,
            graded[1] - gain * grey,
            graded[2] - gain * grey,
        ]
    }

    /// The total (L, a, b) shift at lightness `l`, from the wheels and their weights.
    fn shift(&self, l: f32) -> [f32; 3] {
        let w = self.weights(l);
        let weights = [w[0], w[1], w[2], 1.0];
        let mut s = [0.0f32; 3];
        for ((w, l), c) in weights.iter().zip(self.lightness).zip(self.chroma) {
            s[0] += w * l;
            s[1] += w * c[0];
            s[2] += w * c[1];
        }
        s
    }

    /// The tabulated gains at lightness `l` (interpolated).
    #[inline]
    fn gains_at(&self, l: f32) -> [f32; 4] {
        let x = l.clamp(0.0, 1.0) * STEPS as f32;
        let i = (x as usize).min(STEPS - 1);
        let f = x - i as f32;
        let (p, q) = (self.gains[i], self.gains[i + 1]);
        [
            p[0] + (q[0] - p[0]) * f,
            p[1] + (q[1] - p[1]) * f,
            p[2] + (q[2] - p[2]) * f,
            p[3] + (q[3] - p[3]) * f,
        ]
    }

    /// The shadows', midtones' and highlights' weights at Oklab lightness `l`.
    fn weights(&self, l: f32) -> [f32; 3] {
        let x = l.clamp(0.0, 1.0).powf(self.bend);
        let shadows = (1.0 - x).powf(self.k);
        let highlights = x.powf(self.k);
        [shadows, (1.0 - shadows - highlights).max(0.0), highlights]
    }
}

/// `rgb` (linear sRGB) graded by its lightness (a grey's Oklab lightness is the cube
/// root of its luminance): brightened by the gain, then tinted.
#[inline]
pub fn apply(rgb: [f32; 3], t: &GradeTable) -> [f32; 3] {
    let [wr, wg, wb] = image_core::color::REC709_LUMA;
    let l = cbrt(rgb[0] * wr + rgb[1] * wg + rgb[2] * wb);
    let [gain, tr, tg, tb] = t.gains_at(l);
    [
        (rgb[0] * gain + tr).max(0.0),
        (rgb[1] * gain + tg).max(0.0),
        (rgb[2] * gain + tb).max(0.0),
    ]
}

/// The unit Oklab (a, b) direction of a colour-wheel hue: that of the fully saturated
/// colour at the hue (HSL hue, saturation 1, lightness 0.5).
fn hue_direction(hue: f32) -> [f32; 2] {
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let rgb = match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    // sRGB-encoded to linear.
    let [_, a, b] = oklab(rgb.map(image_core::color::srgb_to_linear));
    let n = a.hypot(b).max(1e-6);
    [a / n, b / n]
}

/// Cube root of `x >= 0`: a first guess from the float's bits, then two Halley steps,
/// as accurate as `f32::cbrt` here and several times cheaper (it runs three times a
/// pixel).
#[inline]
fn cbrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = f32::from_bits(x.to_bits() / 3 + 0x2a51_4067);
    for _ in 0..2 {
        let y3 = y * y * y;
        y *= (y3 + 2.0 * x) / (2.0 * y3 + x);
    }
    y
}

/// Linear sRGB to Oklab (Björn Ottosson's published matrices).
fn oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = cbrt(0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b);
    let m = cbrt(0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b);
    let s = cbrt(0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b);
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Oklab to linear sRGB.
fn linear_srgb([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_93 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_4 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(hue: f32, saturation: f32, luminance: f32) -> GradeWheel {
        GradeWheel {
            hue,
            saturation,
            luminance,
        }
    }

    /// The (a, b) direction `rgb` was moved in by grading, as an angle.
    fn tint_angle(before: [f32; 3], after: [f32; 3]) -> f32 {
        let (p, q) = (oklab(before), oklab(after));
        (q[2] - p[2]).atan2(q[1] - p[1]).to_degrees()
    }

    #[test]
    fn oklab_round_trips() {
        for rgb in [
            [0.2, 0.4, 0.6],
            [1.0, 0.0, 0.0],
            [0.05, 0.05, 0.05],
            [2.0, 1.5, 0.3],
        ] {
            let back = linear_srgb(oklab(rgb));
            for (a, b) in rgb.iter().zip(back) {
                assert!((a - b).abs() < 1e-4, "{rgb:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn nothing_set_changes_nothing() {
        let t = GradeTable::new(&ColourGrading::default());
        assert!(ColourGrading::default().is_identity());
        let out = apply([0.2, 0.4, 0.6], &t);
        for (a, b) in [0.2, 0.4, 0.6].iter().zip(out) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn shadows_take_the_shadow_tint_and_highlights_the_highlight_tint() {
        // Teal shadows, orange highlights: the classic.
        let g = ColourGrading {
            shadows: wheel(190.0, 60.0, 0.0),
            highlights: wheel(30.0, 60.0, 0.0),
            ..Default::default()
        };
        let t = GradeTable::new(&g);
        let (dark, light) = ([0.01; 3], [0.7; 3]);
        let teal = hue_direction(190.0);
        let orange = hue_direction(30.0);
        let angle = |d: [f32; 2]| d[1].atan2(d[0]).to_degrees();
        assert!((tint_angle(dark, apply(dark, &t)) - angle(teal)).abs() < 10.0);
        assert!((tint_angle(light, apply(light, &t)) - angle(orange)).abs() < 10.0);
        // Each range's grey moves most toward its own wheel.
        let chroma = |rgb: [f32; 3]| {
            let p = oklab(rgb);
            p[1].hypot(p[2])
        };
        assert!(chroma(apply(dark, &t)) > 0.02);
    }

    #[test]
    fn brightness_and_global() {
        let g = ColourGrading {
            global: wheel(0.0, 0.0, 50.0),
            ..Default::default()
        };
        let t = GradeTable::new(&g);
        let grey = [0.18; 3];
        assert!(oklab(apply(grey, &t))[0] > oklab(grey)[0] + 0.05);
        // Brightness alone keeps a grey grey.
        let out = apply(grey, &t);
        assert!((out[0] - out[1]).abs() < 1e-4 && (out[1] - out[2]).abs() < 1e-4);
    }

    #[test]
    fn balance_and_blending_shape_the_ranges() {
        let base = GradeTable::new(&ColourGrading::default());
        let mid = MIDDLE_GREY_L;
        let w = base.weights(mid);
        assert!((w[0] - w[2]).abs() < 1e-5, "even at the middle: {w:?}");
        // Positive balance: the middle already counts as highlights.
        let toward_highlights = GradeTable::new(&ColourGrading {
            balance: 60.0,
            ..Default::default()
        });
        let w = toward_highlights.weights(mid);
        assert!(w[2] > w[0]);
        // Less blending: narrower shadows and highlights, more midtones.
        let sharp = GradeTable::new(&ColourGrading {
            blending: 0.0,
            ..Default::default()
        });
        assert!(sharp.weights(0.3)[1] > base.weights(0.3)[1]);
        // Weights stay a share of one.
        for l in [0.0, 0.2, 0.5, 0.9, 1.0] {
            let w = sharp.weights(l);
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn ranges_keep_to_their_own_tones() {
        let t = GradeTable::new(&ColourGrading::default());
        let at = |luminance: f32| t.weights(luminance.cbrt());
        // Middle grey is mostly midtones.
        assert!(at(0.18)[1] > 0.7, "{:?}", at(0.18));
        // A mid-bright tone takes only a little of the highlights' tint...
        assert!(at(0.3)[2] < 0.25, "{:?}", at(0.3));
        // ...and near-white and near-black ones most of their range's.
        assert!(at(0.8)[2] > 0.6, "{:?}", at(0.8));
        assert!(at(0.01)[0] > 0.6, "{:?}", at(0.01));
    }

    #[test]
    fn sanitising_keeps_it_in_range() {
        let g = ColourGrading {
            shadows: wheel(-30.0, 150.0, -300.0),
            blending: f32::NAN,
            balance: 500.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(g.shadows, wheel(330.0, 100.0, -100.0));
        assert_eq!((g.blending, g.balance), (50.0, 100.0));
    }

    #[test]
    fn greys_are_graded_exactly_as_in_oklab() {
        let t = GradeTable::new(&ColourGrading {
            shadows: wheel(190.0, 60.0, -20.0),
            highlights: wheel(30.0, 40.0, 10.0),
            blending: 20.0,
            balance: -30.0,
            ..Default::default()
        });
        for v in [0.01f32, 0.05, 0.18, 0.4, 0.8] {
            let l = cbrt(v);
            let [dl, da, db] = t.shift(l);
            let want = linear_srgb([l + dl, da, db]).map(|c| c.max(0.0));
            let got = apply([v; 3], &t);
            for (a, b) in want.iter().zip(got) {
                assert!(
                    (a - b).abs() < 2e-3 * a.max(0.01),
                    "{v}: {want:?} vs {got:?}"
                );
            }
        }
    }

    #[test]
    fn saturated_dark_colours_keep_their_colour() {
        // Teal shadows at full strength on a deep red: it takes the teal a grey of its
        // lightness takes, but stays red and keeps most of its red; nothing runs away
        // (as dividing by a near-black grey would).
        let t = GradeTable::new(&ColourGrading {
            shadows: wheel(190.0, 100.0, 0.0),
            ..Default::default()
        });
        let red = [0.02, 0.0, 0.0];
        let out = apply(red, &t);
        assert!(out[0] > 0.75 * red[0], "{out:?}");
        assert!(out[0] > out[1] && out[0] > out[2], "{out:?}");
        assert!(out.iter().all(|&c| c < 0.05), "{out:?}");
    }

    #[test]
    fn the_table_matches_the_weights() {
        let t = GradeTable::new(&ColourGrading {
            shadows: wheel(190.0, 60.0, -20.0),
            highlights: wheel(30.0, 40.0, 10.0),
            blending: 20.0,
            balance: -30.0,
            ..Default::default()
        });
        // What counts is the graded grey: the table and the direct gains agree on it.
        for i in 0..=100 {
            let l = i as f32 / 100.0;
            let grey = l * l * l;
            let graded = |g: [f32; 4]| [grey * g[0] + g[1], grey * g[0] + g[2], grey * g[0] + g[3]];
            let (a, b) = (graded(t.grey_gains(l)), graded(t.gains_at(l)));
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 1e-4, "at {l}: {a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn the_fast_cube_root_is_exact_enough() {
        let mut worst = 0.0f32;
        let mut x = 1e-6f32;
        while x < 64.0 {
            worst = worst.max(((cbrt(x) - x.cbrt()) / x.cbrt()).abs());
            x *= 1.003;
        }
        assert!(worst < 2e-6, "relative error {worst}");
        assert_eq!((cbrt(0.0), cbrt(-1.0)), (0.0, 0.0));
    }
}
