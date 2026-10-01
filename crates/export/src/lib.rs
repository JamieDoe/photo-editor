//! Export: encode a rendered image and write it without ever touching the source.
//!
//! Writes are atomic (temporary file in the destination directory, then rename), so a
//! failed or cancelled export never leaves a truncated file behind.

use std::fmt;
use std::path::{Path, PathBuf};

use image_core::{OutputImage, PixelFormat};

mod icc;
pub mod resize;
pub mod sharpen;
#[cfg(feature = "turbojpeg")]
mod turbo;

/// What an export is written as (ADR 0057).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jpeg {
        quality: u8,
    },
    /// 8-bit, lossless, marked sRGB.
    Png,
    /// 16-bit, lossless (Deflate), with an sRGB profile: for printing and further
    /// editing.
    Tiff,
}

impl ExportFormat {
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Jpeg { .. } => &["jpg", "jpeg"],
            Self::Png => &["png"],
            Self::Tiff => &["tif", "tiff"],
        }
    }

    /// The pixels to render for it: 16-bit for TIFF, 8-bit otherwise.
    pub fn pixel_format(self) -> PixelFormat {
        match self {
            Self::Tiff => PixelFormat::Rgb16,
            Self::Jpeg { .. } | Self::Png => PixelFormat::Rgb8,
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

/// Encodes `image` to bytes with the preferred JPEG encoder.
pub fn encode(image: &OutputImage, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    encode_with(image, format, JpegEncoder::preferred())
}

/// Encodes with a specific JPEG encoder (benchmarks and parity tests).
pub fn encode_with(
    image: &OutputImage,
    format: ExportFormat,
    encoder: JpegEncoder,
) -> Result<Vec<u8>, ExportError> {
    let quality = match format {
        ExportFormat::Jpeg { quality } => quality.clamp(1, 100),
        ExportFormat::Png => return encode_png(image),
        ExportFormat::Tiff => return encode_tiff(image),
    };
    if image.format() == PixelFormat::Rgb16 {
        return Err(ExportError::Encode(
            "JPEG is 8-bit; got a 16-bit image".into(),
        ));
    }
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
        PixelFormat::Rgb16 => {
            return Err(ExportError::Encode(
                "JPEG is 8-bit; got a 16-bit image".into(),
            ));
        }
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

/// PNG: 8-bit RGB (or RGBA), marked as sRGB so browsers and viewers show it as
/// rendered.
fn encode_png(image: &OutputImage) -> Result<Vec<u8>, ExportError> {
    let err = |e: png::EncodingError| ExportError::Encode(e.to_string());
    let (color, depth) = match image.format() {
        PixelFormat::Rgb8 => (png::ColorType::Rgb, png::BitDepth::Eight),
        PixelFormat::Rgba8 => (png::ColorType::Rgba, png::BitDepth::Eight),
        PixelFormat::Rgb16 => (png::ColorType::Rgb, png::BitDepth::Sixteen),
    };
    let mut out = Vec::with_capacity(image.byte_size() / 2);
    let mut encoder = png::Encoder::new(&mut out, image.width(), image.height());
    encoder.set_color(color);
    encoder.set_depth(depth);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.set_compression(png::Compression::Balanced);
    let mut writer = encoder.write_header().map_err(err)?;
    match image.format() {
        // PNG samples are big-endian.
        PixelFormat::Rgb16 => {
            let be: Vec<u8> = image
                .data()
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|b| u16::from_ne_bytes(*b).to_be_bytes())
                .collect();
            writer.write_image_data(&be).map_err(err)?;
        }
        _ => writer.write_image_data(image.data()).map_err(err)?,
    }
    writer.finish().map_err(err)?;
    Ok(out)
}

/// TIFF: 16-bit RGB (or 8-bit, as given), Deflate with the horizontal predictor
/// (lossless, about half the size of uncompressed), the sRGB profile embedded, at a
/// nominal 300 ppi.
fn encode_tiff(image: &OutputImage) -> Result<Vec<u8>, ExportError> {
    use tiff::encoder::{Compression, Rational, TiffEncoder, colortype, compression::DeflateLevel};
    use tiff::tags::{Predictor, ResolutionUnit, Tag};
    let err = |e: tiff::TiffError| ExportError::Encode(e.to_string());
    let mut out = std::io::Cursor::new(Vec::with_capacity(image.byte_size() / 2));
    let mut encoder = TiffEncoder::new(&mut out)
        .map_err(err)?
        .with_compression(Compression::Deflate(DeflateLevel::Balanced))
        .with_predictor(Predictor::Horizontal);
    let profile = icc::srgb_profile();
    let (w, h) = (image.width(), image.height());
    macro_rules! write {
        ($color:ty, $data:expr) => {{
            let mut tiff = encoder.new_image::<$color>(w, h).map_err(err)?;
            tiff.resolution(ResolutionUnit::Inch, Rational { n: 300, d: 1 });
            tiff.encoder()
                .write_tag(Tag::IccProfile, &profile[..])
                .map_err(err)?;
            tiff.write_data($data).map_err(err)?;
        }};
    }
    match image.format() {
        PixelFormat::Rgb16 => {
            let samples = image.samples16().unwrap_or_default();
            write!(colortype::RGB16, &samples)
        }
        PixelFormat::Rgb8 => write!(colortype::RGB8, image.data()),
        PixelFormat::Rgba8 => write!(colortype::RGBA8, image.data()),
    }
    Ok(out.into_inner())
}

/// Writes `bytes` to `dest` atomically (see [`platform::fs::write_atomic`]).
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    if dest.file_name().is_none() {
        return Err(ExportError::InvalidDestination("no file name".into()));
    }
    platform::fs::write_atomic(dest, bytes).map_err(ExportError::Io)
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

    fn ramp16(w: u32, h: u32) -> OutputImage {
        let data: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = (i * 977 % 65536) as u16;
                [v, v / 2, 65535 - v]
            })
            .flat_map(u16::to_ne_bytes)
            .collect();
        OutputImage::from_raw(w, h, PixelFormat::Rgb16, data).unwrap()
    }

    #[test]
    fn tiff_keeps_every_16_bit_value_and_carries_the_srgb_profile() {
        let img = ramp16(37, 23);
        let bytes = encode(&img, ExportFormat::Tiff).unwrap();
        let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(dec.dimensions().unwrap(), (37, 23));
        assert_eq!(dec.colortype().unwrap(), tiff::ColorType::RGB(16));
        let profile = dec.get_tag_u8_vec(tiff::tags::Tag::IccProfile).unwrap();
        assert_eq!(profile, icc::srgb_profile());
        let tiff::decoder::DecodingResult::U16(samples) = dec.read_image().unwrap() else {
            panic!("not 16-bit");
        };
        assert_eq!(samples, img.samples16().unwrap(), "lossless");
    }

    #[test]
    fn png_is_lossless_and_marked_srgb() {
        let data: Vec<u8> = (0..31 * 17 * 3).map(|i| (i * 37 % 256) as u8).collect();
        let img = OutputImage::from_raw(31, 17, PixelFormat::Rgb8, data.clone()).unwrap();
        let bytes = encode(&img, ExportFormat::Png).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        assert!(reader.info().srgb.is_some(), "sRGB chunk");
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut buf).unwrap();
        assert_eq!((frame.width, frame.height), (31, 17));
        assert_eq!(&buf[..frame.buffer_size()], &data[..]);
    }

    #[test]
    fn formats_have_their_extensions_and_depth() {
        let src = Path::new("/photos/DSC_0001.NEF");
        for (format, ok, bad) in [
            (ExportFormat::Png, "a.png", "a.jpg"),
            (ExportFormat::Tiff, "a.TIF", "a.png"),
            (ExportFormat::Tiff, "a.tiff", "a.tifff"),
        ] {
            assert!(
                validate_destination(Path::new(ok), src, format).is_ok(),
                "{ok}"
            );
            assert!(
                validate_destination(Path::new(bad), src, format).is_err(),
                "{bad}"
            );
        }
        assert_eq!(ExportFormat::Tiff.pixel_format(), PixelFormat::Rgb16);
        assert_eq!(ExportFormat::Png.pixel_format(), PixelFormat::Rgb8);
        // JPEG never takes 16-bit pixels.
        assert!(encode(&ramp16(4, 4), JPEG).is_err());
    }
}
