//! Reading the ICC profile embedded in a rendered image (ADR 0077), so a file in
//! Adobe RGB, Display P3 or ProPhoto keeps its colours instead of being read as sRGB.
//!
//! Matrix/TRC profiles, the kind every RGB working space and display profile is: a
//! tone curve per channel (`rTRC`, `gTRC`, `bTRC`) and the primaries as XYZ colorants
//! adapted to the profile connection space's D50 white (`rXYZ`, `gXYZ`, `bXYZ`). Grey
//! profiles have one curve (`kTRC`). Profiles built only from lookup tables (`A2B0`,
//! most CMYK and printer profiles) aren't read: their files are taken as sRGB, as
//! before.
//!
//! A colour goes through the curves to linear light, through the colorants to XYZ (D50),
//! then to linear sRGB with the D50-adapted (Bradford) sRGB matrix, and is brought into
//! sRGB's gamut with the soft compression raw files get (ADR 0060).

use crate::gamut::{Compression, FROM_REC2020, compress};

/// Why a profile can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IccError {
    /// The data isn't an ICC profile, or is damaged.
    Malformed(&'static str),
    /// A valid profile of a kind not read (CMYK, Lab, lookup tables only).
    Unsupported(String),
}

impl std::fmt::Display for IccError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(why) => write!(f, "damaged ICC profile: {why}"),
            Self::Unsupported(what) => write!(f, "ICC profile not supported: {what}"),
        }
    }
}

impl std::error::Error for IccError {}

/// A tone curve: encoded value (0..1) to linear light.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// `v^gamma` (1: linear).
    Gamma(f32),
    /// Values at evenly spaced inputs, linearly between them.
    Table(Vec<f32>),
    /// ICC parametric curve types 0 to 4: `[g, a, b, c, d, e, f]`, unused ones 0.
    Parametric { kind: u16, p: [f32; 7] },
}

impl Curve {
    /// Linear light for encoded value `v` (0..1).
    pub fn eval(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        match self {
            Self::Gamma(g) => v.powf(*g),
            Self::Table(t) => {
                let x = v * (t.len() - 1) as f32;
                let i = (x as usize).min(t.len() - 2);
                let f = x - i as f32;
                t[i] + (t[i + 1] - t[i]) * f
            }
            Self::Parametric { kind, p } => {
                let [g, a, b, c, d, e, f] = *p;
                let power = |x: f32| if x > 0.0 { x.powf(g) } else { 0.0 };
                match kind {
                    0 => power(v),
                    1 => {
                        if v >= -b / a {
                            power(a * v + b)
                        } else {
                            0.0
                        }
                    }
                    2 => {
                        if v >= -b / a {
                            power(a * v + b) + c
                        } else {
                            c
                        }
                    }
                    3 => {
                        if v >= d {
                            power(a * v + b)
                        } else {
                            c * v
                        }
                    }
                    _ => {
                        if v >= d {
                            power(a * v + b) + e
                        } else {
                            c * v + f
                        }
                    }
                }
            }
        }
        .clamp(0.0, 1.0)
    }
}

/// What a profile describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Rgb,
    Grey,
}

/// An embedded profile, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub kind: Kind,
    /// The profile's name (its `desc` tag), for logs.
    pub description: Option<String>,
    curves: [Curve; 3],
    /// Linear RGB of the profile's space to linear sRGB.
    to_srgb: [[f32; 3]; 3],
}

/// XYZ (D50) to linear sRGB: the inverse of sRGB's Bradford-adapted D50 colorants.
const XYZ_D50_TO_SRGB: [[f64; 3]; 3] = [
    [3.133_856_1, -1.616_866_7, -0.490_614_6],
    [-0.978_768_4, 1.916_141_5, 0.033_454_0],
    [0.071_945_3, -0.228_991_4, 1.405_242_7],
];

/// How colours beyond sRGB come in: as from raw files (ADR 0060), easing in from 97 %
/// of the way to sRGB's edge so that colours inside keep their values.
const FROM_PROFILE: Compression = FROM_REC2020;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn s15f16_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from(u32_at(d, at)? as i32) / 65536.0)
}

/// Reads `data` as an ICC profile.
pub fn parse(data: &[u8]) -> Result<Profile, IccError> {
    if data.len() < 132 || data.get(36..40) != Some(b"acsp") {
        return Err(IccError::Malformed("no profile header"));
    }
    let space = &data[16..20];
    let kind = match space {
        b"RGB " => Kind::Rgb,
        b"GRAY" => Kind::Grey,
        other => {
            return Err(IccError::Unsupported(format!(
                "{} colour space",
                String::from_utf8_lossy(other).trim()
            )));
        }
    };
    let count = u32_at(data, 128).ok_or(IccError::Malformed("no tag table"))? as usize;
    if count > 1000 {
        return Err(IccError::Malformed("too many tags"));
    }
    let tag = |sig: &[u8; 4]| -> Option<&[u8]> {
        (0..count).find_map(|i| {
            let at = 132 + i * 12;
            if data.get(at..at + 4)? != sig {
                return None;
            }
            let (offset, size) = (
                u32_at(data, at + 4)? as usize,
                u32_at(data, at + 8)? as usize,
            );
            data.get(offset..offset.checked_add(size)?)
        })
    };
    let curve = |sig: &[u8; 4]| -> Result<Curve, IccError> {
        let t = tag(sig)
            .ok_or_else(|| IccError::Unsupported("no tone curves (lookup tables only)".into()))?;
        read_curve(t).ok_or(IccError::Malformed("unreadable tone curve"))
    };
    let description = tag(b"desc").and_then(read_text);
    match kind {
        Kind::Grey => {
            let k = curve(b"kTRC")?;
            Ok(Profile {
                kind,
                description,
                curves: [k.clone(), k.clone(), k],
                to_srgb: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            })
        }
        Kind::Rgb => {
            if &data[20..24] != b"XYZ " {
                return Err(IccError::Unsupported("a Lab connection space".into()));
            }
            let xyz = |sig: &[u8; 4]| -> Result<[f64; 3], IccError> {
                let t = tag(sig).ok_or_else(|| {
                    IccError::Unsupported("no colorants (lookup tables only)".into())
                })?;
                if t.get(..4) != Some(b"XYZ ") {
                    return Err(IccError::Malformed("colorant isn't XYZ"));
                }
                Ok([s15f16_at(t, 8), s15f16_at(t, 12), s15f16_at(t, 16)].map(|v| v.unwrap_or(0.0)))
            };
            let columns = [xyz(b"rXYZ")?, xyz(b"gXYZ")?, xyz(b"bXYZ")?];
            // Linear RGB to XYZ: the colorants as columns; then to sRGB.
            let to_srgb = std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    (0..3)
                        .map(|k| XYZ_D50_TO_SRGB[r][k] * columns[c][k])
                        .sum::<f64>() as f32
                })
            });
            Ok(Profile {
                kind,
                description,
                curves: [curve(b"rTRC")?, curve(b"gTRC")?, curve(b"bTRC")?],
                to_srgb,
            })
        }
    }
}

/// A `curv` or `para` tag.
fn read_curve(t: &[u8]) -> Option<Curve> {
    match t.get(..4)? {
        b"curv" => {
            let n = u32_at(t, 8)? as usize;
            match n {
                0 => Some(Curve::Gamma(1.0)),
                1 => Some(Curve::Gamma(f32::from(u16_at(t, 12)?) / 256.0)),
                _ if n <= 65536 => {
                    let values: Option<Vec<f32>> = (0..n)
                        .map(|i| u16_at(t, 12 + 2 * i).map(|v| f32::from(v) / 65535.0))
                        .collect();
                    Some(Curve::Table(values?))
                }
                _ => None,
            }
        }
        b"para" => {
            let kind = u16_at(t, 8)?;
            let used = match kind {
                0 => 1,
                1 => 3,
                2 => 4,
                3 => 5,
                4 => 7,
                _ => return None,
            };
            let mut p = [0.0f32; 7];
            for (i, v) in p.iter_mut().enumerate().take(used) {
                *v = s15f16_at(t, 12 + 4 * i)? as f32;
            }
            let [_, a, ..] = p;
            if kind != 0 && a == 0.0 {
                return None;
            }
            Some(Curve::Parametric { kind, p })
        }
        _ => None,
    }
}

/// A `desc` (version 2) or `mluc` (version 4) tag's text.
fn read_text(t: &[u8]) -> Option<String> {
    let text = match t.get(..4)? {
        b"desc" => {
            let n = u32_at(t, 8)? as usize;
            String::from_utf8_lossy(t.get(12..12 + n)?).into_owned()
        }
        b"mluc" => {
            // The first record: UTF-16BE.
            let (len, offset) = (u32_at(t, 20)? as usize, u32_at(t, 24)? as usize);
            let units: Vec<u16> = t
                .get(offset..offset + len)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes(*b))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => return None,
    };
    let text = text.trim_end_matches('\0').trim();
    (!text.is_empty()).then(|| text.to_owned())
}

impl Profile {
    /// Linear light of channel `c`'s encoded value `v` (0..1).
    #[inline]
    pub fn linearise(&self, c: usize, v: f32) -> f32 {
        self.curves[c].eval(v)
    }

    /// Linear light of this profile's space in linear sRGB, brought into its gamut.
    #[inline]
    pub fn to_srgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.kind == Kind::Grey {
            return rgb;
        }
        let m = &self.to_srgb;
        compress(
            [0, 1, 2].map(|r| m[r][0] * rgb[0] + m[r][1] * rgb[1] + m[r][2] * rgb[2]),
            &FROM_PROFILE,
        )
    }

    /// Whether it is sRGB, or close enough that reading its file as sRGB (exactly as
    /// files without a profile are read) changes nothing visible.
    pub fn is_srgb(&self) -> bool {
        if self.kind != Kind::Rgb {
            return false;
        }
        let identity = (0..3).all(|r| {
            (0..3).all(|c| (self.to_srgb[r][c] - f32::from(u8::from(r == c))).abs() < 0.01)
        });
        let srgb_curves = (0..=32).all(|i| {
            let v = i as f32 / 32.0;
            let target = crate::color::srgb_to_linear(v);
            self.curves
                .iter()
                .all(|c| (c.eval(v) - target).abs() < 0.004)
        });
        identity && srgb_curves
    }
}

#[cfg(test)]
mod tests;
