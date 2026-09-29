//! Internal image representations shared by the decoder, renderer and export layers.
//!
//! The renderer never sees a decoder-specific type: every source is converted into a
//! [`LinearImage`] (scene-linear, 16-bit, Rec.709/sRGB primaries) and every render
//! produces an [`OutputImage`] (display-encoded, 8-bit).

pub mod buffer;
pub mod cancel;
pub mod color;
pub mod pyramid;
pub mod resize;

pub use buffer::{ImageError, LinearImage, OutputImage, PixelFormat};
pub use cancel::{Cancellation, NeverCancel};
pub use color::Chromaticity;
pub use pyramid::Pyramid;
