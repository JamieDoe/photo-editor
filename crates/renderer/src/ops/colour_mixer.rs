//! Colour mixer (ADR 0025): hue, saturation and luminance for eight colour bands.
//!
//! Works on display-referred values (after the base look), in a square-root domain
//! that is close to how colours look (gamma ~2):
//!
//! - A pixel's hue picks its bands: each band has a centre hue, and between two
//!   centres the adjustments blend smoothly, so no colour sits on an edge.
//! - **Hue** rotates the colour around the grey axis, up to 30° at ±100 (positive
//!   moves red towards orange, blue towards purple).
//! - **Saturation** scales the colour's chroma: 0× at -100, 2× at +100.
//! - Hue and saturation keep the pixel's luminance; **Luminance** is a gain of up to
//!   ±1 stop.
//! - Near-neutral pixels, whose hue is noise, fade out of the effect.
//!
//! Everything that depends only on hue is tabulated per degree by [`MixerTable`].

use serde::{Deserialize, Serialize};

use image_core::color::REC709_LUMA;

/// Hue, saturation and luminance shifts for one band, each -100..100.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HslShift {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

impl HslShift {
    pub fn is_identity(&self) -> bool {
        self.hue == 0.0 && self.saturation == 0.0 && self.luminance == 0.0
    }
}

/// The eight bands, as in the design's colour mixer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ColourMixer {
    pub red: HslShift,
    pub orange: HslShift,
    pub yellow: HslShift,
    pub green: HslShift,
    pub aqua: HslShift,
    pub blue: HslShift,
    pub purple: HslShift,
    pub magenta: HslShift,
}

impl ColourMixer {
    /// In band order (see [`BAND_HUES`]).
    pub fn bands(&self) -> [HslShift; 8] {
        [
            self.red,
            self.orange,
            self.yellow,
            self.green,
            self.aqua,
            self.blue,
            self.purple,
            self.magenta,
        ]
    }

    pub fn bands_mut(&mut self) -> [&mut HslShift; 8] {
        [
            &mut self.red,
            &mut self.orange,
            &mut self.yellow,
            &mut self.green,
            &mut self.aqua,
            &mut self.blue,
            &mut self.purple,
            &mut self.magenta,
        ]
    }

    pub fn is_identity(&self) -> bool {
        self.bands().iter().all(HslShift::is_identity)
    }
}

/// Centre hue of each band in degrees (HSV hue of square-rooted values: 0 red,
/// 60 yellow, 120 green, 180 aqua, 240 blue, 300 magenta). Blues is centred at 225°,
/// between a typical sky (~215°) and pure blue, so a sky responds almost fully to
/// Blues rather than being split with Aquas.
pub const BAND_HUES: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 225.0, 270.0, 300.0];

const MAX_HUE_DEGREES: f32 = 30.0;
const MAX_LUMINANCE_STOPS: f32 = 1.0;
/// Saturation (of square-rooted values) below which the mixer fades out.
const NEUTRAL_FADE: (f32, f32) = (0.04, 0.2);

/// Per-degree table of what the mixer does to a hue: rotation (cos, sin), chroma
/// scale and luminance gain.
#[derive(Debug, Clone, PartialEq)]
pub struct MixerTable {
    entries: Vec<[f32; 4]>,
    /// Per degree: whether the mixer changes hues in `[deg, deg + 1)`, so pixels of
    /// untouched colours skip the work.
    active: Vec<bool>,
}

impl MixerTable {
    pub fn new(bands: &[HslShift; 8]) -> Self {
        let entries = (0..=360)
            .map(|deg| {
                let s = blend(bands, deg as f32);
                let angle = (s.hue / 100.0 * MAX_HUE_DEGREES).to_radians();
                [
                    angle.cos(),
                    angle.sin(),
                    1.0 + s.saturation / 100.0,
                    (s.luminance / 100.0 * MAX_LUMINANCE_STOPS).exp2(),
                ]
            })
            .collect::<Vec<_>>();
        let identity = [1.0, 0.0, 1.0, 1.0];
        let active = entries
            .windows(2)
            .map(|w| w[0] != identity || w[1] != identity)
            .collect();
        Self { entries, active }
    }

    /// The entry for `hue` (0..360), or `None` where the mixer changes nothing.
    #[inline]
    fn at(&self, hue: f32) -> Option<[f32; 4]> {
        let pos = hue.clamp(0.0, 359.999);
        let i = pos as usize;
        if !self.active[i] {
            return None;
        }
        let t = pos - i as f32;
        let (a, b) = (self.entries[i], self.entries[i + 1]);
        Some([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ])
    }
}

/// The shifts at `hue`: neighbouring bands blended with a smoothstep.
fn blend(bands: &[HslShift; 8], hue: f32) -> HslShift {
    let h = hue.rem_euclid(360.0);
    let i = BAND_HUES.iter().rposition(|&c| c <= h).unwrap_or(0);
    let j = (i + 1) % BAND_HUES.len();
    let end = if j == 0 { 360.0 } else { BAND_HUES[j] };
    let t = (h - BAND_HUES[i]) / (end - BAND_HUES[i]);
    let s = t * t * (3.0 - 2.0 * t);
    let (a, b) = (bands[i], bands[j]);
    HslShift {
        hue: a.hue + (b.hue - a.hue) * s,
        saturation: a.saturation + (b.saturation - a.saturation) * s,
        luminance: a.luminance + (b.luminance - a.luminance) * s,
    }
}

#[inline]
pub fn apply(rgb: [f32; 3], table: &MixerTable) -> [f32; 3] {
    let p = rgb.map(|c| c.max(0.0).sqrt());
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    let range = max - min;
    if max <= 1e-6 || range <= 0.0 {
        return rgb;
    }
    let fade = smoothstep(NEUTRAL_FADE.0, NEUTRAL_FADE.1, range / max);
    if fade <= 0.0 {
        return rgb;
    }
    let sixty_over_range = 60.0 / range;
    let hue = if max == p[0] {
        let h = (p[1] - p[2]) * sixty_over_range;
        if h < 0.0 { h + 360.0 } else { h }
    } else if max == p[1] {
        (p[2] - p[0]) * sixty_over_range + 120.0
    } else {
        (p[0] - p[1]) * sixty_over_range + 240.0
    };
    let Some([cos, sin, chroma_scale, gain]) = table.at(hue) else {
        return rgb;
    };

    // Rotate the chroma vector around the grey axis (Rodrigues; its component along
    // the axis is zero), then scale it.
    let m = (p[0] + p[1] + p[2]) / 3.0;
    let c = p.map(|v| v - m);
    let k = 1.0 / 3f32.sqrt();
    let cross = [(c[2] - c[1]) * k, (c[0] - c[2]) * k, (c[1] - c[0]) * k];
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let v = m + (c[i] * cos + cross[i] * sin) * chroma_scale;
        // Squared back to linear, keeping the sign: a channel pushed past sRGB stays
        // negative, for the output's compression (ADR 0060).
        out[i] = v * v.abs();
    }
    // Hue and saturation keep luminance; the luminance control is a plain gain.
    let y0 = luma(rgb);
    let y1 = luma(out);
    let scale = if y1 > 1e-9 { y0 / y1 * gain } else { gain };
    [
        rgb[0] + (out[0] * scale - rgb[0]) * fade,
        rgb[1] + (out[1] * scale - rgb[1]) * fade,
        rgb[2] + (out[2] * scale - rgb[2]) * fade,
    ]
}

#[inline]
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * REC709_LUMA[0] + rgb[1] * REC709_LUMA[1] + rgb[2] * REC709_LUMA[2]
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 3] = [0.60, 0.05, 0.04];
    const SKY: [f32; 3] = [0.13, 0.30, 0.72];
    const FOLIAGE: [f32; 3] = [0.10, 0.30, 0.06];

    fn table(f: impl FnOnce(&mut ColourMixer)) -> MixerTable {
        let mut m = ColourMixer::default();
        f(&mut m);
        MixerTable::new(&m.bands())
    }

    fn hue_of(rgb: [f32; 3]) -> f32 {
        let p = rgb.map(f32::sqrt);
        let (max, min) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        let r = max - min;
        if max == p[0] {
            (60.0 * (p[1] - p[2]) / r).rem_euclid(360.0)
        } else if max == p[1] {
            60.0 * (p[2] - p[0]) / r + 120.0
        } else {
            60.0 * (p[0] - p[1]) / r + 240.0
        }
    }

    fn chroma(rgb: [f32; 3]) -> f32 {
        let p = rgb.map(f32::sqrt);
        p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
    }

    #[test]
    fn identity_and_neutrals_are_unchanged() {
        let t = table(|_| {});
        for c in [RED, SKY, FOLIAGE] {
            let out = apply(c, &t);
            assert!(
                out.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-6),
                "{out:?}"
            );
        }
        let strong = table(|m| {
            for b in m.bands_mut() {
                *b = HslShift {
                    hue: 100.0,
                    saturation: 100.0,
                    luminance: -100.0,
                };
            }
        });
        assert_eq!(apply([0.3; 3], &strong), [0.3; 3]);
        assert_eq!(apply([0.0; 3], &strong), [0.0; 3]);
    }

    #[test]
    fn a_band_changes_its_colours_and_leaves_others() {
        let t = table(|m| m.blue.saturation = -100.0);
        assert!(chroma(apply(SKY, &t)) < 0.35 * chroma(SKY));
        let foliage = apply(FOLIAGE, &t);
        assert!(
            foliage
                .iter()
                .zip(FOLIAGE)
                .all(|(a, b)| (a - b).abs() < 1e-6)
        );
    }

    #[test]
    fn hue_moves_towards_the_neighbouring_band() {
        let plus = table(|m| m.red.hue = 100.0);
        let h = hue_of(apply(RED, &plus));
        assert!(
            (15.0..40.0).contains(&h),
            "red +100 -> {h}° (towards orange)"
        );
        let minus = table(|m| m.red.hue = -100.0);
        let h = hue_of(apply(RED, &minus));
        assert!(h > 320.0, "red -100 -> {h}° (towards magenta)");
    }

    #[test]
    fn hue_and_saturation_keep_luminance_and_luminance_is_a_gain() {
        let t = table(|m| {
            m.blue.hue = 60.0;
            m.blue.saturation = 50.0;
        });
        assert!((luma(apply(SKY, &t)) - luma(SKY)).abs() < 1e-5);
        let darker = table(|m| m.blue.luminance = -100.0);
        let ratio = luma(apply(SKY, &darker)) / luma(SKY);
        assert!((0.45..0.6).contains(&ratio), "{ratio}");
    }

    #[test]
    fn bands_blend_without_steps() {
        // Sweep the hue circle at constant brightness: a strong edit to one band must
        // change the output smoothly.
        let t = table(|m| {
            m.yellow.luminance = -100.0;
            m.green.hue = 100.0;
            m.aqua.saturation = -100.0;
        });
        let colour = |deg: f32| {
            let h = deg.to_radians();
            [
                0.5 + 0.3 * h.cos(),
                0.5 + 0.3 * (h - 2.094).cos(),
                0.5 + 0.3 * (h + 2.094).cos(),
            ]
            .map(|v: f32| v * v)
        };
        let mut last = apply(colour(0.0), &t);
        for i in 1..=720 {
            let out = apply(colour(i as f32 / 2.0), &t);
            let step = out
                .iter()
                .zip(last)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(step < 0.02, "jump of {step} at {}°", i as f32 / 2.0);
            last = out;
        }
    }

    #[test]
    fn blend_hits_each_band_at_its_centre() {
        let mut m = ColourMixer::default();
        for (i, b) in m.bands_mut().into_iter().enumerate() {
            b.hue = i as f32 * 10.0;
        }
        for (i, &c) in BAND_HUES.iter().enumerate() {
            assert!((blend(&m.bands(), c).hue - i as f32 * 10.0).abs() < 1e-4);
        }
        // Halfway between magenta (300°) and red (360°).
        assert!((blend(&m.bands(), 330.0).hue - 35.0).abs() < 1e-4);
    }
}
