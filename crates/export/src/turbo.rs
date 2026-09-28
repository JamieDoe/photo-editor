//! libjpeg-turbo encoder (via the shared `jpeg-turbo` crate).

use image_core::{OutputImage, PixelFormat};

use crate::ExportError;

pub(crate) fn encode(image: &OutputImage, quality: u8) -> Result<Vec<u8>, ExportError> {
    let channels = match image.format() {
        PixelFormat::Rgb8 => 3,
        PixelFormat::Rgba8 => 4,
    };
    jpeg_turbo::encode(
        image.data(),
        image.width(),
        image.height(),
        channels,
        quality,
    )
    .map_err(|e| ExportError::Encode(e.to_string()))
}
