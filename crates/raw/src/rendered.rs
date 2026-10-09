//! Rendered images other than JPEG (PNG, TIFF): their pixels as decoded, made into
//! the linear image the renderer edits, or an 8-bit preview for thumbnails.
//!
//! Their values are display-encoded (assumed sRGB, as for JPEG), 8 or 16 bits a
//! sample; 16-bit files keep their precision. Grey is made RGB and alpha is dropped
//! (transparent areas show their colour, as the file stores it).

use std::sync::OnceLock;

use rayon::prelude::*;

use image_core::pyramid::downsample_2x;
use image_core::{Cancellation, LinearImage, color};

use crate::preview::{self, Rgb8};
use crate::{
    DecodeError, DecodeOptions, DecodeScale, DecodedImage, EmbeddedPreview, PhotoMetadata,
    SourceInfo, SourceKind,
};

/// A file's samples as decoded.
pub(crate) enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

/// An image as decoded: `channels` samples a pixel (1: grey, 2: grey and alpha, 3:
/// RGB, 4: RGBA), rows packed.
pub(crate) struct Raster {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub samples: Samples,
}

/// 16-bit sRGB-encoded values to 16-bit linear ones.
fn srgb16_to_linear16() -> &'static [u16] {
    static TABLE: OnceLock<Vec<u16>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=u16::MAX)
            .map(|v| (color::srgb_to_linear(f32::from(v) / 65535.0) * 65535.0).round() as u16)
            .collect()
    })
}

impl Raster {
    /// Checks that its samples fill it.
    pub fn new(
        width: u32,
        height: u32,
        channels: usize,
        samples: Samples,
    ) -> Result<Self, DecodeError> {
        let len = match &samples {
            Samples::U8(v) => v.len(),
            Samples::U16(v) => v.len(),
        };
        let expected = width as usize * height as usize * channels;
        if width == 0 || height == 0 || !(1..=4).contains(&channels) || len < expected {
            return Err(DecodeError::Corrupt(format!(
                "{width}x{height} image with {channels} channels has {len} samples"
            )));
        }
        Ok(Self {
            width,
            height,
            channels,
            samples,
        })
    }

    /// Pixel `i`'s red, green and blue sample indices.
    fn rgb_of(&self, i: usize) -> [usize; 3] {
        let at = i * self.channels;
        if self.channels < 3 {
            [at; 3]
        } else {
            [at, at + 1, at + 2]
        }
    }

    /// Three values a pixel, from `pixel(i)` for pixel `i`, made row by row in parallel.
    fn per_pixel<T: Send + Copy + Default>(
        &self,
        pixel: impl Fn(usize) -> [T; 3] + Sync,
    ) -> Vec<T> {
        let width = self.width as usize;
        let mut out = vec![T::default(); width * self.height as usize * 3];
        out.par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    *px = pixel(y * width + x);
                }
            });
        out
    }

    /// The linear image the renderer edits.
    pub fn to_linear(&self) -> Result<LinearImage, DecodeError> {
        let data = match &self.samples {
            Samples::U8(s) => {
                let table = color::srgb8_to_linear16_table();
                self.per_pixel(|i| self.rgb_of(i).map(|k| table[usize::from(s[k])]))
            }
            Samples::U16(s) => {
                let table = srgb16_to_linear16();
                self.per_pixel(|i| self.rgb_of(i).map(|k| table[usize::from(s[k])]))
            }
        };
        LinearImage::new(self.width, self.height, data)
            .map_err(|e| DecodeError::Internal(format!("unexpected buffer: {e}")))
    }

    /// An 8-bit sRGB copy, for previews.
    fn to_rgb8(&self) -> Rgb8 {
        let data = match &self.samples {
            Samples::U8(s) => self.per_pixel(|i| self.rgb_of(i).map(|k| s[k])),
            Samples::U16(s) => self.per_pixel(|i| self.rgb_of(i).map(|k| (s[k] >> 8) as u8)),
        };
        Rgb8 {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

/// `raster` decoded for editing, reduced towards the scale asked for.
pub(crate) fn decoded(
    raster: &Raster,
    decoder: &'static str,
    metadata: PhotoMetadata,
    options: DecodeOptions,
    cancel: &dyn Cancellation,
) -> Result<DecodedImage, DecodeError> {
    let mut image = raster.to_linear()?;
    if let DecodeScale::AtLeast(min_edge) = options.scale {
        while image.long_edge() / 2 >= min_edge.max(1) {
            if cancel.is_cancelled() {
                return Err(DecodeError::Cancelled);
            }
            image = downsample_2x(&image);
        }
    }
    Ok(DecodedImage {
        image,
        info: SourceInfo {
            decoder,
            kind: SourceKind::Rendered,
            make: metadata.camera_make.unwrap_or_default(),
            model: metadata.camera_model.unwrap_or_default(),
            full_width: raster.width,
            full_height: raster.height,
            iso: metadata.iso.map(|v| v as f32),
            shutter_seconds: metadata.shutter_seconds,
            aperture: metadata.aperture,
            focal_length_mm: metadata.focal_length_mm,
            as_shot_white: None,
        },
    })
}

/// `raster` as a preview of about `min_long_edge`, for thumbnails.
pub(crate) fn preview_of(raster: &Raster, min_long_edge: u32) -> EmbeddedPreview {
    let mut rgb = raster.to_rgb8();
    while rgb.width.max(rgb.height) / 2 >= min_long_edge.max(1) {
        rgb = preview::downsample_2x(&rgb);
    }
    EmbeddedPreview {
        image: preview::orient_to_rgba(&rgb, 0),
        embedded_width: raster.width,
        embedded_height: raster.height,
    }
}
