//! Minimal safe bindings to libjpeg-turbo (TurboJPEG 3), shared by export (encode)
//! and the RAW decoder (embedded preview decode). See docs/ADR/0006-jpeg-encoder.md.
//!
//! libjpeg-turbo: BSD-3-Clause / IJG / zlib licences; linked dynamically.

use std::ffi::CStr;
use std::fmt;
use std::os::raw::{c_char, c_int};

unsafe extern "C" {
    fn pe_tj_encode(
        pixels: *const u8,
        width: c_int,
        height: c_int,
        channels: c_int,
        quality: c_int,
        out: *mut *mut u8,
        out_len: *mut usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn pe_tj_free(buf: *mut u8);
    fn pe_tj_scaled_size(
        jpeg: *const u8,
        len: usize,
        min_long_edge: u32,
        src_w: *mut u32,
        src_h: *mut u32,
        out_w: *mut u32,
        out_h: *mut u32,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn pe_tj_decode_scaled(
        jpeg: *const u8,
        len: usize,
        min_long_edge: u32,
        dst: *mut u8,
        dst_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
}

/// Upper bound on decoded output, guarding against hostile headers in embedded data.
const MAX_OUTPUT_PIXELS: u64 = 200_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "libjpeg-turbo: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// Interleaved 8-bit RGB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Result of [`decode_scaled`].
#[derive(Debug, Clone)]
pub struct ScaledDecode {
    pub image: Rgb8,
    /// Dimensions of the JPEG before scaling.
    pub source_width: u32,
    pub source_height: u32,
}

/// Encodes interleaved RGB (`channels` = 3) or RGBX (4) at `quality` with 4:4:4 chroma.
pub fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u8,
    quality: u8,
) -> Result<Vec<u8>, Error> {
    if !matches!(channels, 3 | 4) {
        return Err(Error(format!("unsupported channel count {channels}")));
    }
    let expected = width as usize * height as usize * usize::from(channels);
    if pixels.len() != expected || expected == 0 {
        return Err(Error(format!(
            "pixel buffer has {} bytes, expected {expected}",
            pixels.len()
        )));
    }
    let dim = |v: u32| c_int::try_from(v).map_err(|_| Error(format!("dimension {v} too large")));
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut out_len = 0usize;
    let mut err = [0 as c_char; 200];
    // SAFETY: `pixels` holds width*height*channels bytes (checked above); out-pointers
    // are valid; the returned buffer is freed below.
    let rc = unsafe {
        pe_tj_encode(
            pixels.as_ptr(),
            dim(width)?,
            dim(height)?,
            c_int::from(channels),
            c_int::from(quality.clamp(1, 100)),
            &mut out,
            &mut out_len,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if rc != 0 || out.is_null() {
        if !out.is_null() {
            // SAFETY: allocated by TurboJPEG.
            unsafe { pe_tj_free(out) };
        }
        return Err(error_from(&err));
    }
    // SAFETY: TurboJPEG returned `out_len` initialised bytes at `out`.
    let bytes = unsafe { std::slice::from_raw_parts(out, out_len) }.to_vec();
    // SAFETY: allocated by TurboJPEG, not used after this call.
    unsafe { pe_tj_free(out) };
    Ok(bytes)
}

/// Decodes to RGB using DCT-domain scaling: the smallest power-of-two scale (1, 1/2,
/// 1/4, 1/8) whose long edge is still at least `min_long_edge`. Scaled decoding skips
/// most of the IDCT and colour-conversion work and never allocates the full-size
/// image; entropy (Huffman) decoding still covers the whole file.
pub fn decode_scaled(jpeg: &[u8], min_long_edge: u32) -> Result<ScaledDecode, Error> {
    let mut err = [0 as c_char; 200];
    let (mut sw, mut sh, mut w, mut h) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: `jpeg` is a valid slice; all out-pointers are valid.
    let rc = unsafe {
        pe_tj_scaled_size(
            jpeg.as_ptr(),
            jpeg.len(),
            min_long_edge,
            &mut sw,
            &mut sh,
            &mut w,
            &mut h,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if rc != 0 {
        return Err(error_from(&err));
    }
    if u64::from(w) * u64::from(h) > MAX_OUTPUT_PIXELS {
        return Err(Error(format!("refusing to decode {w}x{h}")));
    }
    let mut data = vec![0u8; w as usize * h as usize * 3];
    // SAFETY: `data` holds exactly the scaled size reported for the same inputs.
    let rc = unsafe {
        pe_tj_decode_scaled(
            jpeg.as_ptr(),
            jpeg.len(),
            min_long_edge,
            data.as_mut_ptr(),
            data.len(),
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if rc != 0 {
        return Err(error_from(&err));
    }
    Ok(ScaledDecode {
        image: Rgb8 {
            width: w,
            height: h,
            data,
        },
        source_width: sw,
        source_height: sh,
    })
}

fn error_from(err: &[c_char]) -> Error {
    // SAFETY: the shim always NUL-terminates the error buffer (it is zero-initialised).
    Error(
        unsafe { CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smooth gradient so that scaled decodes can be compared with the source.
    fn gradient(w: u32, h: u32) -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| [(x * 255 / w) as u8, (y * 255 / h) as u8, 128]))
            .collect()
    }

    #[test]
    fn encode_decode_round_trip_full_size() {
        let px = gradient(160, 96);
        let jpeg = encode(&px, 160, 96, 3, 95).unwrap();
        let d = decode_scaled(&jpeg, 1000).unwrap(); // min larger than image: full size
        assert_eq!(
            (
                d.image.width,
                d.image.height,
                d.source_width,
                d.source_height
            ),
            (160, 96, 160, 96)
        );
        let max = d
            .image
            .data
            .iter()
            .zip(&px)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 6, "max diff {max}");
    }

    #[test]
    fn picks_smallest_scale_that_meets_the_minimum() {
        let jpeg = encode(&gradient(800, 600), 800, 600, 3, 90).unwrap();
        // 800 -> 1/2 = 400 >= 300, 1/4 = 200 < 300: expect 1/2 (3/8 is never used).
        let d = decode_scaled(&jpeg, 300).unwrap();
        assert_eq!((d.image.width, d.image.height), (400, 300));
        // 1620 -> 1/2 = 810 < 1024: stays full size, like a halving loop would.
        let wide = encode(&gradient(1620, 1080), 1620, 1080, 3, 90).unwrap();
        let d = decode_scaled(&wide, 1024).unwrap();
        assert_eq!((d.image.width, d.image.height), (1620, 1080));
        let d = decode_scaled(&jpeg, 100).unwrap();
        assert_eq!((d.image.width, d.image.height), (100, 75)); // 1/8
        assert_eq!(d.image.data.len(), 100 * 75 * 3);
    }

    #[test]
    fn scaled_decode_approximates_the_image() {
        let (w, h) = (640, 480);
        let jpeg = encode(&gradient(w, h), w, h, 3, 95).unwrap();
        let d = decode_scaled(&jpeg, 160).unwrap(); // 1/4
        assert_eq!((d.image.width, d.image.height), (160, 120));
        let expected = gradient(160, 120);
        let max = d
            .image
            .data
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 8, "max diff {max}");
    }

    #[test]
    fn rgbx_input_is_accepted() {
        let px: Vec<u8> = gradient(32, 16)
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        assert!(encode(&px, 32, 16, 4, 90).is_ok());
    }

    #[test]
    fn invalid_input_is_an_error_not_a_crash() {
        assert!(decode_scaled(b"not a jpeg", 100).is_err());
        assert!(decode_scaled(&[], 100).is_err());
        let mut truncated = encode(&gradient(64, 64), 64, 64, 3, 90).unwrap();
        truncated.truncate(truncated.len() / 3);
        assert!(decode_scaled(&truncated, 16).is_err());
        assert!(encode(&[0; 10], 4, 4, 3, 90).is_err());
        assert!(encode(&[0; 32], 4, 4, 2, 90).is_err());
    }
}
