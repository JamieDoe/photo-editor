//! Output sharpening (ADR 0059): a final, light sharpening for the medium a photo is
//! exported for, done at the output size after any resize, as the design's "Sharpen
//! for" row offers (Screen, Matte or Glossy paper).
//!
//! An unsharp mask on brightness only: the display-encoded luma less its Gaussian
//! blur is the detail, softly cut below a small threshold (so flat areas and noise
//! are left alone), and the same amount is added to each channel (so colours keep
//! their hue). Screen is fine and light; prints get a wider radius, for 300 ppi, and
//! matte paper more than glossy, as it softens more.

use image_core::{OutputImage, PixelFormat};
use rayon::prelude::*;

/// What an export is sharpened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputSharpening {
    /// Not at all: for files that will be edited further.
    None,
    /// Viewed on screen at its own size.
    #[default]
    Screen,
    /// Printed on matte paper.
    Matte,
    /// Printed on glossy paper.
    Glossy,
}

/// The mask: the blur's sigma (pixels), how much detail is added, and the detail
/// (in 0..1 encoded units) below which it fades out.
struct Mask {
    sigma: f32,
    amount: f32,
    threshold: f32,
}

impl OutputSharpening {
    fn mask(self) -> Option<Mask> {
        match self {
            Self::None => None,
            Self::Screen => Some(Mask {
                sigma: 0.6,
                amount: 0.45,
                threshold: 0.01,
            }),
            Self::Matte => Some(Mask {
                sigma: 1.1,
                amount: 0.9,
                threshold: 0.008,
            }),
            Self::Glossy => Some(Mask {
                sigma: 0.9,
                amount: 0.6,
                threshold: 0.008,
            }),
        }
    }
}

/// `image` sharpened for `medium` (the same image for [`OutputSharpening::None`]).
///
/// Memory stays at two single-channel float planes besides the output: luma, and its
/// horizontal blur; the vertical blur is taken as each output row is written.
pub fn sharpen(image: &OutputImage, medium: OutputSharpening) -> OutputImage {
    let Some(mask) = medium.mask() else {
        return image.clone();
    };
    let (w, h) = (image.width() as usize, image.height() as usize);
    let format = image.format();
    let bpp = format.bytes_per_pixel();
    let wide = format == PixelFormat::Rgb16;
    // One sample as 0..1, and back.
    let read = move |b: &[u8], c: usize| -> f32 {
        if wide {
            f32::from(u16::from_ne_bytes([b[c * 2], b[c * 2 + 1]])) / 65535.0
        } else {
            f32::from(b[c]) / 255.0
        }
    };
    let luma: Vec<f32> = image
        .data()
        .par_chunks(bpp)
        .map(|p| 0.2126 * read(p, 0) + 0.7152 * read(p, 1) + 0.0722 * read(p, 2))
        .collect();
    let (kernel, r) = gaussian_kernel(mask.sigma);
    let mut across = vec![0.0f32; w * h];
    across
        .par_chunks_mut(w)
        .zip(luma.par_chunks(w))
        .for_each(|(out, row)| {
            for (x, o) in out.iter_mut().enumerate() {
                *o = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, wt)| {
                        wt * row[(x as isize + k as isize - r).clamp(0, w as isize - 1) as usize]
                    })
                    .sum();
            }
        });
    let mut out = image.clone();
    out.data_mut()
        .par_chunks_mut(w * bpp)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_mut(bpp).enumerate() {
                let blurred: f32 = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, wt)| {
                        let yy = (y as isize + k as isize - r).clamp(0, h as isize - 1) as usize;
                        wt * across[yy * w + x]
                    })
                    .sum();
                let d = luma[y * w + x] - blurred;
                // Fades in from the threshold to twice it: no hard step.
                let t = ((d.abs() - mask.threshold) / mask.threshold).clamp(0.0, 1.0);
                let d = d * t * t * (3.0 - 2.0 * t) * mask.amount;
                if d == 0.0 {
                    continue;
                }
                // The same added to each colour channel (alpha, if any, untouched).
                for c in 0..3 {
                    let v = (read(px, c) + d).clamp(0.0, 1.0);
                    if wide {
                        px[c * 2..c * 2 + 2]
                            .copy_from_slice(&((v * 65535.0).round() as u16).to_ne_bytes());
                    } else {
                        px[c] = (v * 255.0).round() as u8;
                    }
                }
            }
        });
    out
}

/// A normalised Gaussian kernel for `sigma`, and its radius.
fn gaussian_kernel(sigma: f32) -> (Vec<f32>, isize) {
    let r = (sigma * 3.0).ceil() as isize;
    let kernel: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = kernel.iter().sum();
    (kernel.iter().map(|k| k / total).collect(), r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vertical edge from `dark` to `light` (8-bit grey), 40 x 8.
    fn edge(dark: u8, light: u8) -> OutputImage {
        let data = (0..8 * 40)
            .flat_map(|i| [if i % 40 < 20 { dark } else { light }; 3])
            .collect();
        OutputImage::from_raw(40, 8, PixelFormat::Rgb8, data).unwrap()
    }

    fn row(img: &OutputImage) -> Vec<u8> {
        img.data()[..40 * 3].iter().step_by(3).copied().collect()
    }

    #[test]
    fn edges_get_crisper_and_flat_areas_stay() {
        let img = edge(80, 160);
        for medium in [
            OutputSharpening::Screen,
            OutputSharpening::Matte,
            OutputSharpening::Glossy,
        ] {
            let out = row(&sharpen(&img, medium));
            // Darker just before the edge, lighter just after: overshoot, a crisper edge.
            assert!(out[19] < 80 && out[20] > 160, "{medium:?}: {out:?}");
            // Far from it, untouched.
            assert_eq!((out[2], out[37]), (80, 160), "{medium:?}");
        }
        // Prints are sharpened more (and wider) than screens; matte most.
        let reach = |m| {
            let out = row(&sharpen(&img, m));
            80 - i32::from(out[19]) + i32::from(out[20]) - 160
        };
        assert!(reach(OutputSharpening::Matte) > reach(OutputSharpening::Glossy));
        assert!(reach(OutputSharpening::Glossy) > reach(OutputSharpening::Screen));
    }

    #[test]
    fn none_noise_and_colour() {
        let img = edge(80, 160);
        assert!(sharpen(&img, OutputSharpening::None) == img);
        // Faint grain (a level or two) is below the threshold: left alone.
        let grain = edge(120, 121);
        assert!(sharpen(&grain, OutputSharpening::Screen) == grain);
        // A coloured edge keeps its colours' differences: the same is added to each.
        let data: Vec<u8> = (0..8 * 40)
            .flat_map(|i| {
                if i % 40 < 20 {
                    [100, 60, 40]
                } else {
                    [200, 160, 140]
                }
            })
            .collect();
        let img = OutputImage::from_raw(40, 8, PixelFormat::Rgb8, data).unwrap();
        let out = sharpen(&img, OutputSharpening::Matte);
        let p = &out.data()[19 * 3..19 * 3 + 3];
        assert_eq!(i32::from(p[0]) - i32::from(p[1]), 40, "{p:?}");
    }

    #[test]
    fn sixteen_bit_images_are_sharpened_at_their_depth() {
        let data: Vec<u8> = (0..8 * 40)
            .flat_map(|i| [if i % 40 < 20 { 20000u16 } else { 40000 }; 3])
            .flat_map(u16::to_ne_bytes)
            .collect();
        let img = OutputImage::from_raw(40, 8, PixelFormat::Rgb16, data).unwrap();
        let out = sharpen(&img, OutputSharpening::Glossy);
        assert_eq!(out.format(), PixelFormat::Rgb16);
        let s = out.samples16().unwrap();
        assert!(s[19 * 3] < 20000 && s[20 * 3] > 40000);
        assert_eq!(s[2 * 3], 20000);
    }
}
