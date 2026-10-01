//! Calibration (ADR 0053): moving the red, green and blue primaries' hue and
//! saturation, and tinting the shadows green or magenta, as Lightroom's Calibration
//! panel does.
//!
//! A colour is a mix of the three primaries, so moving a primary moves every colour
//! that contains it, most of all the colours nearest it: that is what makes these
//! sliders a "look" rather than a targeted fix like the colour mixer. A primary's hue
//! moves it toward a neighbouring primary (red toward green, that is orange, or
//! toward blue, magenta), and its saturation away from or toward grey; each shift
//! keeps the primary's luminance. A colour moves by each primary's shift times how far
//! that channel is above the colour's channel average, so greys (and white) never
//! move and no colour's luminance changes: one 3x3 matrix,
//! `I + sum(shift_i (e_i - 1/3)^T)`. (Measuring from the average rather than from
//! luminance treats the three primaries alike: green is most of luminance, so green
//! colours would hardly count.)
//! It runs on scene values before Contrast and the base look, where a camera
//! profile's matrix would.
//!
//! Shadow Tint tints the darkest tones green or magenta through colour grading's
//! table (ADR 0052), with narrower shadows than grading's default.

use serde::{Deserialize, Serialize};

use image_core::color::REC709_LUMA;

use super::colour_grading::{self, ColourGrading, GradeTable, GradeWheel};

/// How far each primary (red, green, blue) moves toward its neighbour at Hue ±100, as
/// a fraction of the neighbour's chroma: measured so that a mid colour of each turns
/// about 20 degrees (Oklab hue), green a little less, as turning it also dulls it.
const HUE_RANGE: [f32; 3] = [0.25, 0.7, 0.35];
/// How much each primary's chroma changes at Saturation ±100: measured so that a mid
/// colour of each gets about x1.25 and x0.75 the chroma.
const SATURATION_RANGE: [f32; 3] = [0.4, 0.35, 0.45];
/// Shadow Tint ±100 as colour grading's shadow strength.
const SHADOW_TINT_STRENGTH: f32 = 0.6;
/// Colour grading's Blending for Shadow Tint: narrower than its default 50, so the
/// tint stays in the darkest tones.
const SHADOW_TINT_BLENDING: f32 = 25.0;

/// The panel's sliders, each -100..100 with 0 unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Calibration {
    /// Green (negative) or magenta (positive) in the shadows.
    pub shadow_tint: f32,
    /// Positive turns red toward orange; negative toward magenta.
    pub red_hue: f32,
    pub red_saturation: f32,
    /// Positive turns green toward cyan; negative toward yellow.
    pub green_hue: f32,
    pub green_saturation: f32,
    /// Positive turns blue toward purple; negative toward cyan.
    pub blue_hue: f32,
    pub blue_saturation: f32,
}

impl Calibration {
    fn values(&self) -> [f32; 7] {
        [
            self.shadow_tint,
            self.red_hue,
            self.red_saturation,
            self.green_hue,
            self.green_saturation,
            self.blue_hue,
            self.blue_saturation,
        ]
    }

    pub fn is_identity(&self) -> bool {
        self.values().iter().all(|v| *v == 0.0)
    }

    pub fn sanitized(self) -> Self {
        let s = |v: f32| {
            if v.is_finite() {
                v.clamp(-100.0, 100.0)
            } else {
                0.0
            }
        };
        Self {
            shadow_tint: s(self.shadow_tint),
            red_hue: s(self.red_hue),
            red_saturation: s(self.red_saturation),
            green_hue: s(self.green_hue),
            green_saturation: s(self.green_saturation),
            blue_hue: s(self.blue_hue),
            blue_saturation: s(self.blue_saturation),
        }
    }

    fn primaries(&self) -> [(f32, f32); 3] {
        [
            (self.red_hue, self.red_saturation),
            (self.green_hue, self.green_saturation),
            (self.blue_hue, self.blue_saturation),
        ]
    }
}

/// A calibration compiled for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationTable {
    /// Output row by input column: `out[r] = sum(matrix[r][c] * rgb[c])`.
    matrix: [[f32; 3]; 3],
    /// Shadow Tint, when set: colour grading's shadows wheel.
    shadows: Option<GradeTable>,
}

impl CalibrationTable {
    pub fn new(c: &Calibration) -> Self {
        let c = c.sanitized();
        Self {
            matrix: primaries_matrix(&c),
            shadows: (c.shadow_tint != 0.0).then(|| {
                GradeTable::new(&ColourGrading {
                    shadows: GradeWheel {
                        // Colour wheel hues: magenta, or green.
                        hue: if c.shadow_tint > 0.0 { 300.0 } else { 120.0 },
                        saturation: c.shadow_tint.abs() * SHADOW_TINT_STRENGTH,
                        luminance: 0.0,
                    },
                    blending: SHADOW_TINT_BLENDING,
                    ..Default::default()
                })
            }),
        }
    }
}

/// The matrix `I + sum(shift_i (e_i - 1/3)^T)`, where `shift_i` is how far primary
/// `i` moves, with no luminance (`y` being the luminance weights).
fn primaries_matrix(c: &Calibration) -> [[f32; 3]; 3] {
    let y = REC709_LUMA;
    // A primary's chroma: the primary less the grey of its luminance.
    let chroma = |i: usize| {
        let mut v = [-y[i]; 3];
        v[i] += 1.0;
        v
    };
    let mut m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for (i, (hue, saturation)) in c.primaries().into_iter().enumerate() {
        // Positive hue toward the next primary (red to green to blue to red).
        let toward = chroma(if hue >= 0.0 { (i + 1) % 3 } else { (i + 2) % 3 });
        let own = chroma(i);
        let shift: [f32; 3] = std::array::from_fn(|k| {
            hue.abs() / 100.0 * HUE_RANGE[i] * toward[k]
                + saturation / 100.0 * SATURATION_RANGE[i] * own[k]
        });
        for (row, s) in m.iter_mut().zip(shift) {
            for (col, v) in row.iter_mut().enumerate() {
                let e = if col == i { 1.0 } else { 0.0 };
                *v += s * (e - 1.0 / 3.0);
            }
        }
    }
    m
}

/// `rgb` (scene-linear) calibrated.
#[inline]
pub fn apply(rgb: [f32; 3], t: &CalibrationTable) -> [f32; 3] {
    let m = &t.matrix;
    let out = [0, 1, 2].map(|r| (m[r][0] * rgb[0] + m[r][1] * rgb[1] + m[r][2] * rgb[2]).max(0.0));
    match &t.shadows {
        Some(shadows) => colour_grading::apply(out, shadows),
        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ops::colour_grading::oklab;

    fn hue_of(rgb: [f32; 3]) -> f32 {
        let [_, a, b] = oklab(rgb);
        b.atan2(a).to_degrees().rem_euclid(360.0)
    }

    fn chroma_of(rgb: [f32; 3]) -> f32 {
        let [_, a, b] = oklab(rgb);
        a.hypot(b)
    }

    fn table(c: Calibration) -> CalibrationTable {
        CalibrationTable::new(&c)
    }

    #[test]
    fn nothing_set_changes_nothing() {
        let t = table(Calibration::default());
        assert!(Calibration::default().is_identity());
        assert_eq!(apply([0.2, 0.4, 0.6], &t), [0.2, 0.4, 0.6]);
    }

    #[test]
    fn luminance_never_changes() {
        let t = table(Calibration {
            red_hue: 60.0,
            green_saturation: -80.0,
            blue_hue: -100.0,
            blue_saturation: 100.0,
            ..Default::default()
        });
        let [wr, wg, wb] = REC709_LUMA;
        let luma = |p: [f32; 3]| p[0] * wr + p[1] * wg + p[2] * wb;
        for p in [[0.3, 0.5, 0.2], [0.6, 0.35, 0.25], [0.25, 0.35, 0.6]] {
            assert!((luma(apply(p, &t)) - luma(p)).abs() < 1e-5);
        }
    }

    #[test]
    fn greys_stay_grey_whatever_the_primaries_do() {
        let t = table(Calibration {
            red_hue: 80.0,
            red_saturation: -40.0,
            green_hue: -60.0,
            green_saturation: 70.0,
            blue_hue: -100.0,
            blue_saturation: 100.0,
            ..Default::default()
        });
        for v in [0.01, 0.18, 0.9, 3.0] {
            let out = apply([v; 3], &t);
            for c in out {
                assert!((c - v).abs() < 1e-4 * v.max(1.0), "{v} -> {out:?}");
            }
        }
    }

    #[test]
    fn hue_turns_each_primary_the_way_its_slider_says() {
        // Positive: red toward orange, green toward cyan, blue toward purple, which
        // are all increasing hue angles.
        let colours = [[0.6, 0.15, 0.1], [0.15, 0.5, 0.15], [0.1, 0.15, 0.6]];
        let set = |i: usize, v: f32| Calibration {
            red_hue: if i == 0 { v } else { 0.0 },
            green_hue: if i == 1 { v } else { 0.0 },
            blue_hue: if i == 2 { v } else { 0.0 },
            ..Default::default()
        };
        for (i, colour) in colours.into_iter().enumerate() {
            let before = hue_of(colour);
            let turn = |v: f32| {
                let d = hue_of(apply(colour, &table(set(i, v)))) - before;
                (d + 180.0).rem_euclid(360.0) - 180.0
            };
            assert!(turn(100.0) > 5.0, "primary {i}: {}", turn(100.0));
            assert!(turn(-100.0) < -5.0, "primary {i}: {}", turn(-100.0));
        }
    }

    #[test]
    fn saturation_strengthens_its_own_colours_most() {
        let t = table(Calibration {
            blue_saturation: 100.0,
            ..Default::default()
        });
        let (blue, red) = ([0.1, 0.15, 0.6], [0.6, 0.15, 0.1]);
        let gain = |c: [f32; 3]| chroma_of(apply(c, &t)) / chroma_of(c);
        assert!(gain(blue) > 1.2, "{}", gain(blue));
        assert!(gain(blue) > gain(red));
        let less = table(Calibration {
            blue_saturation: -100.0,
            ..Default::default()
        });
        assert!(chroma_of(apply(blue, &less)) < chroma_of(blue) * 0.8);
    }

    #[test]
    fn shadow_tint_tints_the_shadows_and_spares_the_highlights() {
        let magenta = table(Calibration {
            shadow_tint: 100.0,
            ..Default::default()
        });
        let green = table(Calibration {
            shadow_tint: -100.0,
            ..Default::default()
        });
        let dark = [0.01; 3];
        // Oklab a: positive is magenta-red, negative green.
        assert!(oklab(apply(dark, &magenta))[1] > 0.01);
        assert!(oklab(apply(dark, &green))[1] < -0.01);
        let light = [0.8; 3];
        assert!(chroma_of(apply(light, &magenta)) < 0.002);
    }

    #[test]
    fn sanitising_keeps_it_in_range() {
        let c = Calibration {
            red_hue: 400.0,
            shadow_tint: f32::NAN,
            blue_saturation: -150.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (c.red_hue, c.shadow_tint, c.blue_saturation),
            (100.0, 0.0, -100.0)
        );
    }
}
