//! Source decoding behind a decoder abstraction.
//!
//! Every decoder converts its format into the renderer's internal representation
//! ([`image_core::LinearImage`]); nothing downstream depends on a specific decoder.
//! [`DecoderRegistry`] picks a decoder by file type.

mod error;
mod jpeg;
pub mod lens;
#[cfg(feature = "libraw")]
mod libraw;
mod metadata;
mod png_image;
mod rendered;
#[cfg(test)]
mod rendered_tests;
mod tiff_ifd;
mod tiff_image;
// Helpers for display previews: embedded RAW previews and reduced JPEG decodes.
mod preview;

use std::path::Path;

pub use error::DecodeError;
use image_core::{Cancellation, LinearImage, OutputImage};
pub use jpeg::JpegDecoder;
#[cfg(feature = "libraw")]
pub use libraw::LibRawDecoder;
pub use metadata::{PhotoMetadata, rotation_from_exif, rotation_from_flip};
pub use png_image::PngDecoder;
pub use tiff_image::TiffDecoder;

/// Which JPEG decoder handles display previews (embedded RAW previews, JPEG
/// thumbnails) in this build.
pub const fn embedded_jpeg_decoder() -> &'static str {
    if cfg!(feature = "turbojpeg") {
        "libjpeg-turbo (DCT-scaled)"
    } else {
        "zune-jpeg (full size)"
    }
}

/// Requested decode resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeScale {
    /// Full sensor/image resolution. Used for export.
    Full,
    /// The decoder may reduce resolution (by any power of two it supports cheaply) as
    /// long as the result's long edge is at least this many pixels. Used for previews.
    AtLeast(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeOptions {
    pub scale: DecodeScale,
    /// Upper bound on threads a decoder may use internally (e.g. LibRaw's OpenMP
    /// regions). `None` leaves the decoder's default. Background jobs pass their
    /// compute-pool size so decodes stay within the lane's CPU budget.
    pub max_threads: Option<usize>,
}

impl DecodeOptions {
    pub fn new(scale: DecodeScale) -> Self {
        Self {
            scale,
            max_threads: None,
        }
    }

    pub fn with_max_threads(mut self, threads: usize) -> Self {
        self.max_threads = Some(threads.max(1));
        self
    }
}

/// Where a source's pixel values sit relative to the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Camera RAW: linear sensor data, not yet tone mapped.
    CameraRaw,
    /// Already-rendered image (JPEG etc.), linearised from its display encoding.
    Rendered,
}

/// Metadata reported by a decoder alongside the pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub decoder: &'static str,
    pub kind: SourceKind,
    pub make: String,
    pub model: String,
    /// Dimensions at full resolution, after orientation.
    pub full_width: u32,
    pub full_height: u32,
    pub iso: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub aperture: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// The light the camera's as-shot white balance neutralised, if known (camera
    /// RAW). White balance adjustments are relative to it. Rendered images have none:
    /// their white is the display's (D65).
    pub as_shot_white: Option<image_core::Chromaticity>,
    /// The EXIF Orientation (1 to 8) this decode turned the file upright by (ADR
    /// 0078); 1 when it is shown as stored. Camera raw files are turned by LibRaw and
    /// say 1.
    pub orientation: u16,
}

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub image: LinearImage,
    pub info: SourceInfo,
}

/// A camera-rendered preview embedded in the source file (e.g. the JPEG inside a
/// RAW). Display-only: shown while the real decode runs, never edited.
#[derive(Debug, Clone)]
pub struct EmbeddedPreview {
    /// Oriented like the decoded image, RGBA8 sRGB.
    pub image: OutputImage,
    /// Dimensions of the embedded image before any downscaling.
    pub embedded_width: u32,
    pub embedded_height: u32,
}

/// A source decoder. Implementations must be thread-safe: the registry is shared by
/// all job workers.
pub trait Decoder: Send + Sync {
    fn name(&self) -> &'static str;

    /// Cheap check (by extension) whether this decoder handles `path`.
    fn handles(&self, path: &Path) -> bool;

    fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError>;

    /// Reads metadata from the file's headers without decoding the image.
    fn read_metadata(&self, _path: &Path) -> Result<PhotoMetadata, DecodeError> {
        Ok(PhotoMetadata::default())
    }

    /// Extracts an embedded preview whose long edge is at least `min_long_edge` if
    /// possible (downscaled towards it), without decoding the image data. Decoders for
    /// formats without embedded previews keep the default.
    fn embedded_preview(
        &self,
        _path: &Path,
        _min_long_edge: u32,
        _cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        Ok(None)
    }

    /// A camera-rendered image for display at about `min_long_edge` (thumbnails),
    /// without decoding the image data at full size: the embedded preview for camera
    /// RAW formats, a reduced-scale decode of the file itself for rendered formats.
    /// `Ok(None)` if there is no such image; callers then decode and render instead.
    fn display_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        self.embedded_preview(path, min_long_edge, cancel)
    }
}

/// Ordered set of decoders; the first that handles a path wins.
pub struct DecoderRegistry {
    decoders: Vec<Box<dyn Decoder>>,
}

impl DecoderRegistry {
    pub fn new(decoders: Vec<Box<dyn Decoder>>) -> Self {
        Self { decoders }
    }

    /// All decoders compiled into this build.
    pub fn with_defaults() -> Self {
        Self::new(vec![
            Box::new(JpegDecoder),
            Box::new(PngDecoder),
            Box::new(TiffDecoder),
            #[cfg(feature = "libraw")]
            Box::new(LibRawDecoder),
        ])
    }

    pub fn decoder_for(&self, path: &Path) -> Option<&dyn Decoder> {
        self.decoders
            .iter()
            .find(|d| d.handles(path))
            .map(|d| d.as_ref())
    }

    pub fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError> {
        let decoder = self.decoder_for(path).ok_or_else(|| {
            DecodeError::Unsupported(format!("no decoder for {}", path.display()))
        })?;
        decoder.decode(path, options, cancel)
    }

    /// Metadata via the decoder for `path`.
    pub fn read_metadata(&self, path: &Path) -> Result<PhotoMetadata, DecodeError> {
        self.decoder_for(path)
            .ok_or_else(|| DecodeError::Unsupported(format!("no decoder for {}", path.display())))?
            .read_metadata(path)
    }

    /// Embedded preview via the decoder for `path`; `Ok(None)` if there is none.
    pub fn embedded_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        match self.decoder_for(path) {
            Some(d) => d.embedded_preview(path, min_long_edge, cancel),
            None => Ok(None),
        }
    }

    /// Display preview via the decoder for `path` (see [`Decoder::display_preview`]).
    pub fn display_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        self.decoder_for(path)
            .ok_or_else(|| DecodeError::Unsupported(format!("no decoder for {}", path.display())))?
            .display_preview(path, min_long_edge, cancel)
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.decoders.iter().map(|d| d.name()).collect()
    }

    /// File extensions (lowercase, no dot) that some decoder accepts.
    pub fn extensions(&self) -> Vec<&'static str> {
        let mut exts: Vec<&'static str> = Vec::new();
        exts.extend(jpeg::EXTENSIONS);
        exts.extend(png_image::EXTENSIONS);
        exts.extend(tiff_image::EXTENSIONS);
        #[cfg(feature = "libraw")]
        exts.extend(libraw::EXTENSIONS);
        exts
    }
}

impl Default for DecoderRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Whether files with extension `ext` (any case) are rendered images (JPEG, PNG,
/// TIFF), not camera raw: they carry their own metadata, and have no sidecars.
pub fn is_rendered_extension(ext: &str) -> bool {
    [
        jpeg::EXTENSIONS,
        png_image::EXTENSIONS,
        tiff_image::EXTENSIONS,
    ]
    .iter()
    .any(|list| list.iter().any(|x| x.eq_ignore_ascii_case(ext)))
}

pub(crate) fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_core::NeverCancel;

    #[test]
    fn registry_selects_by_extension_case_insensitively() {
        let reg = DecoderRegistry::with_defaults();
        assert_eq!(
            reg.decoder_for(Path::new("a/b.JPG")).map(|d| d.name()),
            Some("zune-jpeg")
        );
        #[cfg(feature = "libraw")]
        assert_eq!(
            reg.decoder_for(Path::new("x.NEF")).map(|d| d.name()),
            Some("libraw")
        );
        assert!(reg.decoder_for(Path::new("x.txt")).is_none());
        assert!(reg.decoder_for(Path::new("noext")).is_none());
    }

    #[test]
    fn unsupported_file_is_reported() {
        let reg = DecoderRegistry::with_defaults();
        let opts = DecodeOptions::new(DecodeScale::Full);
        let err = reg
            .decode(Path::new("x.txt"), opts, &NeverCancel)
            .unwrap_err();
        assert!(matches!(err, DecodeError::Unsupported(_)));
    }
}
