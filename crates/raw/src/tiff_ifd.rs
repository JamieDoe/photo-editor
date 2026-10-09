//! A minimal reader of TIFF IFDs: the tags of a TIFF-based file (raw files, TIFF
//! images) read by seeking, a few kilobytes, never the image data.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// A TIFF file's byte order and first IFD.
pub(crate) struct Tiff {
    little: bool,
    pub first_ifd: u32,
}

/// One IFD entry: its tag, type, count, and the four bytes holding its value or
/// where it is.
pub(crate) struct Entry {
    pub tag: u16,
    kind: u16,
    count: u32,
    value: [u8; 4],
}

/// IFDs with more entries than this are taken as damage.
const MAX_ENTRIES: u16 = 1000;
/// Values longer than this aren't read.
const MAX_VALUES: u32 = 4096;

impl Tiff {
    pub(crate) fn open(file: &mut File) -> Option<Self> {
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
    pub(crate) fn entries(&self, file: &mut File, offset: u32) -> Option<Vec<Entry>> {
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
    pub(crate) fn shorts(&self, file: &mut File, e: &Entry) -> Option<Vec<i16>> {
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
    pub(crate) fn text(&self, file: &mut File, e: &Entry) -> Option<String> {
        if e.kind != 2 {
            return None;
        }
        let b = self.bytes(file, e, 1)?;
        let text = String::from_utf8_lossy(&b);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// `e`'s values if it holds 32-bit offsets (LONG or IFD).
    pub(crate) fn longs(&self, file: &mut File, e: &Entry) -> Option<Vec<u32>> {
        if e.kind != 4 && e.kind != 13 {
            return None;
        }
        let b = self.bytes(file, e, 4)?;
        Some(b.as_chunks::<4>().0.iter().map(|c| self.u32(c)).collect())
    }
}

impl Tiff {
    /// `e`'s values if it holds unsigned or signed rationals, as numbers (a zero
    /// denominator gives none).
    pub(crate) fn rationals(&self, file: &mut File, e: &Entry) -> Option<Vec<f64>> {
        if e.kind != 5 && e.kind != 10 {
            return None;
        }
        let b = self.bytes(file, e, 8)?;
        b.as_chunks::<8>()
            .0
            .iter()
            .map(|c| {
                let (n, d) = (self.u32(&c[..4]), self.u32(&c[4..]));
                let (n, d) = if e.kind == 10 {
                    (f64::from(n as i32), f64::from(d as i32))
                } else {
                    (f64::from(n), f64::from(d))
                };
                (d != 0.0).then(|| n / d)
            })
            .collect()
    }

    /// `e`'s first value if it holds an unsigned integer (BYTE, SHORT or LONG).
    pub(crate) fn unsigned(&self, file: &mut File, e: &Entry) -> Option<u32> {
        match e.kind {
            1 => self.bytes(file, e, 1)?.first().map(|&b| u32::from(b)),
            3 => self.shorts(file, e)?.first().map(|&v| u32::from(v as u16)),
            4 => self.longs(file, e)?.first().copied(),
            _ => None,
        }
    }
}
