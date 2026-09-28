//! JPEG decoding via zune-jpeg (pure Rust, MIT/Apache-2.0/Zlib).

use std::io::Cursor;
use std::path::Path;

use image_core::pyramid::downsample_2x;
use image_core::{Cancellation, LinearImage, color};
use zune_jpeg::JpegDecoder as ZuneDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::{
    DecodeError, DecodeOptions, DecodeScale, DecodedImage, Decoder, EmbeddedPreview, SourceInfo,
    SourceKind, preview,
};

pub(crate) const EXTENSIONS: &[&str] = &["jpg", "jpeg"];

/// Decodes baseline/progressive JPEG and linearises its sRGB encoding.
///
/// Limitations (Phase 0): EXIF orientation is ignored and embedded ICC profiles are
/// assumed to be sRGB.
#[derive(Debug, Default, Clone, Copy)]
pub struct JpegDecoder;

impl Decoder for JpegDecoder {
    fn name(&self) -> &'static str {
        "zune-jpeg"
    }

    fn handles(&self, path: &Path) -> bool {
        crate::has_extension(path, EXTENSIONS)
    }

    fn read_metadata(&self, path: &Path) -> Result<crate::PhotoMetadata, DecodeError> {
        crate::metadata::read_jpeg(path)
    }

    /// The JPEG itself, decoded at reduced scale (DCT scaling with libjpeg-turbo, so
    /// a 24 MP file is never decoded at full size). EXIF orientation is ignored, as in
    /// [`JpegDecoder::decode`].
    fn display_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        let bytes = std::fs::read(path)?;
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }
        let (mut rgb, (width, height)) = preview::decode_jpeg(&bytes, min_long_edge)?;
        while rgb.width.max(rgb.height) / 2 >= min_long_edge.max(1) {
            rgb = preview::downsample_2x(&rgb);
        }
        Ok(Some(EmbeddedPreview {
            image: preview::orient_to_rgba(&rgb, 0),
            embedded_width: width,
            embedded_height: height,
        }))
    }

    fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError> {
        let bytes = std::fs::read(path)?;
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }
        let opts = DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGB)
            .set_max_width(1 << 16)
            .set_max_height(1 << 16);
        let mut decoder = ZuneDecoder::new_with_options(Cursor::new(bytes), opts);
        let rgb8 = decoder
            .decode()
            .map_err(|e| DecodeError::Corrupt(format!("{e:?}")))?;
        let info = decoder
            .info()
            .ok_or_else(|| DecodeError::Internal("missing JPEG info".into()))?;
        let (w, h) = (u32::from(info.width), u32::from(info.height));
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }

        let table = color::srgb8_to_linear16_table();
        let linear: Vec<u16> = rgb8.iter().map(|&v| table[usize::from(v)]).collect();
        let mut image = LinearImage::new(w, h, linear)
            .map_err(|e| DecodeError::Corrupt(format!("unexpected JPEG buffer: {e}")))?;

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
                decoder: self.name(),
                kind: SourceKind::Rendered,
                make: String::new(),
                model: String::new(),
                full_width: w,
                full_height: h,
                iso: None,
                shutter_seconds: None,
                aperture: None,
                focal_length_mm: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_core::NeverCancel;

    fn write_fixture(dir: &Path, w: u16, h: u16) -> std::path::PathBuf {
        let path = dir.join("test.jpg");
        std::fs::write(&path, fixtures::chart_jpeg(w, h, 95)).unwrap();
        path
    }

    #[test]
    fn display_preview_is_a_reduced_decode_of_the_file() {
        let dir = fixtures::TempDir::new("jpeg-display");
        let path = write_fixture(dir.path(), 2048, 1536);
        let p = JpegDecoder
            .display_preview(&path, 512, &NeverCancel)
            .unwrap()
            .expect("a JPEG is its own display image");
        let long = p.image.width().max(p.image.height());
        assert!((512..1024).contains(&long), "long edge {long}");
        assert_eq!((p.embedded_width, p.embedded_height), (2048, 1536));
        assert_eq!(p.image.format(), image_core::PixelFormat::Rgba8);
    }

    #[test]
    fn decodes_full_resolution() {
        let dir = fixtures::TempDir::new("jpeg-full");
        let path = write_fixture(dir.path(), 320, 200);
        let opts = DecodeOptions::new(DecodeScale::Full);
        let out = JpegDecoder.decode(&path, opts, &NeverCancel).unwrap();
        assert_eq!((out.image.width(), out.image.height()), (320, 200));
        assert_eq!((out.info.full_width, out.info.full_height), (320, 200));
        assert_eq!(out.info.kind, SourceKind::Rendered);
    }

    #[test]
    fn preview_scale_downsamples_but_respects_minimum() {
        let dir = fixtures::TempDir::new("jpeg-half");
        let path = write_fixture(dir.path(), 640, 400);
        let opts = DecodeOptions::new(DecodeScale::AtLeast(200));
        let out = JpegDecoder.decode(&path, opts, &NeverCancel).unwrap();
        // 640 -> 320 (>= 200), not 160.
        assert_eq!((out.image.width(), out.image.height()), (320, 200));
        assert_eq!(out.info.full_width, 640);
    }

    #[test]
    fn linearises_srgb_encoding() {
        let dir = fixtures::TempDir::new("jpeg-linear");
        let path = dir.path().join("grey.jpg");
        std::fs::write(&path, fixtures::solid_jpeg(16, 16, [128, 128, 128])).unwrap();
        let opts = DecodeOptions::new(DecodeScale::Full);
        let out = JpegDecoder.decode(&path, opts, &NeverCancel).unwrap();
        let v = f32::from(out.image.data()[0]) / 65535.0;
        // sRGB 128 ~= 0.216 linear (allow for JPEG quantisation).
        assert!((v - 0.216).abs() < 0.01, "{v}");
    }

    #[test]
    fn corrupt_file_is_reported() {
        let dir = fixtures::TempDir::new("jpeg-corrupt");
        let path = dir.path().join("bad.jpg");
        std::fs::write(&path, b"not a jpeg").unwrap();
        let opts = DecodeOptions::new(DecodeScale::Full);
        let err = JpegDecoder.decode(&path, opts, &NeverCancel).unwrap_err();
        assert!(matches!(err, DecodeError::Corrupt(_)), "{err}");
    }

    #[test]
    fn missing_file_is_not_found() {
        let opts = DecodeOptions::new(DecodeScale::Full);
        let err = JpegDecoder.decode(Path::new("/nonexistent/x.jpg"), opts, &NeverCancel);
        assert!(matches!(err, Err(DecodeError::NotFound(_))));
    }
}
