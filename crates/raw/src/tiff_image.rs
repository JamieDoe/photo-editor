//! TIFF decoding via the `tiff` crate (MIT, pure Rust): 8- and 16-bit grey, RGB and
//! their alpha forms, uncompressed or LZW, Deflate or JPEG compressed. The first image
//! of the file; CMYK, palette and floating-point TIFFs are not supported.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image_core::Cancellation;
use tiff::ColorType;
use tiff::decoder::{Decoder as TiffReader, DecodingResult, Limits};

use crate::rendered::{self, Raster, Samples};
use crate::{DecodeError, DecodeOptions, DecodedImage, Decoder, EmbeddedPreview, PhotoMetadata};

pub(crate) const EXTENSIONS: &[&str] = &["tif", "tiff"];

/// Decodes TIFF images (as sRGB, like JPEG; embedded colour profiles are not read
/// yet).
#[derive(Debug, Default, Clone, Copy)]
pub struct TiffDecoder;

fn corrupt(e: impl std::fmt::Display) -> DecodeError {
    DecodeError::Corrupt(format!("TIFF: {e}"))
}

/// The TIFF's first image.
fn read(path: &Path) -> Result<Raster, DecodeError> {
    let file = BufReader::new(File::open(path)?);
    // Large scans and stitched panoramas are legitimate: no limit below what the
    // decoded image itself needs.
    let mut reader = TiffReader::new(file)
        .map_err(corrupt)?
        .with_limits(Limits::unlimited());
    let (width, height) = reader.dimensions().map_err(corrupt)?;
    let channels = match reader.colortype().map_err(corrupt)? {
        ColorType::Gray(8 | 16) => 1,
        ColorType::GrayA(8 | 16) => 2,
        ColorType::RGB(8 | 16) => 3,
        ColorType::RGBA(8 | 16) => 4,
        other => {
            return Err(DecodeError::Unsupported(format!(
                "TIFF images of type {other:?} are not supported"
            )));
        }
    };
    let samples = match reader.read_image().map_err(corrupt)? {
        DecodingResult::U8(v) => Samples::U8(v),
        DecodingResult::U16(v) => Samples::U16(v),
        _ => return Err(DecodeError::Unsupported("TIFF sample format".into())),
    };
    Raster::new(width, height, channels, samples)
}

impl Decoder for TiffDecoder {
    fn name(&self) -> &'static str {
        "tiff"
    }

    fn handles(&self, path: &Path) -> bool {
        crate::has_extension(path, EXTENSIONS)
    }

    fn read_metadata(&self, path: &Path) -> Result<PhotoMetadata, DecodeError> {
        crate::metadata::read_tiff(path)
    }

    /// The TIFF itself, reduced (a TIFF can't be decoded at a smaller scale).
    fn display_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        let raster = read(path)?;
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }
        Ok(Some(rendered::preview_of(&raster, min_long_edge)))
    }

    fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError> {
        let raster = read(path)?;
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }
        let metadata = crate::metadata::read_tiff(path).unwrap_or_default();
        rendered::decoded(&raster, self.name(), metadata, options, cancel)
    }
}
