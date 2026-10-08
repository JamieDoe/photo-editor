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
    if now < low {
        solve(0.0, hi, low, &mut f)
    } else if now > high {
        solve(lo, 0.0, high, &mut f)
    } else {
        Ok(0.0)
    }
}

/// Auto tone for the photo `measure` renders: it renders `recipe` (a small sample is
/// plenty) and gives its tones. `recipe` is the photo as edited; its six tone
/// sliders are found afresh, the rest of the edit kept.
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
    let (lo, hi) = EXPOSURE_RANGE;
    r.exposure = round_to(
        settle(lo, hi, MEDIAN, |v| {
            measure(&EditRecipe {
                exposure: v,
                ..r.clone()
            })
            .map(|s| s.percentile(0.5))
        })?,
        0.05,
    );
    r.contrast = settle(0.0, 30.0, (MIN_SPREAD, f32::INFINITY), |v| {
        measure(&EditRecipe {
            contrast: v,
            ..r.clone()
        })
        .map(|s| s.percentile(0.75) - s.percentile(0.25))
    })?
    .round();
    // More Highlights means less near white: recover down to the share allowed.
    r.highlights = settle(-70.0, 0.0, (f32::NEG_INFINITY, BRIGHT_SHARE), |v| {
        measure(&EditRecipe {
            highlights: v,
            ..r.clone()
        })
        .map(|s| s.share_above(BRIGHT))
    })?
    .round();
    // More Shadows means less near black: open up to the share allowed.
    r.shadows = settle(0.0, 60.0, (-DARK_SHARE, f32::INFINITY), |v| {
        measure(&EditRecipe {
            shadows: v,
            ..r.clone()
        })
        .map(|s| -s.share_below(DARK))
    })?
    .round();
    r.whites = settle(-40.0, 40.0, WHITE_POINT, |v| {
        measure(&EditRecipe {
            whites: v,
            ..r.clone()
        })
        .map(|s| s.percentile(0.995))
    })?
    .round();
    r.blacks = settle(-40.0, 30.0, BLACK_POINT, |v| {
        measure(&EditRecipe {
            blacks: v,
            ..r.clone()
        })
        .map(|s| s.percentile(0.005))
    })?
    .round();
    // Vibrance: muted photos gain colour, already colourful ones hardly any.
    let colourful = measure(&r)?.colourfulness;
    let vibrance = (((0.30 - colourful) * 100.0).clamp(0.0, 20.0) / 5.0).round() * 5.0;
    Ok(AutoTone {
        exposure: r.exposure,
        contrast: r.contrast,
        highlights: r.highlights,
        shadows: r.shadows,
        whites: r.whites,
        blacks: r.blacks,
        vibrance,
    })
}

fn round_to(v: f32, step: f32) -> f32 {
    (v / step).round() * step
}

#[cfg(test)]
mod tests;
