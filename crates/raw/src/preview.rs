//! Helpers for embedded (camera-rendered) previews: 8-bit sRGB, display only.
//!
//! These images are placeholders shown while the real decode runs, never inputs to
//! editing, so simple 8-bit processing is acceptable here.

use image_core::{OutputImage, PixelFormat};

use crate::DecodeError;

/// Interleaved 8-bit RGB.
pub(crate) struct Rgb8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Decodes an embedded JPEG towards `min_long_edge`, returning the image and the
/// JPEG's full dimensions. With libjpeg-turbo this uses DCT-domain scaling (1/8..1/1),
/// so a full-size embedded JPEG is never decoded at full size.
#[cfg(feature = "turbojpeg")]
pub(crate) fn decode_jpeg(
    bytes: &[u8],
    min_long_edge: u32,
) -> Result<(Rgb8, (u32, u32)), DecodeError> {
    let d = jpeg_turbo::decode_scaled(bytes, min_long_edge)
        .map_err(|e| DecodeError::Corrupt(format!("embedded JPEG: {e}")))?;
    let rgb = Rgb8 {
        width: d.image.width,
        height: d.image.height,
        data: d.image.data,
    };
    Ok((rgb, (d.source_width, d.source_height)))
}

/// Fallback without libjpeg-turbo: full-size decode (the caller box-downscales).
#[cfg(not(feature = "turbojpeg"))]
pub(crate) fn decode_jpeg(
    bytes: &[u8],
    _min_long_edge: u32,
) -> Result<(Rgb8, (u32, u32)), DecodeError> {
    use std::io::Cursor;
    use zune_jpeg::JpegDecoder as ZuneDecoder;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;

    let opts = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(1 << 16)
        .set_max_height(1 << 16);
    let mut decoder = ZuneDecoder::new_with_options(Cursor::new(bytes), opts);
    let data = decoder
        .decode()
        .map_err(|e| DecodeError::Corrupt(format!("embedded JPEG: {e:?}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| DecodeError::Internal("missing JPEG info".into()))?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    Ok((
        Rgb8 {
            width: w,
            height: h,
            data,
        },
        (w, h),
    ))
}

/// Halves both dimensions with a 2x2 box filter.
pub(crate) fn downsample_2x(src: &Rgb8) -> Rgb8 {
    let (w, h) = (
        (src.width / 2).max(1) as usize,
        (src.height / 2).max(1) as usize,
    );
    let sw = src.width as usize;
    let mut data = vec![0u8; w * h * 3];
    for y in 0..h {
        let (r0, r1) = (
            (y * 2).min(src.height as usize - 1),
            (y * 2 + 1).min(src.height as usize - 1),
        );
        for x in 0..w {
            let (c0, c1) = ((x * 2).min(sw - 1), (x * 2 + 1).min(sw - 1));
            for c in 0..3 {
                let s = u16::from(src.data[(r0 * sw + c0) * 3 + c])
                    + u16::from(src.data[(r0 * sw + c1) * 3 + c])
                    + u16::from(src.data[(r1 * sw + c0) * 3 + c])
                    + u16::from(src.data[(r1 * sw + c1) * 3 + c]);
                data[(y * w + x) * 3 + c] = ((s + 2) / 4) as u8;
            }
        }
    }
    Rgb8 {
        width: w as u32,
        height: h as u32,
        data,
    }
}

/// Applies a dcraw/LibRaw `flip` value (bit 2: transpose, bit 1: flip rows, bit 0:
/// flip columns; 3 = 180°, 5 = 90° CCW, 6 = 90° CW) and converts to opaque RGBA8.
pub(crate) fn orient_to_rgba(src: &Rgb8, flip: i32) -> OutputImage {
    let (iw, ih) = (src.width as usize, src.height as usize);
    let transpose = flip & 4 != 0;
    let (ow, oh) = if transpose { (ih, iw) } else { (iw, ih) };
    let mut out =
        OutputImage::new(ow as u32, oh as u32, PixelFormat::Rgba8).expect("non-empty preview");
    let dst = out.data_mut();
    for r in 0..oh {
        for c in 0..ow {
            let (mut sr, mut sc) = if transpose { (c, r) } else { (r, c) };
            if flip & 2 != 0 {
                sr = ih - 1 - sr;
            }
            if flip & 1 != 0 {
                sc = iw - 1 - sc;
            }
            let s = (sr * iw + sc) * 3;
            let d = (r * ow + c) * 4;
            dst[d..d + 3].copy_from_slice(&src.data[s..s + 3]);
            dst[d + 3] = u8::MAX;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3x2 image whose red channel encodes the pixel index.
    fn indexed() -> Rgb8 {
        Rgb8 {
            width: 3,
            height: 2,
            data: (0..6u8).flat_map(|i| [i, 0, 0]).collect(),
        }
    }

    fn reds(img: &OutputImage) -> Vec<u8> {
        img.data().as_chunks::<4>().0.iter().map(|p| p[0]).collect()
    }

    // Source:  0 1 2
    //          3 4 5
    #[test]
    fn orientation_follows_dcraw_flip_semantics() {
        let src = indexed();
        let none = orient_to_rgba(&src, 0);
        assert_eq!(
            (none.width(), none.height(), reds(&none)),
            (3, 2, vec![0, 1, 2, 3, 4, 5])
        );
        let r180 = orient_to_rgba(&src, 3);
        assert_eq!(reds(&r180), vec![5, 4, 3, 2, 1, 0]);
        // 90° clockwise: the old bottom-left becomes top-left.
        let cw = orient_to_rgba(&src, 6);
        assert_eq!(
            (cw.width(), cw.height(), reds(&cw)),
            (2, 3, vec![3, 0, 4, 1, 5, 2])
        );
        // 90° counter-clockwise: the old top-right becomes top-left.
        let ccw = orient_to_rgba(&src, 5);
        assert_eq!(
            (ccw.width(), ccw.height(), reds(&ccw)),
            (2, 3, vec![2, 5, 1, 4, 0, 3])
        );
        assert!(none.data().as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn downsample_averages_2x2_blocks() {
        let src = Rgb8 {
            width: 2,
            height: 2,
            data: vec![0, 0, 0, 100, 100, 100, 200, 200, 200, 100, 100, 100],
        };
        let d = downsample_2x(&src);
        assert_eq!((d.width, d.height, d.data), (1, 1, vec![100, 100, 100]));
    }
}
