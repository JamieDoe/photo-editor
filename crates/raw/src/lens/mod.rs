//! A lens's corrections as the camera recorded them in the raw file (ADR 0075), so
//! distortion and vignetting can be corrected without a lens database.
//!
//! - **Sony** (ARW): in the raw image's IFD, tags `0x7037` (distortion) and `0x7032`
//!   (vignetting), each a count then that many signed 16-bit values at radii evenly
//!   spaced from the centre to the corner.
//!
//! Only the file's headers are read, a few kilobytes, never the image.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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

/// A TIFF file's byte order and first IFD.
struct Tiff {
    little: bool,
    first_ifd: u32,
}

/// One IFD entry: its tag, type, count, and the four bytes holding its value or
/// where it is.
struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    value: [u8; 4],
}

/// IFDs with more entries than this are taken as damage.
const MAX_ENTRIES: u16 = 1000;
/// Values longer than this aren't read.
const MAX_VALUES: u32 = 4096;

impl Tiff {
    fn open(file: &mut File) -> Option<Self> {
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let little = match &head[..4] {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        let tiff = Self {
            little,
            first_ifd: 0,
        };
        Some(Self {
            first_ifd: tiff.u32(&head[4..8]),
            ..tiff
        })
    }

    fn u16(&self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    }

    fn u32(&self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    }

    /// The entries of the IFD at `offset`. IFDs chained after it aren't followed:
    /// the tags read here are in SubIFDs.
    fn entries(&self, file: &mut File, offset: u32) -> Option<Vec<Entry>> {
        file.seek(SeekFrom::Start(u64::from(offset))).ok()?;
        let mut count = [0u8; 2];
        file.read_exact(&mut count).ok()?;
        let n = self.u16(&count);
        if n > MAX_ENTRIES {
            return None;
        }
        let mut raw = vec![0u8; usize::from(n) * 12];
        file.read_exact(&mut raw).ok()?;
        Some(
            raw.as_chunks::<12>()
                .0
                .iter()
                .map(|e| Entry {
                    tag: self.u16(&e[0..2]),
                    kind: self.u16(&e[2..4]),
                    count: self.u32(&e[4..8]),
                    value: [e[8], e[9], e[10], e[11]],
                })
                .collect(),
        )
    }

    /// The bytes of `e`'s value: in the entry when they fit in four, else where it
    /// points.
    fn bytes(&self, file: &mut File, e: &Entry, size: usize) -> Option<Vec<u8>> {
        if e.count > MAX_VALUES {
            return None;
        }
        let len = size * e.count as usize;
        if len <= 4 {
            return Some(e.value[..len].to_vec());
        }
        file.seek(SeekFrom::Start(u64::from(self.u32(&e.value))))
            .ok()?;
        let mut out = vec![0u8; len];
        file.read_exact(&mut out).ok()?;
        Some(out)
    }

    /// `e`'s values if it holds 16-bit integers (SHORT or SSHORT), as signed.
    fn shorts(&self, file: &mut File, e: &Entry) -> Option<Vec<i16>> {
        if e.kind != 3 && e.kind != 8 {
            return None;
        }
        let b = self.bytes(file, e, 2)?;
        Some(
            b.as_chunks::<2>()
                .0
                .iter()
                .map(|c| self.u16(c) as i16)
                .collect(),
        )
    }

    /// `e`'s text if it holds ASCII, trimmed; `None` when empty.
    fn text(&self, file: &mut File, e: &Entry) -> Option<String> {
        if e.kind != 2 {
            return None;
        }
        let b = self.bytes(file, e, 1)?;
        let text = String::from_utf8_lossy(&b);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// `e`'s values if it holds 32-bit offsets (LONG or IFD).
    fn longs(&self, file: &mut File, e: &Entry) -> Option<Vec<u32>> {
        if e.kind != 4 && e.kind != 13 {
            return None;
        }
        let b = self.bytes(file, e, 4)?;
        Some(b.as_chunks::<4>().0.iter().map(|c| self.u32(c)).collect())
    }
}

#[cfg(test)]
mod tests;
