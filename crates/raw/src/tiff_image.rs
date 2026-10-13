//! TIFF decoding via the `tiff` crate (MIT, pure Rust): 8- and 16-bit grey, RGB and
//! their alpha forms, uncompressed or LZW, Deflate or JPEG compressed, stored pixel by
//! pixel or plane by plane. The first image of the file; CMYK, palette and
//! floating-point TIFFs are not supported.
//!
//! Its strips or tiles are decoded in parallel, straight to linear light and reduced
//! as they are read (ADR 0082).

use std::path::Path;

use image_core::Cancellation;

use crate::tiff_chunks::{self, Layout};
use crate::{
    DecodeError, DecodeOptions, DecodeScale, DecodedImage, Decoder, EmbeddedPreview, PhotoMetadata,
    rendered,
};

pub(crate) const EXTENSIONS: &[&str] = &["tif", "tiff"];

/// Decodes TIFF images, converting from their embedded ICC profile (ADR 0077);
/// without one they are taken as sRGB.
#[derive(Debug, Default, Clone, Copy)]
pub struct TiffDecoder;

fn layout(path: &Path) -> Result<Layout, DecodeError> {
    Layout::of(&mut tiff_chunks::open(path)?)
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

    /// The TIFF itself, reduced as it is decoded (a TIFF has no smaller scale to
    /// decode at).
    fn display_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        let orientation = crate::metadata::read_tiff(path).map_or(0, |m| m.orientation);
        let layout = layout(path)?;
        let factor = rendered::reduction(layout.width, layout.height, min_long_edge);
        let image = tiff_chunks::decode_linear(path, &layout, factor, cancel)?;
        let rgb = crate::preview::Rgb8 {
            width: image.width(),
            height: image.height(),
            data: image
                .data()
                .iter()
                .map(|&v| rendered::encode_srgb8(f32::from(v) / 65535.0))
                .collect(),
        };
        Ok(Some(rendered::oriented_preview(
            &rgb,
            (layout.width, layout.height),
            orientation,
        )))
    }

    fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError> {
        let metadata = crate::metadata::read_tiff(path).unwrap_or_default();
        let layout = layout(path)?;
        let factor = match options.scale {
            DecodeScale::AtLeast(min_edge) => {
                rendered::reduction(layout.width, layout.height, min_edge)
            }
            DecodeScale::Full => 1,
        };
        let image = tiff_chunks::decode_linear(path, &layout, factor, cancel)?;
        let image = rendered::upright(image, metadata.orientation)?;
        let full = if crate::metadata::flip_from_exif(metadata.orientation) & 4 != 0 {
            (layout.height, layout.width)
        } else {
            (layout.width, layout.height)
        };
        Ok(rendered::described(image, full, self.name(), metadata))
    }
}
