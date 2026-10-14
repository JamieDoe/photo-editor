//! A TIFF decoded strip by strip (or tile by tile) in parallel, straight to linear light
//! and reduced on the way (ADR 0082).
//!
//! The `tiff` crate decodes a whole image on one thread, and large 16-bit TIFFs are
//! almost all decompression. Their strips and tiles are compressed independently, so
//! here each worker opens the file and decodes a band of them. Each pixel is made linear
//! through its curve as it is read (sRGB's, or the embedded profile's, then into sRGB's
//! primaries: ADR 0077) and, for a thumbnail or a smaller decode, added into its reduced
//! pixel there and then, so neither the file's full-size samples nor a full-size linear
//! copy is held. Each pixel is converted before averaging, as a full decode is, so a
//! preview and an export agree.
//!
//! Samples stored plane by plane (a strip of red, then of green, then of blue) are read
//! the same way, a plane at a time; their profile, if any, is then applied once a
//! band's planes are all in. Planes stored in tiles aren't supported (the `tiff` crate
//! sizes their edge tiles wrongly).
//!
//! A reduction by `f` averages `f` × `f` boxes in linear light: the size and the result
//! of halving `f` times as the pyramid does, rounded once instead of at every halving.
//! Bands are whole chunks and whole boxes high, so no chunk is decoded twice.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;

use image_core::icc::Profile;
use image_core::{Cancellation, LinearImage, color};
use rayon::prelude::*;
use tiff::ColorType;
use tiff::decoder::{ChunkType, Decoder as TiffReader, DecodingResult, Limits};
use tiff::tags::Tag;

use crate::DecodeError;
use crate::rendered;

pub(crate) type Reader = TiffReader<BufReader<File>>;

pub(crate) fn corrupt(e: impl std::fmt::Display) -> DecodeError {
    DecodeError::Corrupt(format!("TIFF: {e}"))
}

/// The TIFF at `path`, ready to read its first image. Large scans and stitched
/// panoramas are legitimate: no limit below what the decoded image itself needs.
pub(crate) fn open(path: &Path) -> Result<Reader, DecodeError> {
    let file = BufReader::new(File::open(path)?);
    Ok(TiffReader::new(file)
        .map_err(corrupt)?
        .with_limits(Limits::unlimited()))
}

/// Samples a pixel for the supported colour types (1: grey, 2: grey and alpha, 3: RGB,
/// 4: RGBA), and whether they are 16-bit.
pub(crate) fn channels_of(reader: &mut Reader) -> Result<(usize, bool), DecodeError> {
    match reader.colortype().map_err(corrupt)? {
        ColorType::Gray(b) if b == 8 || b == 16 => Ok((1, b == 16)),
        ColorType::GrayA(b) if b == 8 || b == 16 => Ok((2, b == 16)),
        ColorType::RGB(b) if b == 8 || b == 16 => Ok((3, b == 16)),
        ColorType::RGBA(b) if b == 8 || b == 16 => Ok((4, b == 16)),
        other => Err(DecodeError::Unsupported(format!(
            "TIFF images of type {other:?} are not supported"
        ))),
    }
}

/// What decoding by chunks needs to know of the image.
pub(crate) struct Layout {
    pub width: u32,
    pub height: u32,
    channels: usize,
    sixteen: bool,
    profile: Option<Profile>,
    /// Planes of samples: 1 when stored pixel by pixel, else one for each sample.
    planes: usize,
    /// A chunk's size (the last ones across and down may be cut short) and how many
    /// there are across.
    chunk: (u32, u32),
    across: u32,
}

impl Layout {
    /// The layout of `reader`'s image.
    pub(crate) fn of(reader: &mut Reader) -> Result<Self, DecodeError> {
        let (width, height) = reader.dimensions().map_err(corrupt)?;
        let (channels, sixteen) = channels_of(reader)?;
        if width == 0 || height == 0 {
            return Err(corrupt("an empty image"));
        }
        let planar = reader
            .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
            .map_err(corrupt)?
            == Some(2);
        let tiles = reader.get_chunk_type() == ChunkType::Tile;
        if planar && tiles {
            return Err(DecodeError::Unsupported(
                "TIFF images stored in tiles plane by plane are not supported".into(),
            ));
        }
        let planes = if planar { channels } else { 1 };
        let (cw, ch) = reader.chunk_dimensions();
        let chunk = (cw.clamp(1, width), ch.clamp(1, height));
        let across = width.div_ceil(chunk.0);
        let expected = u64::from(across) * u64::from(height.div_ceil(chunk.1)) * planes as u64;
        let count = if tiles {
            reader.tile_count()
        } else {
            reader.strip_count()
        }
        .map_err(corrupt)?;
        if u64::from(count) < expected {
            return Err(corrupt(format!(
                "{count} strips or tiles where {expected} are needed"
            )));
        }
        let icc = reader.get_tag_u8_vec(Tag::IccProfile).ok();
        Ok(Self {
            width,
            height,
            channels,
            sixteen,
            profile: rendered::profile_for(icc.as_deref(), channels),
            planes,
            chunk,
            across,
        })
    }
}

/// sRGB's curve over every 8- or 16-bit value.
fn srgb_table(sixteen: bool) -> &'static [f32] {
    static EIGHT: OnceLock<Vec<f32>> = OnceLock::new();
    static SIXTEEN: OnceLock<Vec<f32>> = OnceLock::new();
    let (table, n) = if sixteen {
        (&SIXTEEN, 65536)
    } else {
        (&EIGHT, 256)
    };
    table.get_or_init(|| {
        (0..n)
            .map(|v| color::srgb_to_linear(v as f32 / (n - 1) as f32))
            .collect()
    })
}

/// The image at `path`, laid out as `layout`, decoded to linear sRGB and reduced by
/// `factor` (a power of two): `width / factor` × `height / factor`, at least 1 × 1.
pub(crate) fn decode_linear(
    path: &Path,
    layout: &Layout,
    factor: u32,
    cancel: &dyn Cancellation,
) -> Result<LinearImage, DecodeError> {
    let owned;
    let curves: [&[f32]; 3] = match &layout.profile {
        Some(p) => {
            owned = rendered::curve_tables(p, if layout.sixteen { 65536 } else { 256 });
            [&owned[0], &owned[1], &owned[2]]
        }
        None => [srgb_table(layout.sixteen); 3],
    };
    let factor = factor.max(1);
    let out_w = (layout.width / factor).max(1) as usize;
    let out_h = (layout.height / factor).max(1) as usize;
    // Output rows a band: whole chunks and whole boxes high, and enough bands to keep
    // every thread busy.
    let chunk_h = u64::from(layout.chunk.1);
    let unit = (lcm(chunk_h, u64::from(factor)) / u64::from(factor)) as usize;
    let units = out_h.div_ceil(unit.max(1));
    let band_rows = unit * (units / (rayon::current_num_threads() * 4)).max(1);
    let mut data = vec![0u16; out_w * out_h * 3];
    data.par_chunks_mut(out_w * 3 * band_rows)
        .enumerate()
        .try_for_each_init(
            || open(path),
            |reader, (b, out)| {
                let reader = reader
                    .as_mut()
                    .map_err(|e| corrupt(format!("reopening: {e}")))?;
                let band = Band {
                    layout,
                    curves,
                    factor,
                    out_w,
                    first_row: b * band_rows,
                };
                band.decode(reader, out, cancel)
            },
        )?;
    LinearImage::new(out_w as u32, out_h as u32, data)
        .map_err(|e| DecodeError::Internal(format!("unexpected buffer: {e}")))
}

fn lcm(a: u64, b: u64) -> u64 {
    let gcd = |mut a: u64, mut b: u64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    a / gcd(a, b) * b
}

/// One band of output rows and the chunks it is made from.
struct Band<'a> {
    layout: &'a Layout,
    curves: [&'a [f32]; 3],
    factor: u32,
    out_w: usize,
    /// Its first output row.
    first_row: usize,
}

impl Band<'_> {
    fn decode(
        &self,
        reader: &mut Reader,
        out: &mut [u16],
        cancel: &dyn Cancellation,
    ) -> Result<(), DecodeError> {
        let l = self.layout;
        let f = self.factor as usize;
        let rows = out.len() / (self.out_w * 3);
        let (width, height) = (l.width as usize, l.height as usize);
        // The source rows it covers.
        let top = self.first_row * f;
        let bottom = ((self.first_row + rows) * f).min(height);
        let (cw, ch) = (l.chunk.0 as usize, l.chunk.1 as usize);
        let across = l.across as usize;
        let per_plane = across * height.div_ceil(ch);
        // Grey needs only its first plane; alpha is never read.
        let planes = if l.channels < 3 { 1 } else { l.planes.min(3) };
        let mut sums = vec![0.0f32; out.len()];
        let mut chunks = Vec::with_capacity(planes);
        for cy in top / ch..bottom.div_ceil(ch) {
            for cx in 0..across {
                let index = cy * across + cx;
                let (dw, dh) = reader.chunk_data_dimensions(index as u32);
                let (dw, dh) = (dw as usize, dh as usize);
                chunks.clear();
                for p in 0..planes {
                    if cancel.is_cancelled() {
                        return Err(DecodeError::Cancelled);
                    }
                    let at = (index + p * per_plane) as u32;
                    chunks.push(reader.read_chunk(at).map_err(corrupt)?);
                }
                let (x0, y0) = (cx * cw, cy * ch);
                let rows_in = top.max(y0)..bottom.min(y0 + dh);
                let place = (x0, y0, dw);
                if let Some(bufs) = samples(&chunks, |c| match c {
                    DecodingResult::U8(v) => Some(&v[..]),
                    _ => None,
                }) {
                    self.add(&mut sums, &bufs, place, rows_in, usize::from);
                } else if let Some(bufs) = samples(&chunks, |c| match c {
                    DecodingResult::U16(v) => Some(&v[..]),
                    _ => None,
                }) {
                    self.add(&mut sums, &bufs, place, rows_in, usize::from);
                } else {
                    return Err(DecodeError::Unsupported("TIFF sample format".into()));
                }
            }
        }
        // Each reduced pixel: the mean of its box.
        let span = |o: usize, size: usize| ((o + 1) * f).min(size) - o * f;
        for (r, row) in out.chunks_exact_mut(self.out_w * 3).enumerate() {
            let n_y = span(self.first_row + r, height);
            for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let n = (span(x, width) * n_y) as f32;
                let at = (r * self.out_w + x) * 3;
                *px = rendered::quantise([0, 1, 2].map(|c| sums[at + c] / n));
            }
        }
        Ok(())
    }

    /// Adds the chunk at (`x0`, `y0`), `dw` pixels wide, into `sums`: its rows `rows`
    /// (image rows) made linear sRGB, each pixel into its reduced pixel. `bufs` holds
    /// its samples: one buffer of whole pixels, or one plane each for red, green and
    /// blue.
    fn add<T: Copy>(
        &self,
        sums: &mut [f32],
        bufs: &[&[T]],
        (x0, y0, dw): (usize, usize, usize),
        rows: std::ops::Range<usize>,
        index: impl Fn(T) -> usize,
    ) {
        let l = self.layout;
        let shift = self.factor.trailing_zeros();
        // Only pixels in whole boxes count (the rest are cut off, as halving does),
        // unless the image is smaller than one box.
        let used_w = (self.out_w << shift).min(l.width as usize);
        let n = (x0 + dw).min(used_w).saturating_sub(x0);
        if n == 0 {
            return;
        }
        // Where each of red, green and blue is: its buffer, and its offset in a pixel.
        let planar = bufs.len() > 1;
        let stride = if planar { 1 } else { l.channels };
        let source = |c: usize| match (l.channels < 3, planar) {
            (true, _) => (0, 0),
            (false, true) => (c, 0),
            (false, false) => (0, c),
        };
        let [t0, t1, t2] = self.curves;
        for y in rows {
            let out_row = ((y >> shift) - self.first_row) * self.out_w * 3;
            let start = (y - y0) * dw * stride;
            let [r, g, b] = [0, 1, 2].map(|c| {
                let (buf, offset) = source(c);
                &bufs[buf][start + offset..]
            });
            for i in 0..n {
                let j = i * stride;
                let lin = [t0[index(r[j])], t1[index(g[j])], t2[index(b[j])]];
                let px = match &l.profile {
                    Some(p) => p.to_srgb(lin).map(|v| v.clamp(0.0, 1.0)),
                    None => lin,
                };
                let at = out_row + ((x0 + i) >> shift) * 3;
                sums[at] += px[0];
                sums[at + 1] += px[1];
                sums[at + 2] += px[2];
            }
        }
    }
}

/// The chunks' samples, if they are all of one type.
fn samples<'a, T>(
    chunks: &'a [DecodingResult],
    of: impl Fn(&'a DecodingResult) -> Option<&'a [T]>,
) -> Option<Vec<&'a [T]>> {
    chunks.iter().map(of).collect()
}

#[cfg(test)]
mod tests;
