//! Export colour spaces (ADR 0061): sRGB, Display P3 or Adobe RGB, as the design's
//! "Colour space" row offers.
//!
//! The working space is sRGB, with colours beyond it softly compressed in (ADR 0060),
//! so a P3 or Adobe RGB export holds the same colours: it is a conversion and a tag,
//! for wide-gamut displays and for print labs and workflows that ask for those spaces.
//! The rendered image (sRGB-encoded, rendered at 16 bits for this) is decoded to
//! linear light, taken to the space's primaries by matrix (all three share D65, so no
//! white adaptation), encoded with the space's own curve and rounded to the file's
//! depth once. The file carries the space's profile.

use image_core::color::{linear_to_srgb, srgb_to_linear};
use image_core::{OutputImage, PixelFormat};
use rayon::prelude::*;

use crate::icc::{self, Curve, Space};

/// The colour space an export is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportColourSpace {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
}

impl ExportColourSpace {
    pub fn space(self) -> &'static Space {
        match self {
            Self::Srgb => &icc::SRGB,
            Self::DisplayP3 => &icc::DISPLAY_P3,
            Self::AdobeRgb => &icc::ADOBE_RGB,
        }
    }

    /// The profile the file carries.
    pub fn profile(self) -> Vec<u8> {
        icc::profile(self.space())
    }
}

/// Linear sRGB to the space's linear RGB: sRGB to XYZ, then XYZ to the space.
fn matrix(to: &Space) -> [[f32; 3]; 3] {
    let from = icc::rgb_to_xyz(&icc::SRGB);
    let back = icc::invert(&icc::rgb_to_xyz(to));
    std::array::from_fn(|r| {
        std::array::from_fn(|c| (0..3).map(|k| back[r][k] * from[k][c]).sum::<f64>() as f32)
    })
}

fn encode_curve(curve: Curve, v: f32) -> f32 {
    match curve {
        Curve::Srgb => linear_to_srgb(v),
        Curve::Gamma(g) => v.powf(1.0 / g as f32),
    }
}

/// `image` (sRGB-encoded RGB, 8- or 16-bit) in `to`, as `format` (8- or 16-bit RGB).
/// sRGB to sRGB only changes the depth.
pub fn convert(image: &OutputImage, to: ExportColourSpace, format: PixelFormat) -> OutputImage {
    let (w, h) = (image.width(), image.height());
    let wide_in = image.format() == PixelFormat::Rgb16;
    if to == ExportColourSpace::Srgb && image.format() == format {
        return image.clone();
    }
    // Decoding table for the input's depth.
    let levels = if wide_in { 65536 } else { 256 };
    let table: Vec<f32> = (0..levels)
        .map(|v| srgb_to_linear(v as f32 / (levels - 1) as f32))
        .collect();
    let samples: Vec<u16> = if wide_in {
        image.samples16().unwrap_or_default()
    } else {
        image.data().iter().map(|&v| u16::from(v)).collect()
    };
    let space = to.space();
    let m = matrix(space);
    let depth16 = format == PixelFormat::Rgb16;
    let pixels: Vec<[f32; 3]> = samples
        .par_chunks(image.format().channels())
        .map(|p| {
            let lin = [0, 1, 2].map(|c| table[usize::from(p[c])]);
            let out = [0, 1, 2].map(|r| m[r][0] * lin[0] + m[r][1] * lin[1] + m[r][2] * lin[2]);
            out.map(|v| encode_curve(space.curve, v.clamp(0.0, 1.0)))
        })
        .collect();
    let data: Vec<u8> = if depth16 {
        pixels
            .par_iter()
            .flat_map_iter(|p| {
                p.iter()
                    .flat_map(|v| ((v * 65535.0).round() as u16).to_ne_bytes())
                    .collect::<Vec<_>>()
            })
            .collect()
    } else {
        pixels
            .par_iter()
            .flat_map_iter(|p| p.map(|v| (v * 255.0).round() as u8))
            .collect()
    };
    OutputImage::from_raw(w, h, format, data).unwrap_or_else(|_| image.clone())
}

/// `jpeg` with `profile` embedded as an ICC_PROFILE APP2 segment, after the JFIF APP0
/// segment if there is one (the profile fits one segment: well under 64 KB).
pub fn jpeg_with_profile(jpeg: &[u8], profile: &[u8]) -> Vec<u8> {
    const SIGNATURE: &[u8] = b"ICC_PROFILE\0";
    let mut segment = vec![0xFF, 0xE2];
    let len = (2 + SIGNATURE.len() + 2 + profile.len()) as u16;
    segment.extend(len.to_be_bytes());
    segment.extend(SIGNATURE);
    segment.extend([1, 1]); // part 1 of 1
    segment.extend(profile);
    // After SOI, and after APP0 (JFIF) when it comes first.
    let mut at = 2;
    if jpeg.len() > 6 && jpeg[2] == 0xFF && jpeg[3] == 0xE0 {
        at = 4 + usize::from(u16::from_be_bytes([jpeg[4], jpeg[5]]));
    }
    let mut out = Vec::with_capacity(jpeg.len() + segment.len());
    out.extend(&jpeg[..at]);
    out.extend(segment);
    out.extend(&jpeg[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgb: [u8; 3]) -> OutputImage {
        OutputImage::from_raw(1, 1, PixelFormat::Rgb8, rgb.to_vec()).unwrap()
    }

    #[test]
    fn greys_and_white_stay_as_they_are() {
        // Same white, and P3 shares sRGB's curve: greys keep their values.
        for v in [0u8, 50, 128, 200, 255] {
            let out = convert(
                &pixel([v; 3]),
                ExportColourSpace::DisplayP3,
                PixelFormat::Rgb8,
            );
            assert_eq!(out.data(), &[v; 3], "{v}");
        }
        // Adobe RGB's own curve: mid grey encodes differently, white and black do not.
        let white = convert(
            &pixel([255; 3]),
            ExportColourSpace::AdobeRgb,
            PixelFormat::Rgb8,
        );
        assert_eq!(white.data(), &[255; 3]);
        let black = convert(
            &pixel([0; 3]),
            ExportColourSpace::AdobeRgb,
            PixelFormat::Rgb8,
        );
        assert_eq!(black.data(), &[0; 3]);
    }

    #[test]
    fn saturated_srgb_colours_sit_inside_the_wider_spaces() {
        // sRGB's pure red is less saturated than P3's or Adobe RGB's: green and blue
        // are no longer zero, red below the maximum.
        for space in [ExportColourSpace::DisplayP3, ExportColourSpace::AdobeRgb] {
            let out = convert(&pixel([255, 0, 0]), space, PixelFormat::Rgb8);
            let p = out.data();
            assert!(p[0] < 255 && p[0] > 200, "{space:?}: {p:?}");
            if space == ExportColourSpace::DisplayP3 {
                assert!(p[1] > 0, "{p:?}");
            }
        }
        // sRGB's pure green in Adobe RGB (whose green reaches much further): the
        // well-known (144, 255, 60).
        let g = convert(
            &pixel([0, 255, 0]),
            ExportColourSpace::AdobeRgb,
            PixelFormat::Rgb8,
        );
        let want = [144, 255, 60];
        for (got, want) in g.data().iter().zip(want) {
            assert!(i32::from(*got).abs_diff(want) <= 1, "{:?}", g.data());
        }
    }

    #[test]
    fn round_trips_through_the_matrices() {
        // Converting to P3 and back by the inverse matrix returns the colour.
        let m = matrix(&icc::DISPLAY_P3);
        let back = icc::invert(&icc::rgb_to_xyz(&icc::SRGB));
        let to = icc::rgb_to_xyz(&icc::DISPLAY_P3);
        let v = [0.3f32, 0.5, 0.1];
        let p3 = [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2]);
        let xyz = [0, 1, 2].map(|r| (0..3).map(|k| to[r][k] * f64::from(p3[k])).sum::<f64>());
        let srgb = [0, 1, 2].map(|r| (0..3).map(|k| back[r][k] * xyz[k]).sum::<f64>());
        for (a, b) in srgb.iter().zip(v) {
            assert!((a - f64::from(b)).abs() < 1e-5, "{srgb:?}");
        }
    }

    #[test]
    fn sixteen_bit_in_and_out() {
        let data: Vec<u8> = [30000u16, 20000, 10000]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        let img = OutputImage::from_raw(1, 1, PixelFormat::Rgb16, data).unwrap();
        let out = convert(&img, ExportColourSpace::AdobeRgb, PixelFormat::Rgb16);
        assert_eq!(out.format(), PixelFormat::Rgb16);
        // 16-bit to 8-bit sRGB: only the depth changes.
        let eight = convert(&img, ExportColourSpace::Srgb, PixelFormat::Rgb8);
        assert_eq!(
            eight.data()[0],
            (30000.0f32 / 65535.0 * 255.0).round() as u8
        );
    }

    #[test]
    fn jpegs_get_their_profile_after_jfif() {
        // SOI, APP0 (JFIF, 16 bytes), then the rest.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        jpeg.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        jpeg.extend([0xFF, 0xDB, 0x00, 0x02]);
        let profile = ExportColourSpace::DisplayP3.profile();
        let out = jpeg_with_profile(&jpeg, &profile);
        let at = 2 + 2 + 16;
        assert_eq!(&out[at..at + 2], &[0xFF, 0xE2]);
        assert_eq!(&out[at + 4..at + 16], b"ICC_PROFILE\0");
        assert_eq!(&out[at + 18..at + 18 + profile.len()], &profile[..]);
        assert_eq!(&out[out.len() - 4..], &[0xFF, 0xDB, 0x00, 0x02]);
    }
}
