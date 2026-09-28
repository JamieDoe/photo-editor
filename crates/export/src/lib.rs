//! Export: encode a rendered image and write it without ever touching the source.
//!
//! Writes are atomic (temporary file in the destination directory, then rename), so a
//! failed or cancelled export never leaves a truncated file behind.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use image_core::{OutputImage, PixelFormat};

#[cfg(feature = "turbojpeg")]
mod turbo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jpeg { quality: u8 },
}

impl ExportFormat {
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Jpeg { .. } => &["jpg", "jpeg"],
        }
    }
}

#[derive(Debug)]
pub enum ExportError {
    /// Destination is the source photograph (or otherwise not allowed).
    InvalidDestination(String),
    Encode(String),
    Io(std::io::Error),
}

impl ExportError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidDestination(_) => {
                "Choose a different file name. Exports can never overwrite the original photograph."
            }
            Self::Encode(_) => "The image could not be encoded.",
            Self::Io(_) => {
                "The exported file could not be written. Check the folder exists and there is free space."
            }
        }
    }
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDestination(m) => write!(f, "invalid export destination: {m}"),
            Self::Encode(m) => write!(f, "encoding failed: {m}"),
            Self::Io(e) => write!(f, "write failed: {e}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Rejects destinations that would overwrite the source photograph or that do not
/// match the export format.
pub fn validate_destination(
    dest: &Path,
    source: &Path,
    format: ExportFormat,
) -> Result<(), ExportError> {
    let ext_ok = dest.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        format
            .extensions()
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    });
    if !ext_ok {
        return Err(ExportError::InvalidDestination(format!(
            "{} does not have a {} extension",
            dest.display(),
            format.extensions().join("/")
        )));
    }
    if same_file(dest, source) {
        return Err(ExportError::InvalidDestination(format!(
            "{} is the source photograph",
            dest.display()
        )));
    }
    Ok(())
}

/// JPEG encoder implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpegEncoder {
    /// libjpeg-turbo (SIMD). Available with the `turbojpeg` feature.
    Turbo,
    /// Pure-Rust `jpeg-encoder`. Always available; portable fallback.
    PureRust,
}

impl JpegEncoder {
    /// The fastest encoder compiled into this build.
    pub const fn preferred() -> Self {
        if cfg!(feature = "turbojpeg") {
            Self::Turbo
        } else {
            Self::PureRust
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Turbo => "libjpeg-turbo",
            Self::PureRust => "jpeg-encoder",
        }
    }
}

/// Encodes `image` to bytes with the preferred encoder.
pub fn encode(image: &OutputImage, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    encode_with(image, format, JpegEncoder::preferred())
}

/// Encodes with a specific encoder (benchmarks and parity tests).
pub fn encode_with(
    image: &OutputImage,
    format: ExportFormat,
    encoder: JpegEncoder,
) -> Result<Vec<u8>, ExportError> {
    let ExportFormat::Jpeg { quality } = format;
    let quality = quality.clamp(1, 100);
    match encoder {
        #[cfg(feature = "turbojpeg")]
        JpegEncoder::Turbo => turbo::encode(image, quality),
        #[cfg(not(feature = "turbojpeg"))]
        JpegEncoder::Turbo => Err(ExportError::Encode(
            "built without the turbojpeg feature".into(),
        )),
        JpegEncoder::PureRust => encode_pure_rust(image, quality),
    }
}

fn encode_pure_rust(image: &OutputImage, quality: u8) -> Result<Vec<u8>, ExportError> {
    let (w, h) = (to_u16(image.width())?, to_u16(image.height())?);
    let color = match image.format() {
        PixelFormat::Rgb8 => jpeg_encoder::ColorType::Rgb,
        PixelFormat::Rgba8 => jpeg_encoder::ColorType::Rgba,
    };
    let mut out = Vec::with_capacity(image.byte_size() / 4);
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    // 4:4:4 chroma: photographs are the product, not bandwidth.
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    encoder
        .encode(image.data(), w, h, color)
        .map_err(|e| ExportError::Encode(e.to_string()))?;
    Ok(out)
}

/// Writes `bytes` to `dest` atomically.
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let dir = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = dest
        .file_name()
        .ok_or_else(|| ExportError::InvalidDestination("no file name".into()))?;
    let tmp: PathBuf = dir.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(ExportError::Io)
}

fn to_u16(v: u32) -> Result<u16, ExportError> {
    u16::try_from(v)
        .map_err(|_| ExportError::Encode(format!("dimension {v} exceeds JPEG limit 65535")))
}

fn same_file(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| -> Option<PathBuf> {
        if let Ok(c) = p.canonicalize() {
            return Some(c);
        }
        // Destination may not exist yet: canonicalise its directory.
        let parent = p
            .parent()
            .filter(|x| !x.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Some(parent.canonicalize().ok()?.join(p.file_name()?))
    };
    match (canon(a), canon(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: ExportFormat = ExportFormat::Jpeg { quality: 90 };

    #[test]
    fn refuses_to_overwrite_source() {
        let dir = fixtures::TempDir::new("export-src");
        let src = dir.path().join("photo.jpg");
        std::fs::write(&src, b"original").unwrap();
        let err = validate_destination(&src, &src, JPEG).unwrap_err();
        assert!(matches!(err, ExportError::InvalidDestination(_)));
        // Also through a non-canonical path.
        let indirect = dir.path().join(".").join("photo.jpg");
        assert!(validate_destination(&indirect, &src, JPEG).is_err());
        assert_eq!(std::fs::read(&src).unwrap(), b"original");
    }

    #[test]
    fn requires_matching_extension() {
        let dir = fixtures::TempDir::new("export-ext");
        let src = dir.path().join("photo.nef");
        assert!(validate_destination(&dir.path().join("out.png"), &src, JPEG).is_err());
        assert!(validate_destination(&dir.path().join("out.JPG"), &src, JPEG).is_ok());
    }

    fn decode(bytes: Vec<u8>) -> (u16, u16, Vec<u8>) {
        let mut dec = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
        let pixels = dec.decode().unwrap();
        let info = dec.info().unwrap();
        (info.width, info.height, pixels)
    }

    fn encoders() -> Vec<JpegEncoder> {
        let mut e = vec![JpegEncoder::PureRust];
        if cfg!(feature = "turbojpeg") {
            e.push(JpegEncoder::Turbo);
        }
        e
    }

    #[test]
    fn all_encoders_round_trip_the_chart_with_similar_fidelity() {
        // Chart rendered to sRGB 8-bit, in both pixel layouts.
        let linear = fixtures::chart_linear(301, 203);
        let rgb: Vec<u8> = linear
            .data()
            .iter()
            .map(|&v| {
                (image_core::color::linear_to_srgb(f32::from(v) / 65535.0) * 255.0).round() as u8
            })
            .collect();
        let rgba: Vec<u8> = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        for encoder in encoders() {
            for img in [
                OutputImage::from_raw(301, 203, PixelFormat::Rgb8, rgb.clone()).unwrap(),
                OutputImage::from_raw(301, 203, PixelFormat::Rgba8, rgba.clone()).unwrap(),
            ] {
                let (w, h, px) = decode(encode_with(&img, JPEG, encoder).unwrap());
                assert_eq!((w, h), (301, 203), "{encoder:?}");
                let mse: f64 = px
                    .iter()
                    .zip(&rgb)
                    .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
                    .sum::<f64>()
                    / px.len() as f64;
                let psnr = 10.0 * (255.0f64 * 255.0 / mse).log10();
                assert!(
                    psnr > 38.0,
                    "{encoder:?} {:?}: PSNR {psnr:.1} dB",
                    img.format()
                );
            }
        }
    }

    #[test]
    fn preferred_encoder_matches_build_features() {
        let expected = if cfg!(feature = "turbojpeg") {
            "libjpeg-turbo"
        } else {
            "jpeg-encoder"
        };
        assert_eq!(JpegEncoder::preferred().name(), expected);
    }

    #[test]
    fn encodes_decodable_jpeg_of_right_size() {
        let img = OutputImage::from_raw(33, 17, PixelFormat::Rgb8, vec![128; 33 * 17 * 3]).unwrap();
        let bytes = encode(&img, JPEG).unwrap();
        let mut dec = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
        let pixels = dec.decode().unwrap();
        let info = dec.info().unwrap();
        assert_eq!((info.width, info.height), (33, 17));
        assert!(pixels.iter().all(|&v| v.abs_diff(128) <= 2));
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = fixtures::TempDir::new("export-atomic");
        let dest = dir.path().join("out.jpg");
        write_atomic(&dest, b"one").unwrap();
        write_atomic(&dest, b"two").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    #[test]
    fn write_to_missing_directory_fails_cleanly() {
        let err = write_atomic(Path::new("/nonexistent-dir/out.jpg"), b"x").unwrap_err();
        assert!(matches!(err, ExportError::Io(_)));
    }
}
