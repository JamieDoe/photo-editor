use std::fmt;

/// Errors raised when constructing image buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Width or height is zero.
    EmptyDimensions,
    /// The pixel buffer length does not match `width * height * channels`.
    LengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDimensions => write!(f, "image dimensions must be non-zero"),
            Self::LengthMismatch { expected, actual } => {
                write!(f, "pixel buffer has {actual} samples, expected {expected}")
            }
        }
    }
}

impl std::error::Error for ImageError {}

/// Scene-linear RGB image, 16 bits per channel, interleaved, Rec.709/sRGB primaries.
///
/// `0` is black and `65535` is the source white (sensor clip point for RAW, `255` for
/// 8-bit sources). Values are stored as integers to halve memory relative to `f32`;
/// the renderer converts to `f32` per row chunk, so processing precision is not
/// limited to 16 bits.
///
/// Images are immutable once made, and each one made gets its own [`id`](Self::id)
/// (a clone keeps it, having the same pixels), so caches can key on it exactly.
#[derive(Clone)]
pub struct LinearImage {
    width: u32,
    height: u32,
    data: Vec<u16>,
    id: u64,
}

impl PartialEq for LinearImage {
    /// Equal pixels, whatever the ids.
    fn eq(&self, other: &Self) -> bool {
        (self.width, self.height) == (other.width, other.height) && self.data == other.data
    }
}

impl LinearImage {
    pub const CHANNELS: usize = 3;
    pub const WHITE: u16 = u16::MAX;

    pub fn new(width: u32, height: u32, data: Vec<u16>) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::EmptyDimensions);
        }
        let expected = width as usize * height as usize * Self::CHANNELS;
        if data.len() != expected {
            return Err(ImageError::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Ok(Self {
            width,
            height,
            data,
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        })
    }

    /// This image's identity: different for every image made, so a cache keyed on it
    /// never mistakes one image for another (even one made at the same address).
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn long_edge(&self) -> u32 {
        self.width.max(self.height)
    }

    pub fn data(&self) -> &[u16] {
        &self.data
    }

    /// Samples for one row (`width * 3` values).
    pub fn row(&self, y: u32) -> &[u16] {
        let stride = self.width as usize * Self::CHANNELS;
        let start = y as usize * stride;
        &self.data[start..start + stride]
    }

    pub fn byte_size(&self) -> usize {
        self.data.len() * size_of::<u16>()
    }
}

impl fmt::Debug for LinearImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinearImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Channel layout of an [`OutputImage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 8-bit RGBA with opaque alpha. Used for display so the webview can blit directly.
    Rgba8,
    /// 8-bit RGB. Used for export encoders.
    Rgb8,
}

impl PixelFormat {
    pub fn channels(self) -> usize {
        match self {
            Self::Rgba8 => 4,
            Self::Rgb8 => 3,
        }
    }
}

/// Display-encoded (sRGB) 8-bit image produced by the renderer.
#[derive(Clone, PartialEq)]
pub struct OutputImage {
    width: u32,
    height: u32,
    format: PixelFormat,
    data: Vec<u8>,
}

impl OutputImage {
    /// Allocates a zeroed image.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::EmptyDimensions);
        }
        let len = width as usize * height as usize * format.channels();
        Ok(Self {
            width,
            height,
            format,
            data: vec![0; len],
        })
    }

    pub fn from_raw(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::EmptyDimensions);
        }
        let expected = width as usize * height as usize * format.channels();
        if data.len() != expected {
            return Err(ImageError::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            format,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn byte_size(&self) -> usize {
        self.data.len()
    }
}

impl fmt::Debug for OutputImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_image_has_its_own_id() {
        let a = LinearImage::new(1, 1, vec![1, 2, 3]).unwrap();
        let b = LinearImage::new(1, 1, vec![1, 2, 3]).unwrap();
        // Same pixels, equal, but never the same image to a cache.
        assert!(a == b);
        assert_ne!(a.id(), b.id());
        // A clone is the same image.
        assert_eq!(a.clone().id(), a.id());
    }

    #[test]
    fn linear_image_validates_length() {
        assert!(LinearImage::new(2, 2, vec![0; 12]).is_ok());
        assert_eq!(
            LinearImage::new(2, 2, vec![0; 11]),
            Err(ImageError::LengthMismatch {
                expected: 12,
                actual: 11
            })
        );
        assert_eq!(
            LinearImage::new(0, 2, vec![]),
            Err(ImageError::EmptyDimensions)
        );
    }

    #[test]
    fn linear_image_row_access() {
        let data: Vec<u16> = (0..18).collect();
        let img = LinearImage::new(3, 2, data).unwrap();
        assert_eq!(img.row(1), &[9, 10, 11, 12, 13, 14, 15, 16, 17]);
        assert_eq!(img.long_edge(), 3);
        assert_eq!(img.byte_size(), 36);
    }

    #[test]
    fn output_image_sizes_by_format() {
        assert_eq!(
            OutputImage::new(4, 2, PixelFormat::Rgba8)
                .unwrap()
                .byte_size(),
            32
        );
        assert_eq!(
            OutputImage::new(4, 2, PixelFormat::Rgb8)
                .unwrap()
                .byte_size(),
            24
        );
        assert!(OutputImage::from_raw(1, 1, PixelFormat::Rgb8, vec![0; 4]).is_err());
    }
}
