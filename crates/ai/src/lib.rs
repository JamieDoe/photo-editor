//! The optional AI subsystem (ADR 0074): masks generated from a photo.
//!
//! A [`Segmenter`] takes the photo as an 8-bit sRGB picture and returns how much each
//! pixel belongs to what was asked for ([`MaskKind`]): a [`Coverage`] map, the same
//! primitive brush masks produce, so the renderer never needs to know where a mask
//! came from (CLAUDE.md §15). Segmenters run locally, and none is required: a
//! platform without one simply offers no generated masks.
//!
//! - **macOS:** Apple Vision (Subject and People from macOS 14). The system's own
//!   models, so nothing is shipped or licensed (ADR 0074's licensing policy).
//! - **Elsewhere:** none yet (ADR 0074, phase 2).

use std::fmt;

#[cfg(target_os = "macos")]
mod vision;

/// What a generated mask covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskKind {
    /// The photo's main subject or subjects, whatever they are.
    Subject,
    /// The people in it.
    People,
}

/// A photo for a segmenter: 8-bit sRGB RGBA, `width` × `height`, rows packed.
#[derive(Debug, Clone, Copy)]
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// How much each pixel belongs to the mask, 0 (not at all) to 255 (wholly), `width` ×
/// `height` over the whole picture (fractions of it map one to one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Coverage {
    /// Coverage at the picture's fractional point (`x`, `y`), 0..1, sampled bilinearly.
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let (w, h) = (self.width as usize, self.height as usize);
        if w == 0 || h == 0 {
            return 0.0;
        }
        let fx = (x * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
        let fy = (y * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let v = |xx: usize, yy: usize| f32::from(self.data[yy * w + xx]) / 255.0;
        let top = v(x0, y0) + (v(x1, y0) - v(x0, y0)) * tx;
        let bottom = v(x0, y1) + (v(x1, y1) - v(x0, y1)) * tx;
        top + (bottom - top) * ty
    }

    /// The share of the picture covered (0..1), counting partial coverage.
    pub fn share(&self) -> f32 {
        if self.data.is_empty() {
            return 0.0;
        }
        self.data.iter().map(|&v| u64::from(v)).sum::<u64>() as f32
            / (255.0 * self.data.len() as f32)
    }
}

/// Why a segmenter could not make a mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// This segmenter does not make this kind of mask (on this system).
    Unsupported(MaskKind),
    /// The picture's size and pixels don't agree.
    BadPicture,
    /// The system's model failed.
    Failed(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(kind) => write!(f, "{kind:?} masks are not available on this system"),
            Self::BadPicture => write!(f, "the picture's size does not match its pixels"),
            Self::Failed(m) => write!(f, "mask generation failed: {m}"),
        }
    }
}

impl std::error::Error for AiError {}

/// Makes masks from a photo.
pub trait Segmenter: Send + Sync {
    /// Whether it makes `kind` masks here (a system may be too old for one).
    fn supports(&self, kind: MaskKind) -> bool;

    /// Identifies what made a mask, for storing it (ADR 0074): the generator and its
    /// version, so a mask made by a newer model is known as such.
    fn generator(&self, kind: MaskKind) -> String;

    /// The `kind` mask of `picture`, or `None` when the photo has nothing of the kind
    /// (no subject, nobody). Synchronous and possibly slow: run it in the background.
    fn segment(&self, picture: Picture<'_>, kind: MaskKind) -> Result<Option<Coverage>, AiError>;
}

/// This platform's segmenter, if it has one.
pub fn platform_segmenter() -> Option<Box<dyn Segmenter>> {
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(vision::VisionSegmenter))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

impl Picture<'_> {
    /// Whether its pixels match its size (and it has some).
    pub fn check(&self) -> Result<(), AiError> {
        let expected = self.width as usize * self.height as usize * 4;
        if self.width == 0 || self.height == 0 || self.rgba.len() != expected {
            return Err(AiError::BadPicture);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_samples_between_its_pixels_and_measures_its_share() {
        let c = Coverage {
            width: 2,
            height: 1,
            data: vec![0, 255],
        };
        assert_eq!(c.at(0.0, 0.5), 0.0);
        assert_eq!(c.at(1.0, 0.5), 1.0);
        assert!((c.at(0.5, 0.5) - 0.5).abs() < 1e-6);
        assert!((c.share() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_picture_must_have_its_pixels() {
        let rgba = vec![0u8; 4 * 4 * 3];
        let picture = |height| Picture {
            width: 4,
            height,
            rgba: &rgba,
        };
        assert!(picture(3).check().is_ok());
        assert_eq!(picture(4).check(), Err(AiError::BadPicture));
    }
}
