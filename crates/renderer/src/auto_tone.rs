//! Auto tone (ADR 0071): the design's Auto button, a starting point for the photo's
//! light. Exposure, Contrast, Highlights, Shadows, Whites and Blacks are found by
//! rendering a small sample of the photo through the real pipeline, as edited but
//! with those six at zero, and moving each only as far as it takes to bring the
//! sample into a band; Vibrance follows from how colourful the sample is. Measured,
//! not guessed, so the result follows the renderer's own curves.
//!
//! The default render already matches the camera's own JPEG (ADR 0022), so a
//! well-exposed photo should hardly change: each setting stays at zero while the
//! photo is within its band, and outside it moves to the band's nearer edge. The
//! bands, on the rendered (sRGB-encoded) luma:
//! - **Exposure:** the median within [`MEDIAN`]; a night scene stays a night scene
//!   (the change is bounded by [`EXPOSURE_RANGE`]).
//! - **Contrast:** only added, to a flat photo whose middle half spreads less than
//!   [`MIN_SPREAD`].
//! - **Highlights:** only to recover, when more than [`BRIGHT_SHARE`] of the photo is
//!   above [`BRIGHT`].
//! - **Shadows:** only to open up, when more than [`DARK_SHARE`] is below [`DARK`].
//! - **Whites / Blacks:** the tonal range's ends (99.5th and 0.5th percentiles)
//!   within [`WHITE_POINT`] and [`BLACK_POINT`]: brought nearer white and black when
//!   they fall well short, pulled in when they clip.

use image_core::{OutputImage, PixelFormat};

use crate::EditRecipe;

/// The median's band.
pub const MEDIAN: (f32, f32) = (0.40, 0.52);
/// The middle half's spread (75th less 25th percentile) below which contrast is added.
pub const MIN_SPREAD: f32 = 0.26;
/// Bright: near white. At most `BRIGHT_SHARE` of the photo there.
pub const BRIGHT: f32 = 0.92;
pub const BRIGHT_SHARE: f32 = 0.03;
/// Dark: near black. At most `DARK_SHARE` of the photo there.
pub const DARK: f32 = 0.06;
pub const DARK_SHARE: f32 = 0.06;
/// The bands of the white end (99.5th percentile) and black end (0.5th).
pub const WHITE_POINT: (f32, f32) = (0.90, 0.985);
pub const BLACK_POINT: (f32, f32) = (0.012, 0.06);
/// How far Exposure may move, in stops.
pub const EXPOSURE_RANGE: (f32, f32) = (-2.0, 2.5);

/// Auto tone's settings for a photo.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AutoTone {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub vibrance: f32,
}

impl AutoTone {
    /// `recipe` with these settings.
    pub fn apply(&self, recipe: &EditRecipe) -> EditRecipe {
        EditRecipe {
            exposure: self.exposure,
            contrast: self.contrast,
            highlights: self.highlights,
            shadows: self.shadows,
            whites: self.whites,
            blacks: self.blacks,
            vibrance: self.vibrance,
            ..recipe.clone()
        }
    }
}

/// What a rendered sample's tones are: its luma histogram, and its mean colourfulness.
#[derive(Debug, Clone)]
pub struct ToneStats {
    /// Pixels per 8-bit luma value.
    histogram: [u32; 256],
    total: u32,
    /// Mean of each pixel's largest less smallest channel, 0..1.
    pub colourfulness: f32,
}

impl ToneStats {
    /// The tones of `image` (8-bit, sRGB-encoded RGB or RGBA).
    pub fn of(image: &OutputImage) -> Self {
        let stride = match image.format() {
            PixelFormat::Rgba8 => 4,
            _ => 3,
        };
        let mut histogram = [0u32; 256];
        let mut colour = 0u64;
        let mut total = 0u32;
        for px in image.data().chunks_exact(stride) {
            let (r, g, b) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
            // Rec. 709 weights in 1/256ths.
            let luma = (54 * r + 183 * g + 19 * b + 128) >> 8;
            histogram[luma.min(255) as usize] += 1;
            colour += u64::from(r.max(g).max(b) - r.min(g).min(b));
            total += 1;
        }
        Self {
            histogram,
            total: total.max(1),
            colourfulness: colour as f32 / (f32::from(255u8) * total.max(1) as f32),
        }
    }

    /// The luma below which `p` (0..1) of the pixels fall, 0..1.
    pub fn percentile(&self, p: f32) -> f32 {
        let wanted = p.clamp(0.0, 1.0) * self.total as f32;
        let mut seen = 0.0f32;
        for (v, &n) in self.histogram.iter().enumerate() {
            if n > 0 && seen + n as f32 >= wanted {
                // Interpolated within the bin.
                let within = ((wanted - seen) / n as f32).clamp(0.0, 1.0);
                return (v as f32 + within) / 256.0;
            }
            seen += n as f32;
        }
        1.0
    }

    /// The share of pixels with luma above `t` (0..1).
    pub fn share_above(&self, t: f32) -> f32 {
        let from = ((t * 256.0).ceil() as usize).min(256);
        self.histogram[from..].iter().sum::<u32>() as f32 / self.total as f32
    }

    /// The share of pixels with luma below `t` (0..1).
    pub fn share_below(&self, t: f32) -> f32 {
        let to = ((t * 256.0).floor() as usize).min(256);
        self.histogram[..to].iter().sum::<u32>() as f32 / self.total as f32
    }
}

/// Bisection steps per slider: a slider's range in 2^7 steps is finer than it shows.
const STEPS: usize = 7;

/// The value in `lo..hi` at which `f` (rising with the value) meets `target`: `lo` or
/// `hi` when it can't.
fn solve<E>(
    lo: f32,
    hi: f32,
    target: f32,
    f: &mut impl FnMut(f32) -> Result<f32, E>,
) -> Result<f32, E> {
    if f(lo)? >= target {
        return Ok(lo);
    }
    if f(hi)? <= target {
        return Ok(hi);
    }
    let (mut lo, mut hi) = (lo, hi);
    for _ in 0..STEPS {
        let mid = (lo + hi) / 2.0;
        if f(mid)? < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.0)
}

/// The setting in `lo..hi` (around zero) that brings `f` (rising with it) into `band`:
/// zero when it is already there, else the nearer edge as far as the range allows.
fn settle<E>(
    lo: f32,
    hi: f32,
    (low, high): (f32, f32),
    mut f: impl FnMut(f32) -> Result<f32, E>,
) -> Result<f32, E> {
    let now = f(0.0)?;
    let v = if now < low {
        solve(0.0, hi, low, &mut f)?
    } else if now > high {
        solve(lo, 0.0, high, &mut f)?
    } else {
        return Ok(0.0);
    };
    // A setting that can't bring the photo into its band, and hardly changes it on
    // the way, stays at zero rather than going to its limit for nothing: Whites, say,
    // acts on tones near the sensor's white, and a photo with none there can't be
    // brightened by it.
    let reached = f(v)?;
    let short = reached < low - REACHED || reached > high + REACHED;
    if short && (reached - now).abs() < MIN_EFFECT * now.abs().max(0.1) {
        return Ok(0.0);
    }
    Ok(v)
}

/// How near its band a measure must come to count as reached.
const REACHED: f32 = 0.005;

/// The least change worth moving a setting for, as a share of the measure (luma, a
/// spread, or a share of the photo; at least a tenth): Whites moving a white end of
/// 0.85 by 0.001 does nothing, Contrast widening a flat photo's 0.05 spread by 0.01
/// does plenty.
const MIN_EFFECT: f32 = 0.02;

/// One of the settings Auto sets: Auto per setting (Shift-double-click on its slider,
/// as in Lightroom) finds just that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ToneSetting {
    Exposure,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Vibrance,
}

impl ToneSetting {
    /// The order Auto finds them in: each measured with the ones before it set.
    pub const ALL: [Self; 7] = [
        Self::Exposure,
        Self::Contrast,
        Self::Highlights,
        Self::Shadows,
        Self::Whites,
        Self::Blacks,
        Self::Vibrance,
    ];

    /// `recipe` with this setting at `v`.
    fn with(self, recipe: &EditRecipe, v: f32) -> EditRecipe {
        let mut r = recipe.clone();
        *match self {
            Self::Exposure => &mut r.exposure,
            Self::Contrast => &mut r.contrast,
            Self::Highlights => &mut r.highlights,
            Self::Shadows => &mut r.shadows,
            Self::Whites => &mut r.whites,
            Self::Blacks => &mut r.blacks,
            Self::Vibrance => &mut r.vibrance,
        } = v;
        r
    }
}

/// How a setting is found: kept within its band (Auto, gentle on a photo that needs
/// little) or aimed at its target (Auto for that setting alone, asked for by name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aim {
    Band,
    Target,
}

/// What Auto for one setting aims at, on the rendered luma: the median mid-band, the
/// middle half's spread, at most 1 % near white and 2 % near black (Highlights only
/// recovers and Shadows only opens, as in the bands), and the tonal range's ends just
/// short of white and black.
pub const MEDIAN_TARGET: f32 = 0.46;
pub const SPREAD_TARGET: f32 = 0.36;
pub const BRIGHT_TARGET: f32 = 0.01;
pub const DARK_TARGET: f32 = 0.02;
pub const WHITE_TARGET: f32 = 0.975;
pub const BLACK_TARGET: f32 = 0.02;

/// The value of `setting` for the photo `measure` renders, the rest of `recipe` as it
/// is: Auto for that setting alone (Shift-double-click on its slider), aimed at its
/// target. Vibrance follows from the photo's colourfulness.
pub fn auto_setting<E>(
    recipe: &EditRecipe,
    setting: ToneSetting,
    measure: impl FnMut(&EditRecipe) -> Result<ToneStats, E>,
) -> Result<f32, E> {
    find(recipe, setting, Aim::Target, measure)
}

/// `setting` found by `aim`: within its band, zero while the photo is there and else
/// the band's nearer edge (see the module's notes); or at its target.
fn find<E>(
    recipe: &EditRecipe,
    setting: ToneSetting,
    aim: Aim,
    mut measure: impl FnMut(&EditRecipe) -> Result<ToneStats, E>,
) -> Result<f32, E> {
    let mut at =
        |v: f32, read: fn(&ToneStats) -> f32| measure(&setting.with(recipe, v)).map(|s| read(&s));
    // The band or the target, as one band (a target is a band of one value).
    let band = |band: (f32, f32), target: f32| match aim {
        Aim::Band => band,
        Aim::Target => (target, target),
    };
    Ok(match setting {
        ToneSetting::Exposure => {
            let (lo, hi) = EXPOSURE_RANGE;
            let b = band(MEDIAN, MEDIAN_TARGET);
            round_to(settle(lo, hi, b, |v| at(v, |s| s.percentile(0.5)))?, 0.05)
        }
        ToneSetting::Contrast => {
            // Asked for, Contrast may also lessen a harsh photo's.
            let (lo, b) = match aim {
                Aim::Band => (0.0, (MIN_SPREAD, f32::INFINITY)),
                Aim::Target => (-15.0, (SPREAD_TARGET, SPREAD_TARGET)),
            };
            settle(lo, 30.0, b, |v| {
                at(v, |s| s.percentile(0.75) - s.percentile(0.25))
            })?
            .round()
        }
        // More Highlights means less near white: recover down to the share allowed.
        ToneSetting::Highlights => {
            let b = band((f32::NEG_INFINITY, BRIGHT_SHARE), BRIGHT_TARGET);
            settle(-70.0, 0.0, (f32::NEG_INFINITY, b.1), |v| {
                at(v, |s| s.share_above(BRIGHT))
            })?
            .round()
        }
        // More Shadows means less near black: open up to the share allowed.
        ToneSetting::Shadows => {
            let b = band((DARK_SHARE, f32::INFINITY), DARK_TARGET);
            settle(0.0, 60.0, (-b.0, f32::INFINITY), |v| {
                at(v, |s| -s.share_below(DARK))
            })?
            .round()
        }
        ToneSetting::Whites => {
            let b = band(WHITE_POINT, WHITE_TARGET);
            settle(-40.0, 40.0, b, |v| at(v, |s| s.percentile(0.995)))?.round()
        }
        ToneSetting::Blacks => {
            let b = band(BLACK_POINT, BLACK_TARGET);
            settle(-40.0, 30.0, b, |v| at(v, |s| s.percentile(0.005)))?.round()
        }
        // Muted photos gain colour, already colourful ones hardly any.
        ToneSetting::Vibrance => {
            let colourful = at(0.0, |s| s.colourfulness)?;
            (((0.30 - colourful) * 100.0).clamp(0.0, 20.0) / 5.0).round() * 5.0
        }
    })
}

/// Auto tone for the photo `measure` renders: it renders `recipe` (a small sample is
/// plenty) and gives its tones. `recipe` is the photo as edited; its six tone
/// sliders are found afresh, in [`ToneSetting::ALL`]'s order, the rest of the edit kept.
pub fn auto_tone<E>(
    recipe: &EditRecipe,
    mut measure: impl FnMut(&EditRecipe) -> Result<ToneStats, E>,
) -> Result<AutoTone, E> {
    let mut r = EditRecipe {
        exposure: 0.0,
        contrast: 0.0,
        highlights: 0.0,
        shadows: 0.0,
        whites: 0.0,
        blacks: 0.0,
        ..recipe.clone()
    };
    for setting in ToneSetting::ALL {
        let v = find(&r, setting, Aim::Band, &mut measure)?;
        r = setting.with(&r, v);
    }
    Ok(AutoTone {
        exposure: r.exposure,
        contrast: r.contrast,
        highlights: r.highlights,
        shadows: r.shadows,
        whites: r.whites,
        blacks: r.blacks,
        vibrance: r.vibrance,
    })
}

fn round_to(v: f32, step: f32) -> f32 {
    (v / step).round() * step
}

#[cfg(test)]
mod tests;
