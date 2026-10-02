//! Minimal uncompressed mosaic DNG writer for synthetic RAW fixtures.
//!
//! The simulated sensor has per-channel sensitivities (so white balance matters) and
//! an RGGB (Bayer) or Fujifilm X-Trans mosaic. Its colour matrix is chosen so that
//! camera WB + matrix recover the scene-linear chart values, giving tests a known
//! expected output.

use image_core::color::XYZ_TO_LINEAR_SRGB;

use crate::scene::sample;

pub const DNG_MAKE: &str = "PhotoEditor";
pub const DNG_MODEL: &str = "Synthetic Chart";

/// Relative channel sensitivities of the simulated sensor (R, G, B).
const SENSITIVITY: [f32; 3] = [0.5, 1.0, 0.7];
const BLACK_LEVEL: u16 = 512;
const WHITE_LEVEL: u16 = 16383;
/// RGGB: 0 = red, 1 = green, 2 = blue.
const CFA: [u8; 4] = [0, 1, 1, 2];
/// Fujifilm X-Trans 6x6 pattern, same colour codes.
const XTRANS_CFA: [u8; 36] = [
    1, 1, 0, 1, 1, 2, //
    1, 1, 2, 1, 1, 0, //
    2, 0, 1, 0, 2, 1, //
    1, 1, 2, 1, 1, 0, //
    1, 1, 0, 1, 1, 2, //
    0, 2, 1, 2, 0, 1, //
];
/// Peak amplitude of the X-Trans chart's per-photosite texture (scene-linear).
pub const XTRANS_TEXTURE: f32 = 0.02;

const BYTE: u16 = 1;
const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const SRATIONAL: u16 = 10;

struct Tag {
    id: u16,
    kind: u16,
    count: u32,
    data: Vec<u8>,
}

impl Tag {
    fn short(id: u16, v: u16) -> Self {
        Self {
            id,
            kind: SHORT,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }
    fn shorts(id: u16, v: &[u16]) -> Self {
        Self {
            id,
            kind: SHORT,
            count: v.len() as u32,
            data: v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        }
    }
    fn long(id: u16, v: u32) -> Self {
        Self {
            id,
            kind: LONG,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }
    fn bytes(id: u16, v: &[u8]) -> Self {
        Self {
            id,
            kind: BYTE,
            count: v.len() as u32,
            data: v.to_vec(),
        }
    }
    fn ascii(id: u16, s: &str) -> Self {
        let mut data = s.as_bytes().to_vec();
        data.push(0);
        Self {
            id,
            kind: ASCII,
            count: data.len() as u32,
            data,
        }
    }
    fn rationals(id: u16, v: &[f32]) -> Self {
        let data = v
            .iter()
            .flat_map(|x| {
                let n = (x * 10_000.0).round() as u32;
                [n.to_le_bytes(), 10_000u32.to_le_bytes()].concat()
            })
            .collect();
        Self {
            id,
            kind: RATIONAL,
            count: v.len() as u32,
            data,
        }
    }
    fn srationals(id: u16, v: &[f32]) -> Self {
        let data = v
            .iter()
            .flat_map(|x| {
                let n = (x * 10_000.0).round() as i32;
                [n.to_le_bytes(), 10_000i32.to_le_bytes()].concat()
            })
            .collect();
        Self {
            id,
            kind: SRATIONAL,
            count: v.len() as u32,
            data,
        }
    }
}

/// Generates the synthetic chart as a 14-bit uncompressed Bayer DNG.
pub fn chart_dng(width: u32, height: u32) -> Vec<u8> {
    mosaic_dng(width, height, &CFA, 2, 0.0)
}

/// Generates the synthetic chart as a 14-bit uncompressed X-Trans DNG.
///
/// Every photosite carries deterministic texture of up to [`XTRANS_TEXTURE`], so
/// neighbouring samples of one colour differ and demosaic results depend on exactly
/// which samples are used (the flat chart alone would hide ordering bugs).
pub fn chart_xtrans_dng(width: u32, height: u32) -> Vec<u8> {
    mosaic_dng(width, height, &XTRANS_CFA, 6, XTRANS_TEXTURE)
}

/// Deterministic value in [-1, 1] per photosite.
fn texture(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 32767.5 - 1.0
}

fn mosaic_dng(width: u32, height: u32, cfa: &[u8], period: u32, texture_amp: f32) -> Vec<u8> {
    debug_assert_eq!(cfa.len() as u32, period * period);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 2);
    let range = f32::from(WHITE_LEVEL - BLACK_LEVEL);
    for y in 0..height {
        for x in 0..width {
            let c = cfa[((y % period) * period + (x % period)) as usize] as usize;
            let scene = sample(x, y, width, height)[c] + texture_amp * texture(x, y);
            let v = f32::from(BLACK_LEVEL) + scene * SENSITIVITY[c] * range;
            pixels.extend((v.round().clamp(0.0, f32::from(WHITE_LEVEL)) as u16).to_le_bytes());
        }
    }

    // Camera native = diag(sensitivity) * linear sRGB, so XYZ -> camera is:
    let mut color_matrix = [0.0f32; 9];
    for r in 0..3 {
        for c in 0..3 {
            color_matrix[r * 3 + c] = SENSITIVITY[r] * XYZ_TO_LINEAR_SRGB[r][c];
        }
    }
    let max_s = SENSITIVITY.iter().copied().fold(0.0, f32::max);
    let as_shot_neutral: Vec<f32> = SENSITIVITY.iter().map(|s| s / max_s).collect();

    let strip_bytes = pixels.len() as u32;
    // Tags must be sorted by id. StripOffsets is patched after layout.
    let tags = vec![
        Tag::long(254, 0),
        Tag::long(256, width),
        Tag::long(257, height),
        Tag::short(258, 16),
        Tag::short(259, 1),
        Tag::short(262, 32803),
        Tag::ascii(271, DNG_MAKE),
        Tag::ascii(272, DNG_MODEL),
        Tag::long(273, 0),
        Tag::short(274, 1),
        Tag::short(277, 1),
        Tag::long(278, height),
        Tag::long(279, strip_bytes),
        Tag::short(284, 1),
        Tag::ascii(305, "photo-editor fixtures"),
        Tag::shorts(33421, &[period as u16, period as u16]),
        Tag::bytes(33422, cfa),
        Tag::bytes(50706, &[1, 4, 0, 0]),
        Tag::bytes(50707, &[1, 1, 0, 0]),
        Tag::ascii(50708, &format!("{DNG_MAKE} {DNG_MODEL}")),
        Tag::long(50714, u32::from(BLACK_LEVEL)),
        Tag::long(50717, u32::from(WHITE_LEVEL)),
        Tag::srationals(50721, &color_matrix),
        Tag::rationals(50728, &as_shot_neutral),
        Tag::short(50778, 21),
    ];
    debug_assert!(tags.windows(2).all(|w| w[0].id < w[1].id));

    let ifd_offset = 8u32;
    let ifd_size = 2 + tags.len() as u32 * 12 + 4;
    let mut extra_offset = ifd_offset + ifd_size;
    let mut extra = Vec::new();
    let mut entries = Vec::new();
    for tag in &tags {
        let value = if tag.data.len() <= 4 {
            let mut v = tag.data.clone();
            v.resize(4, 0);
            v
        } else {
            let off = extra_offset + extra.len() as u32;
            extra.extend_from_slice(&tag.data);
            if extra.len() % 2 == 1 {
                extra.push(0);
            }
            off.to_le_bytes().to_vec()
        };
        entries.push((tag.id, tag.kind, tag.count, value));
    }
    extra_offset += extra.len() as u32;
    let strip_offset = extra_offset;
    for e in &mut entries {
        if e.0 == 273 {
            e.3 = strip_offset.to_le_bytes().to_vec();
        }
    }

    let mut out = Vec::with_capacity(strip_offset as usize + pixels.len());
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&ifd_offset.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (id, kind, count, value) in entries {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value);
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&extra);
    debug_assert_eq!(out.len() as u32, strip_offset);
    out.extend_from_slice(&pixels);
    out
}
