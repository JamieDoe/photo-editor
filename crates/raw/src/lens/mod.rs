//! A lens's corrections as the camera recorded them in the raw file (ADR 0075), so
//! distortion, vignetting and chromatic aberration can be corrected without a lens
//! database.
//!
//! - **Sony** (ARW): in the raw image's IFD, tags `0x7037` (distortion) and `0x7032`
//!   (vignetting), each a count then that many signed 16-bit values at radii evenly
//!   spaced from the centre to the corner; and `0x7035` (chromatic aberration, ADR
//!   0083), a count then red's values and blue's, at the same radii.
//!
//! Only the file's headers are read, a few kilobytes, never the image.

use std::fs::File;
use std::path::Path;

use crate::tiff_ifd::Tiff;

/// A lens's corrections, as curves of the radius: 0 at the centre, 1 at a corner (the
/// half-diagonal) of the image as decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct LensProfile {
    /// Who recorded it: "Sony".
    pub source: &'static str,
    /// The lens, as the camera names it (EXIF `LensModel`), if it does.
    pub lens: Option<String>,
    /// Radii the curves are given at, ascending, from 0 to 1.
    pub knots: Vec<f32>,
    /// At each knot, where a point of the scene really is on the sensor: its radius
    /// as a multiple of where an ideal lens would put it (1: none; below 1, barrel).
    pub distortion: Option<Vec<f32>>,
    /// At each knot, how bright the lens leaves the image, relative to its centre (1:
    /// no vignetting).
    pub vignetting: Option<Vec<f32>>,
    /// At each knot, how much larger the lens makes the red and the blue images than
    /// the green one: a point's radius in red (or blue) as a multiple of its radius in
    /// green (1: no chromatic aberration).
    pub chromatic_aberration: Option<[Vec<f32>; 2]>,
}

/// Sony's tags.
const SONY_VIGNETTING: u16 = 0x7032;
const SONY_CHROMATIC_ABERRATION: u16 = 0x7035;
const SONY_DISTORTION: u16 = 0x7037;
const SUB_IFDS: u16 = 0x014a;
const EXIF_IFD: u16 = 0x8769;
const LENS_MODEL: u16 = 0xa434;
/// IFDs read at most, against loops in damaged files.
const MAX_IFDS: usize = 16;

/// The corrections recorded in `path`, if it is a raw file that has them.
pub fn read(path: &Path) -> Option<LensProfile> {
    let mut file = File::open(path).ok()?;
    let tiff = Tiff::open(&mut file)?;
    let mut pending = vec![tiff.first_ifd];
    let mut seen = Vec::new();
    let (mut distortion, mut vignetting, mut chromatic, mut lens) = (None, None, None, None);
    while let Some(offset) = pending.pop() {
        if offset == 0 || seen.contains(&offset) || seen.len() >= MAX_IFDS {
            continue;
        }
        seen.push(offset);
        let Some(entries) = tiff.entries(&mut file, offset) else {
            continue;
        };
        for e in &entries {
            match e.tag {
                SUB_IFDS | EXIF_IFD => pending.extend(tiff.longs(&mut file, e).unwrap_or_default()),
                LENS_MODEL => lens = tiff.text(&mut file, e),
                SONY_DISTORTION => {
                    distortion = tiff.shorts(&mut file, e).and_then(|v| sony_curve(&v))
                }
                SONY_VIGNETTING => {
                    vignetting = tiff.shorts(&mut file, e).and_then(|v| sony_curve(&v))
                }
                SONY_CHROMATIC_ABERRATION => {
                    chromatic = tiff.shorts(&mut file, e).and_then(|v| sony_curve(&v))
                }
                _ => {}
            }
        }
    }
    sony_profile(distortion, vignetting, chromatic).map(|p| LensProfile { lens, ..p })
}

/// A Sony curve: a count `n`, then `n` values.
fn sony_curve(values: &[i16]) -> Option<Vec<i16>> {
    let n = usize::try_from(*values.first()?).ok()?;
    (n >= 2 && values.len() > n).then(|| values[1..=n].to_vec())
}

/// Sony's encodings as curves (as darktable and RawTherapee read them):
/// - distortion: `1 + v / 2^14`;
/// - vignetting: `2^(0.5 - 2^(v / 2^13 - 1))`, so 0 is no vignetting;
/// - chromatic aberration: red's values then blue's, each `1 + v / 2^21`.
fn sony_profile(
    distortion: Option<Vec<i16>>,
    vignetting: Option<Vec<i16>>,
    chromatic: Option<Vec<i16>>,
) -> Option<LensProfile> {
    // Red's and blue's curves, each as long as the others.
    let chromatic = chromatic.filter(|c| c.len() % 2 == 0).map(|c| {
        let (red, blue) = c.split_at(c.len() / 2);
        [red, blue].map(|v| {
            v.iter()
                .map(|&v| 1.0 + f32::from(v) / 2_097_152.0)
                .collect::<Vec<f32>>()
        })
    });
    let n = distortion
        .as_ref()
        .or(vignetting.as_ref())
        .map(Vec::len)
        .or(chromatic.as_ref().map(|c| c[0].len()))?;
    let fits = |c: &Option<Vec<i16>>| c.as_ref().is_none_or(|c| c.len() == n);
    if !fits(&distortion) || !fits(&vignetting) || n < 2 {
        return None;
    }
    // A chromatic aberration curve that doesn't fit the others is left out, not the
    // whole profile.
    let chromatic = chromatic.filter(|c| c[0].len() == n);
    let knots = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
    Some(LensProfile {
        source: "Sony",
        lens: None,
        knots,
        distortion: distortion.map(|d| d.iter().map(|&v| 1.0 + f32::from(v) / 16384.0).collect()),
        vignetting: vignetting.map(|d| {
            d.iter()
                .map(|&v| (0.5 - (f32::from(v) / 8192.0 - 1.0).exp2()).exp2())
                .collect()
        }),
        chromatic_aberration: chromatic,
    })
}

#[cfg(test)]
mod tests;
