//! Downscaling of display-encoded images (thumbnails).

use crate::{OutputImage, PixelFormat};

/// Downscales `image` so its long edge is at most `long_edge`, keeping the aspect
/// ratio. Uses area averaging (each output pixel is the coverage-weighted mean of the
/// source pixels under it), which is alias-free for any ratio. Images that already fit
/// are returned unchanged.
///
/// Averaging happens on the 8-bit display-encoded values. That is slightly darker
/// than averaging linear light on fine high-contrast detail, which is acceptable for
/// thumbnails; editing never goes through this path.
pub fn fit_long_edge(image: OutputImage, long_edge: u32) -> OutputImage {
    let (w, h) = (image.width(), image.height());
    let long_edge = long_edge.max(1);
    if w.max(h) <= long_edge {
        return image;
    }
    let (dw, dh) = if w >= h {
        (long_edge, scaled(h, long_edge, w))
    } else {
        (scaled(w, long_edge, h), long_edge)
    };
    let format = image.format();
    let ch = format.channels();

    // Horizontal pass: w x h -> dw x h, then vertical: dw x h -> dw x dh.
    let xs = weights(w, dw);
    let mut mid = vec![0f32; dw as usize * h as usize * ch];
    for (src_row, mid_row) in image
        .data()
        .chunks_exact(w as usize * ch)
        .zip(mid.chunks_exact_mut(dw as usize * ch))
    {
        for (x, (start, ws)) in xs.iter().enumerate() {
            let out = &mut mid_row[x * ch..(x + 1) * ch];
            for (k, wt) in ws.iter().enumerate() {
                let s = (start + k) * ch;
                for c in 0..ch {
                    out[c] += wt * f32::from(src_row[s + c]);
                }
            }
        }
    }

    let ys = weights(h, dh);
    let row_len = dw as usize * ch;
    let mut data = vec![0u8; row_len * dh as usize];
    let mut acc = vec![0f32; row_len];
    for (y, (start, ws)) in ys.iter().enumerate() {
        acc.fill(0.0);
        for (k, wt) in ws.iter().enumerate() {
            let row = &mid[(start + k) * row_len..(start + k + 1) * row_len];
            for (a, v) in acc.iter_mut().zip(row) {
                *a += wt * v;
            }
        }
        for (d, a) in data[y * row_len..(y + 1) * row_len].iter_mut().zip(&acc) {
            *d = a.round().clamp(0.0, 255.0) as u8;
        }
    }
    if format == PixelFormat::Rgba8 {
        // Sources are opaque; keep alpha exact regardless of rounding.
        for px in data.as_chunks_mut::<4>().0 {
            px[3] = u8::MAX;
        }
    }
    OutputImage::from_raw(dw, dh, format, data).expect("dimensions match the buffer")
}

/// `short * long_edge / long`, rounded, at least 1.
fn scaled(short: u32, long_edge: u32, long: u32) -> u32 {
    ((u64::from(short) * u64::from(long_edge) + u64::from(long) / 2) / u64::from(long)).max(1)
        as u32
}

/// For each output index: the first source index and the normalised coverage weights
/// of the source pixels it spans.
fn weights(src: u32, dst: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = f64::from(src) / f64::from(dst);
    (0..dst)
        .map(|i| {
            let (lo, hi) = (f64::from(i) * scale, f64::from(i + 1) * scale);
            let first = lo.floor() as usize;
            let last = (hi.ceil() as usize).min(src as usize);
            let ws = (first..last)
                .map(|j| {
                    let overlap = hi.min(j as f64 + 1.0) - lo.max(j as f64);
                    (overlap / scale) as f32
                })
                .collect();
            (first, ws)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, values: &[u8]) -> OutputImage {
        let data = values.iter().flat_map(|&v| [v, v, v]).collect();
        OutputImage::from_raw(w, h, PixelFormat::Rgb8, data).unwrap()
    }

    fn reds(img: &OutputImage) -> Vec<u8> {
        img.data()
            .chunks_exact(img.format().channels())
            .map(|p| p[0])
            .collect()
    }

    #[test]
    fn keeps_aspect_ratio_for_both_orientations() {
        let landscape = OutputImage::new(1616, 1080, PixelFormat::Rgba8).unwrap();
        let out = fit_long_edge(landscape, 512);
        assert_eq!((out.width(), out.height()), (512, 342));
        let portrait = OutputImage::new(1080, 1616, PixelFormat::Rgb8).unwrap();
        let out = fit_long_edge(portrait, 512);
        assert_eq!((out.width(), out.height()), (342, 512));
    }

    #[test]
    fn images_that_fit_are_returned_unchanged() {
        let img = gray(3, 2, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(fit_long_edge(img.clone(), 3), img);
        assert_eq!(fit_long_edge(img.clone(), 512), img);
    }

    #[test]
    fn integer_ratio_is_a_box_average() {
        let img = gray(4, 2, &[0, 100, 200, 250, 100, 100, 0, 50]);
        let out = fit_long_edge(img, 2);
        assert_eq!((out.width(), out.height()), (2, 1));
        assert_eq!(reds(&out), [75, 125]);
    }

    #[test]
    fn fractional_ratio_weights_partial_coverage() {
        // 3 -> 2: each output covers 1.5 source pixels.
        let img = gray(3, 1, &[0, 90, 180]);
        let out = fit_long_edge(img, 2);
        assert_eq!(reds(&out), [30, 150]);
    }

    #[test]
    fn uniform_colour_and_opaque_alpha_survive() {
        let mut img = OutputImage::new(97, 61, PixelFormat::Rgba8).unwrap();
        for px in img.data_mut().as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[12, 200, 77, 255]);
        }
        let out = fit_long_edge(img, 40);
        assert_eq!((out.width(), out.height()), (40, 25));
        assert!(
            out.data()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [12, 200, 77, 255])
        );
    }
}
