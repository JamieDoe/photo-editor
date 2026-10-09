//! PNG decoding via the `png` crate (MIT/Apache-2.0, pure Rust): 8- and 16-bit grey,
//! RGB and their alpha forms; palettes and low bit depths are expanded to 8 bits.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image_core::Cancellation;

use crate::rendered::{self, Raster, Samples};
use crate::{DecodeError, DecodeOptions, DecodedImage, Decoder, EmbeddedPreview, PhotoMetadata};

pub(crate) const EXTENSIONS: &[&str] = &["png"];

/// Decodes PNG files (as sRGB, like JPEG; embedded colour profiles are not read yet).
#[derive(Debug, Default, Clone, Copy)]
pub struct PngDecoder;

fn corrupt(e: impl std::fmt::Display) -> DecodeError {
    DecodeError::Corrupt(format!("PNG: {e}"))
}

/// The PNG's first frame.
fn read(path: &Path) -> Result<Raster, DecodeError> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(corrupt)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| corrupt("image too large"))?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(corrupt)?;
    buf.truncate(info.buffer_size());
    let channels = info.color_type.samples();
    let samples = match info.bit_depth {
        png::BitDepth::Sixteen => Samples::U16(
            buf.as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes(*b))
                .collect(),
        ),
        _ => Samples::U8(buf),
    };
    Raster::new(info.width, info.height, channels, samples)
}

impl Decoder for PngDecoder {
    fn name(&self) -> &'static str {
        "png"
    }

    fn handles(&self, path: &Path) -> bool {
        crate::has_extension(path, EXTENSIONS)
    }

    fn read_metadata(&self, path: &Path) -> Result<PhotoMetadata, DecodeError> {
        crate::metadata::read_png(path)
    }

    /// The PNG itself, reduced (a PNG can't be decoded at a smaller scale).
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
        let metadata = crate::metadata::read_png(path).unwrap_or_default();
        rendered::decoded(&raster, self.name(), metadata, options, cancel)
    }
}
