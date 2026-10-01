//! Downscaling for sized exports (ADR 0050): each output pixel is the average of the
//! source area it covers (partial pixels weighted by how much they cover), in linear
//! light, so fine detail keeps its brightness. Two separable passes, rows in parallel.

use image_core::color::{linear_to_srgb, srgb_to_linear, srgb8_to_linear16_table};
use image_core::{OutputImage, PixelFormat};
use rayon::prelude::*;

/// The size `width` x `height` becomes with its long edge at most `long_edge`: the
/// same when it is already that small (exports never enlarge).
pub fn fitted_size(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long_edge == 0 || long <= long_edge {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(long);
    let fit = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    if width >= height {
        (long_edge, fit(height))
    } else {
        (fit(width), long_edge)
    }
}

/// For each output index, the source pixels it covers and their weights (summing to 1).
fn coverage(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = from as f64 / to as f64;
    (0..to)
        .map(|o| {
            let (start, end) = (o as f64 * scale, (o + 1) as f64 * scale);
            let mut taps = Vec::new();
            let mut i = start.floor() as usize;
            while (i as f64) < end && i < from {
                let w = (end.min(i as f64 + 1.0) - start.max(i as f64)) / scale;
                if w > 0.0 {
                    taps.push((i, w as f32));
                }
                i += 1;
            }
            taps
        })
        .collect()
}

/// `image` (RGB8, RGBA8 or 16-bit RGB, sRGB) with its long edge at most `long_edge`.
pub fn fit_long_edge(image: &OutputImage, long_edge: u32) -> OutputImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let (ow, oh) = fitted_size(image.width(), image.height(), long_edge);
    if (ow as usize, oh as usize) == (w, h) {
        return image.clone();
    }
    let (ow, oh) = (ow as usize, oh as usize);
    let format = image.format();
    let ch = format.channels();
    // Samples into linear light (alpha, a coverage, as it is), and back.
    let linear = if format == PixelFormat::Rgb16 {
        let samples = image.samples16().unwrap_or_default();
        let table: Vec<f32> = (0..=u16::MAX)
            .map(|v| srgb_to_linear(f32::from(v) / 65535.0))
            .collect();
        resample(&samples, w, h, ow, oh, ch, |v: u16, _| {
            table[usize::from(v)]
        })
    } else {
        let to_linear: Vec<f32> = srgb8_to_linear16_table()
            .iter()
            .map(|&v| f32::from(v) / 65535.0)
            .collect();
        resample(image.data(), w, h, ow, oh, ch, |v: u8, c| {
            if c == 3 {
                f32::from(v) / 255.0
            } else {
                to_linear[usize::from(v)]
            }
        })
    };
    let data = if format == PixelFormat::Rgb16 {
        let mut data = vec![0u8; ow * oh * 6];
        data.par_chunks_mut(6 * 1024)
            .zip(linear.par_chunks(3 * 1024))
            .for_each(|(out, values)| {
                for (o, v) in out.as_chunks_mut::<2>().0.iter_mut().zip(values) {
                    let code = (linear_to_srgb(v.clamp(0.0, 1.0)) * 65535.0).round() as u16;
                    *o = code.to_ne_bytes();
                }
            });
        data
    } else {
        // Back to 8 bits through a fine table: 4096 steps are well under an 8-bit step.
        const STEPS: usize = 4096;
        let to_srgb: Vec<u8> = (0..=STEPS)
            .map(|i| {
                (linear_to_srgb(i as f32 / STEPS as f32) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect();
        linear
            .par_iter()
            .enumerate()
            .map(|(i, v)| {
                let v = v.clamp(0.0, 1.0);
                if i % ch == 3 {
                    (v * 255.0).round() as u8
                } else {
                    to_srgb[(v * STEPS as f32).round() as usize]
                }
            })
            .collect()
    };
    OutputImage::from_raw(ow as u32, oh as u32, format, data).unwrap_or_else(|_| image.clone())
}

/// `src` (`w` x `h`, `ch` samples a pixel) averaged down to `ow` x `oh` in linear
/// light, each sample made linear by `to_linear(sample, channel)`: rows first, then
/// columns, in parallel.
fn resample<T: Copy + Sync>(
    src: &[T],
    w: usize,
    h: usize,
    ow: usize,
    oh: usize,
    ch: usize,
    to_linear: impl Fn(T, usize) -> f32 + Sync,
) -> Vec<f32> {
    let across = coverage(w, ow);
    let mut rows = vec![0f32; h * ow * ch];
    rows.par_chunks_mut(ow * ch)
        .enumerate()
        .for_each(|(y, out)| {
            let line = &src[y * w * ch..(y + 1) * w * ch];
            for (x, taps) in across.iter().enumerate() {
                for c in 0..ch {
                    out[x * ch + c] = taps
                        .iter()
                        .map(|&(i, wt)| wt * to_linear(line[i * ch + c], c))
                        .sum();
                }
            }
        });
    let down = coverage(h, oh);
    let mut out = vec![0f32; ow * oh * ch];
    out.par_chunks_mut(ow * ch)
        .enumerate()
        .for_each(|(y, line)| {
            for (i, v) in line.iter_mut().enumerate() {
                *v = down[y]
                    .iter()
                    .map(|&(r, wt)| wt * rows[r * ow * ch + i])
                    .sum();
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_core::PixelFormat;

    #[test]
    fn sizes_fit_the_long_edge_and_never_grow() {
        assert_eq!(fitted_size(6000, 4000, 2048), (2048, 1365));
        assert_eq!(fitted_size(4000, 6000, 1350), (900, 1350));
        assert_eq!(fitted_size(1200, 800, 2048), (1200, 800));
        assert_eq!(fitted_size(1200, 800, 0), (1200, 800));
    }

    fn solid(w: u32, h: u32, v: u8) -> OutputImage {
        OutputImage::from_raw(w, h, PixelFormat::Rgb8, vec![v; (w * h * 3) as usize]).unwrap()
    }

    #[test]
    fn flat_areas_keep_their_value() {
        let out = fit_long_edge(&solid(301, 200, 137), 100);
        assert_eq!((out.width(), out.height()), (100, 66));
        assert!(
            out.data().iter().all(|&v| v == 137),
            "{:?}",
            &out.data()[..9]
        );
    }

    #[test]
    fn fine_detail_averages_in_linear_light() {
        // Alternating black and white columns, halved: each output pixel covers one of
        // each. In linear light that is half the light, sRGB 188, not 128.
        let (w, h) = (200u32, 2u32);
        let data: Vec<u8> = (0..w * h)
            .flat_map(|i| [if i % 2 == 0 { 0 } else { 255 }; 3])
            .collect();
        let img = OutputImage::from_raw(w, h, PixelFormat::Rgb8, data).unwrap();
        let out = fit_long_edge(&img, 100);
        assert_eq!((out.width(), out.height()), (100, 1));
        assert!(
            out.data().iter().all(|&v| (187..=189).contains(&v)),
            "{:?}",
            &out.data()[..6]
        );
    }

    #[test]
    fn small_images_are_unchanged() {
        let img = solid(50, 40, 90);
        assert_eq!(fit_long_edge(&img, 2048).data(), img.data());
    }

    #[test]
    fn sixteen_bit_images_resize_in_linear_light_keeping_their_depth() {
        // A flat colour stays exactly that colour; a fine checker of black and white
        // averages to linear mid grey (sRGB 0.735), not sRGB 0.5.
        let flat: Vec<u8> = (0..64 * 64)
            .flat_map(|_| [40000u16, 1234, 65535])
            .flat_map(u16::to_ne_bytes)
            .collect();
        let img = OutputImage::from_raw(64, 64, PixelFormat::Rgb16, flat).unwrap();
        let small = fit_long_edge(&img, 16);
        assert_eq!(
            (small.width(), small.height(), small.format()),
            (16, 16, PixelFormat::Rgb16)
        );
        for px in small.samples16().unwrap().chunks(3) {
            assert!(
                (i32::from(px[0]) - 40000).abs() <= 2 && (i32::from(px[1]) - 1234).abs() <= 2,
                "{px:?}"
            );
        }
        let checker: Vec<u8> = (0..64 * 64)
            .flat_map(|i| {
                [if (i % 64 + i / 64) % 2 == 0 {
                    0u16
                } else {
                    65535
                }; 3]
            })
            .flat_map(u16::to_ne_bytes)
            .collect();
        let img = OutputImage::from_raw(64, 64, PixelFormat::Rgb16, checker).unwrap();
        let grey = fit_long_edge(&img, 8).samples16().unwrap()[0];
        assert!((f32::from(grey) / 65535.0 - 0.735).abs() < 0.005, "{grey}");
    }
}
