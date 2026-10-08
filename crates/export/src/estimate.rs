//! Export size estimates (ADR 0068): the design's "≈ 4.2 MB" beside Export.
//!
//! A sample of the photo, rendered and encoded at the export's settings, gives its
//! bytes per pixel; the export's size follows from its pixel count. Files don't grow
//! in proportion: a larger image of the same scene has less detail per pixel, so
//! compressed formats grow by pixels to a power below 1. The sample (a preview level)
//! also packs a little more detail than the export does, hence a factor below 1. Both
//! are fitted per format to real exports of the camera fixtures (`bench --estimate`,
//! PERFORMANCE §49).

use crate::ExportFormat;

/// The long edge of the sample estimated from.
pub const SAMPLE_LONG_EDGE: u32 = 1024;

/// How a format's size grows from the sample's: `bytes = factor × sample bytes ×
/// (pixels / sample pixels) ^ exponent`, as `(exponent, factor)`. Fitted by
/// `bench --estimate` (a least-squares fit of the logs over 6 cameras × 3 sizes; JPEG
/// over qualities 85 and 95 together).
pub fn model(format: ExportFormat) -> (f64, f64) {
    match format {
        ExportFormat::Jpeg { .. } => (0.903, 0.900),
        ExportFormat::Png => (0.959, 0.937),
        ExportFormat::Tiff => (0.994, 0.985),
    }
}

/// The export's estimated size in bytes, from a sample of `sample_pixels` that encoded
/// to `sample_bytes`, for an export of `target_pixels`.
pub fn scale(
    sample_bytes: u64,
    sample_pixels: u64,
    target_pixels: u64,
    format: ExportFormat,
) -> u64 {
    if sample_pixels == 0 {
        return 0;
    }
    let ratio = target_pixels as f64 / sample_pixels as f64;
    let (exponent, factor) = model(format);
    (factor * sample_bytes as f64 * ratio.powf(exponent)).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_size_is_the_sample_by_its_factor() {
        for f in [
            ExportFormat::Jpeg { quality: 85 },
            ExportFormat::Png,
            ExportFormat::Tiff,
        ] {
            let (_, factor) = model(f);
            assert_eq!(
                scale(500_000, 1_000_000, 1_000_000, f),
                (500_000.0 * factor).round() as u64
            );
        }
        assert_eq!(scale(500_000, 0, 1_000_000, ExportFormat::Png), 0);
    }

    #[test]
    fn larger_exports_grow_no_faster_than_their_pixels() {
        for f in [
            ExportFormat::Jpeg { quality: 85 },
            ExportFormat::Png,
            ExportFormat::Tiff,
        ] {
            let four = scale(100_000, 1_000_000, 4_000_000, f);
            assert!(four > 100_000 && four <= 400_000, "{f:?}: {four}");
        }
    }
}
