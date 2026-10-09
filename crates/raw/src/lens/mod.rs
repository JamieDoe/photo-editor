//! A lens's corrections as the camera recorded them in the raw file (ADR 0075), so
//! distortion and vignetting can be corrected without a lens database.
//!
//! - **Sony** (ARW): in the raw image's IFD, tags `0x7037` (distortion) and `0x7032`
//!   (vignetting), each a count then that many signed 16-bit values at radii evenly
//!   spaced from the centre to the corner.
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
}

/// Sony's tags.
const SONY_VIGNETTING: u16 = 0x7032;
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
    let (mut distortion, mut vignetting, mut lens) = (None, None, None);
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
                _ => {}
            }
        }
    }
    sony_profile(distortion, vignetting).map(|p| LensProfile { lens, ..p })
}

/// A Sony curve: a count `n`, then `n` values.
fn sony_curve(values: &[i16]) -> Option<Vec<i16>> {
    let n = usize::try_from(*values.first()?).ok()?;
    (n >= 2 && values.len() > n).then(|| values[1..=n].to_vec())
}

/// Sony's encodings as curves (as darktable and RawTherapee read them):
/// - distortion: `1 + v / 2^14`;
/// - vignetting: `2^(0.5 - 2^(v / 2^13 - 1))`, so 0 is no vignetting.
fn sony_profile(distortion: Option<Vec<i16>>, vignetting: Option<Vec<i16>>) -> Option<LensProfile> {
    let n = distortion.as_ref().or(vignetting.as_ref())?.len();
    let fits = |c: &Option<Vec<i16>>| c.as_ref().is_none_or(|c| c.len() == n);
    if !fits(&distortion) || !fits(&vignetting) {
        return None;
    }
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
    })
}

#[cfg(test)]
mod tests;
