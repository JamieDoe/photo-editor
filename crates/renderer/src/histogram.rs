//! The histogram of a rendered frame (ADR 0036): what the panel's graph draws.
//!
//! Counted on the display-encoded output, as the photo appears on screen: red, green
//! and blue per channel, and luminance (Rec. 709 weights on the encoded values).

use image_core::OutputImage;
use rayon::prelude::*;

pub const BINS: usize = 256;

/// Pixel counts per 8-bit value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub red: [u32; BINS],
    pub green: [u32; BINS],
    pub blue: [u32; BINS],
    pub luma: [u32; BINS],
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            red: [0; BINS],
            green: [0; BINS],
            blue: [0; BINS],
            luma: [0; BINS],
        }
    }
}

/// Pixels per parallel work item.
const CHUNK_PIXELS: usize = 64 * 1024;

impl Histogram {
    /// Counts every pixel of `image` (alpha ignored). For the 8-bit frames shown on
    /// screen; 16-bit export images are never counted.
    pub fn of(image: &OutputImage) -> Self {
        debug_assert_eq!(image.format().bytes_per_sample(), 1, "8-bit images only");
        let channels = image.format().channels();
        image
            .data()
            .par_chunks(CHUNK_PIXELS * channels)
            .fold(Self::default, |mut h, chunk| {
                for px in chunk.chunks_exact(channels) {
                    let (r, g, b) = (px[0], px[1], px[2]);
                    h.red[usize::from(r)] += 1;
                    h.green[usize::from(g)] += 1;
                    h.blue[usize::from(b)] += 1;
                    // 0.2126, 0.7152, 0.0722 in 256ths (54 + 183 + 19 = 256).
                    let y = (54 * u32::from(r) + 183 * u32::from(g) + 19 * u32::from(b) + 128) >> 8;
                    h.luma[y as usize] += 1;
                }
                h
            })
            .reduce(Self::default, |mut a, b| {
                for (x, y) in [
                    (&mut a.red, &b.red),
                    (&mut a.green, &b.green),
                    (&mut a.blue, &b.blue),
                    (&mut a.luma, &b.luma),
                ] {
                    x.iter_mut().zip(y).for_each(|(p, q)| *p += q);
                }
                a
            })
    }

    /// Pixels counted.
    pub fn total(&self) -> u64 {
        self.luma.iter().map(|&n| u64::from(n)).sum()
    }

    /// Appends red, green, blue and luminance counts as little-endian `u32`s
    /// (`4 * BINS * 4` bytes).
    pub fn write_le(&self, out: &mut Vec<u8>) {
        for plane in [&self.red, &self.green, &self.blue, &self.luma] {
            for n in plane {
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
    }
}

/// Bytes [`Histogram::write_le`] appends.
pub const ENCODED_BYTES: usize = 4 * BINS * 4;

#[cfg(test)]
mod tests {
    use super::*;
    use image_core::PixelFormat;

    #[test]
    fn counts_channels_and_luminance() {
        // Two white, one black, one pure red pixel.
        let data = vec![
            255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255,
        ];
        let mut img = OutputImage::new(2, 2, PixelFormat::Rgba8).unwrap();
        img.data_mut().copy_from_slice(&data);
        let h = Histogram::of(&img);
        assert_eq!(h.total(), 4);
        assert_eq!((h.red[255], h.red[0]), (3, 1));
        assert_eq!((h.green[255], h.green[0]), (2, 2));
        assert_eq!((h.luma[255], h.luma[0], h.luma[54]), (2, 1, 1));
        let mut bytes = Vec::new();
        h.write_le(&mut bytes);
        assert_eq!(bytes.len(), ENCODED_BYTES);
        // Red's count for 255 sits at index 255 of the first plane.
        assert_eq!(&bytes[255 * 4..256 * 4], &3u32.to_le_bytes());
    }

    #[test]
    fn parallel_chunks_add_up() {
        let (w, h) = (700u32, 300u32);
        let mut img = OutputImage::new(w, h, PixelFormat::Rgb8).unwrap();
        img.data_mut()
            .iter_mut()
            .enumerate()
            .for_each(|(i, v)| *v = (i % 251) as u8);
        let hist = Histogram::of(&img);
        assert_eq!(hist.total(), u64::from(w * h));
        assert_eq!(
            hist.red.iter().map(|&n| u64::from(n)).sum::<u64>(),
            u64::from(w * h)
        );
    }
}
