//! PNG and TIFF input (ADR 0076).

use std::io::Write;
use std::path::{Path, PathBuf};

use image_core::{NeverCancel, color};

use crate::{DecodeOptions, DecodeScale, Decoder, DecoderRegistry, PngDecoder, TiffDecoder};

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
    path
}

fn png(
    width: u32,
    height: u32,
    colour: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(colour);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    out
}

fn full() -> DecodeOptions {
    DecodeOptions::new(DecodeScale::Full)
}

/// The linear value 8-bit sRGB `v` decodes to.
fn linear8(v: u8) -> u16 {
    color::srgb8_to_linear16_table()[usize::from(v)]
}

#[test]
fn pngs_decode_in_every_form() {
    let dir = fixtures::TempDir::new("png-input");
    // 8-bit RGB: two pixels.
    let rgb = write(
        dir.path(),
        "rgb.png",
        &png(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[10, 128, 250, 0, 255, 64],
        ),
    );
    let d = PngDecoder.decode(&rgb, full(), &NeverCancel).unwrap();
    assert_eq!((d.image.width(), d.image.height()), (2, 1));
    assert_eq!(
        &d.image.data()[..3],
        &[linear8(10), linear8(128), linear8(250)]
    );
    assert_eq!(d.info.kind, crate::SourceKind::Rendered);
    // 16-bit keeps its precision: 0x8000 is between two 8-bit steps.
    let wide = write(
        dir.path(),
        "wide.png",
        &png(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[0x80, 0x00, 0, 0, 0xff, 0xff],
        ),
    );
    let d = PngDecoder.decode(&wide, full(), &NeverCancel).unwrap();
    let expected = (color::srgb_to_linear(f32::from(0x8000u16) / 65535.0) * 65535.0).round() as u16;
    assert_eq!(d.image.data(), &[expected, 0, 65535]);
    assert!(d.image.data()[0] != linear8(0x7f) && d.image.data()[0] != linear8(0x80));
    // Grey becomes RGB; alpha is dropped.
    let grey = write(
        dir.path(),
        "grey.png",
        &png(
            1,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[100],
        ),
    );
    assert_eq!(
        PngDecoder
            .decode(&grey, full(), &NeverCancel)
            .unwrap()
            .image
            .data(),
        &[linear8(100); 3]
    );
    let rgba = write(
        dir.path(),
        "rgba.png",
        &png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[1, 2, 3, 0],
        ),
    );
    assert_eq!(
        PngDecoder
            .decode(&rgba, full(), &NeverCancel)
            .unwrap()
            .image
            .data(),
        &[linear8(1), linear8(2), linear8(3)]
    );
    // Metadata: the size from the header.
    let meta = PngDecoder.read_metadata(&rgb).unwrap();
    assert_eq!((meta.width, meta.height), (Some(2), Some(1)));
}

#[test]
fn large_images_are_reduced_for_previews_and_smaller_decodes() {
    let dir = fixtures::TempDir::new("png-reduce");
    let data: Vec<u8> = (0..400 * 300 * 3).map(|i| (i % 251) as u8).collect();
    let path = write(
        dir.path(),
        "big.png",
        &png(400, 300, png::ColorType::Rgb, png::BitDepth::Eight, &data),
    );
    let preview = PngDecoder
        .display_preview(&path, 100, &NeverCancel)
        .unwrap()
        .unwrap();
    assert_eq!((preview.image.width(), preview.image.height()), (100, 75));
    assert_eq!(
        (preview.embedded_width, preview.embedded_height),
        (400, 300)
    );
    let small = PngDecoder
        .decode(
            &path,
            DecodeOptions::new(DecodeScale::AtLeast(150)),
            &NeverCancel,
        )
        .unwrap();
    assert_eq!((small.image.width(), small.image.height()), (200, 150));
    assert_eq!((small.info.full_width, small.info.full_height), (400, 300));
}

fn tiff<C: tiff::encoder::colortype::ColorType>(
    width: u32,
    height: u32,
    compression: tiff::encoder::Compression,
    data: &[C::Inner],
) -> Vec<u8>
where
    [C::Inner]: tiff::encoder::TiffValue,
{
    let mut out = std::io::Cursor::new(Vec::new());
    tiff::encoder::TiffEncoder::new(&mut out)
        .unwrap()
        .with_compression(compression)
        .write_image::<C>(width, height, data)
        .unwrap();
    out.into_inner()
}

#[test]
fn tiffs_decode_8_and_16_bit_and_compressed() {
    use tiff::encoder::{Compression, DeflateLevel, colortype};
    let dir = fixtures::TempDir::new("tiff-input");
    for (name, compression) in [
        ("plain.tif", Compression::Uncompressed),
        ("lzw.tif", Compression::Lzw),
        ("deflate.tiff", Compression::Deflate(DeflateLevel::Balanced)),
    ] {
        let path = write(
            dir.path(),
            name,
            &tiff::<colortype::RGB8>(2, 1, compression, &[10, 128, 250, 0, 255, 64]),
        );
        let d = TiffDecoder.decode(&path, full(), &NeverCancel).unwrap();
        assert_eq!(
            &d.image.data()[..3],
            &[linear8(10), linear8(128), linear8(250)],
            "{name}"
        );
    }
    let wide = write(
        dir.path(),
        "wide.tif",
        &tiff::<colortype::RGB16>(1, 1, Compression::Lzw, &[0x8000, 0, 0xffff]),
    );
    let expected = (color::srgb_to_linear(f32::from(0x8000u16) / 65535.0) * 65535.0).round() as u16;
    assert_eq!(
        TiffDecoder
            .decode(&wide, full(), &NeverCancel)
            .unwrap()
            .image
            .data(),
        &[expected, 0, 65535]
    );
    let grey = write(
        dir.path(),
        "grey.tif",
        &tiff::<colortype::Gray16>(1, 1, Compression::Uncompressed, &[0xffff]),
    );
    assert_eq!(
        TiffDecoder
            .decode(&grey, full(), &NeverCancel)
            .unwrap()
            .image
            .data(),
        &[65535; 3]
    );
    // CMYK isn't supported, and says so.
    let cmyk = write(
        dir.path(),
        "cmyk.tif",
        &tiff::<colortype::CMYK8>(1, 1, Compression::Uncompressed, &[0, 0, 0, 0]),
    );
    assert!(matches!(
        TiffDecoder.decode(&cmyk, full(), &NeverCancel),
        Err(crate::DecodeError::Unsupported(_))
    ));
}

/// A little-endian TIFF: IFD0 (a 2x2 RGB strip, camera, orientation) pointing to an
/// EXIF IFD and a GPS IFD.
fn tiff_with_metadata() -> Vec<u8> {
    // Each entry: tag, type, count, value bytes.
    type Entry = (u16, u16, u32, Vec<u8>);
    let short = |v: u16| v.to_le_bytes().to_vec();
    let long = |v: u32| v.to_le_bytes().to_vec();
    let text = |s: &str| [s.as_bytes(), &[0]].concat();
    let rationals = |v: &[(u32, u32)]| {
        v.iter()
            .flat_map(|(n, d)| [n.to_le_bytes(), d.to_le_bytes()].concat())
            .collect::<Vec<u8>>()
    };
    let ifd_len = |n: usize| 2 + 12 * n + 4;
    // Layout: header, IFD0 (13 entries), EXIF IFD (6), GPS IFD (4), then values, then the strip.
    let (ifd0_at, n0, n_exif, n_gps) = (8u32, 13usize, 6usize, 4usize);
    let exif_at = ifd0_at + ifd_len(n0) as u32;
    let gps_at = exif_at + ifd_len(n_exif) as u32;
    let values_at = gps_at + ifd_len(n_gps) as u32;
    let strip: Vec<u8> = vec![200; 12];
    let ifd0: Vec<Entry> = vec![
        (0x0100, 3, 1, short(2)),
        (0x0101, 3, 1, short(2)),
        (0x0102, 3, 3, [short(8), short(8), short(8)].concat()),
        (0x0103, 3, 1, short(1)),
        (0x0106, 3, 1, short(2)),
        (0x010f, 2, 8, text("TestCam")),
        (0x0110, 2, 3, text("T1")),
        // StripOffsets: filled in once the values' length is known.
        (0x0111, 4, 1, long(0)),
        (0x0112, 3, 1, short(6)),
        (0x0115, 3, 1, short(3)),
        (0x0117, 4, 1, long(12)),
        (0x8769, 4, 1, long(exif_at)),
        (0x8825, 4, 1, long(gps_at)),
    ];
    let exif: Vec<Entry> = vec![
        (0x829a, 5, 1, rationals(&[(1, 250)])),
        (0x829d, 5, 1, rationals(&[(56, 10)])),
        (0x8827, 3, 1, short(400)),
        (0x9003, 2, 20, text("2026:10:09 12:00:00")),
        (0x920a, 5, 1, rationals(&[(35, 1)])),
        (0xa434, 2, 10, text("Test 35mm")),
    ];
    let gps: Vec<Entry> = vec![
        (1, 2, 2, text("S")),
        (2, 5, 3, rationals(&[(51, 1), (30, 1), (0, 1)])),
        (3, 2, 2, text("W")),
        (4, 5, 3, rationals(&[(0, 1), (7, 1), (30, 1)])),
    ];
    assert_eq!((ifd0.len(), exif.len(), gps.len()), (n0, n_exif, n_gps));
    let mut values: Vec<u8> = Vec::new();
    let mut ifds: Vec<u8> = Vec::new();
    for entries in [&ifd0, &exif, &gps] {
        ifds.extend((entries.len() as u16).to_le_bytes());
        for (tag, kind, count, value) in entries {
            ifds.extend(tag.to_le_bytes());
            ifds.extend(kind.to_le_bytes());
            ifds.extend(count.to_le_bytes());
            if value.len() <= 4 {
                let mut v = value.clone();
                v.resize(4, 0);
                ifds.extend(v);
            } else {
                ifds.extend((values_at + values.len() as u32).to_le_bytes());
                values.extend(value);
            }
        }
        ifds.extend(0u32.to_le_bytes());
    }
    let strip_at = values_at + values.len() as u32;
    let mut out = b"II*\0".to_vec();
    out.extend(ifd0_at.to_le_bytes());
    out.extend(ifds);
    out.extend(values);
    out.extend(&strip);
    // StripOffsets is IFD0's eighth entry: its value at 8 + 2 + 7 * 12 + 8.
    let at = (8 + 2 + 7 * 12 + 8) as usize;
    out[at..at + 4].copy_from_slice(&strip_at.to_le_bytes());
    out
}

#[test]
fn tiff_metadata_is_read_by_seeking() {
    let dir = fixtures::TempDir::new("tiff-metadata");
    let path = write(dir.path(), "meta.tif", &tiff_with_metadata());
    let meta = TiffDecoder.read_metadata(&path).unwrap();
    assert_eq!(meta.camera_make.as_deref(), Some("TestCam"));
    assert_eq!(meta.camera_model.as_deref(), Some("T1"));
    assert_eq!(meta.lens.as_deref(), Some("Test 35mm"));
    assert_eq!(meta.iso, Some(400));
    assert_eq!(meta.aperture, Some(5.6));
    assert_eq!(meta.shutter_seconds, Some(0.004));
    assert_eq!(meta.focal_length_mm, Some(35.0));
    assert_eq!(meta.captured_at.as_deref(), Some("2026-10-09T12:00:00"));
    assert_eq!(meta.rotation, 90);
    // Turned a quarter, the size is swapped.
    assert_eq!((meta.width, meta.height), (Some(2), Some(2)));
    let (lat, lon) = meta.gps.unwrap();
    assert!(
        (lat + 51.5).abs() < 1e-9 && (lon + 0.125).abs() < 1e-9,
        "{lat}, {lon}"
    );
    // The image itself decodes, with the camera's settings.
    let d = TiffDecoder.decode(&path, full(), &NeverCancel).unwrap();
    assert_eq!(d.image.data(), &[linear8(200); 12]);
    assert_eq!((d.info.make.as_str(), d.info.iso), ("TestCam", Some(400.0)));
}

#[test]
fn the_registry_offers_png_and_tiff() {
    let registry = DecoderRegistry::with_defaults();
    for ext in ["png", "tif", "tiff"] {
        assert!(registry.extensions().contains(&ext), "{ext}");
    }
    assert_eq!(
        registry.decoder_for(Path::new("a.PNG")).unwrap().name(),
        "png"
    );
    assert_eq!(
        registry.decoder_for(Path::new("a.Tif")).unwrap().name(),
        "tiff"
    );
    for ext in ["jpg", "JPEG", "png", "tif", "TIFF"] {
        assert!(crate::is_rendered_extension(ext), "{ext}");
    }
    assert!(!crate::is_rendered_extension("nef") && !crate::is_rendered_extension("dng"));
    // DNG is still LibRaw's.
    assert_ne!(
        registry.decoder_for(Path::new("a.dng")).map(|d| d.name()),
        Some("tiff")
    );
}

/// EXIF (a little-endian TIFF) recording Orientation `o`.
fn exif_orientation(o: u16) -> Vec<u8> {
    let mut t = b"II*\0".to_vec();
    t.extend(8u32.to_le_bytes());
    t.extend(1u16.to_le_bytes());
    t.extend(0x0112u16.to_le_bytes());
    t.extend(3u16.to_le_bytes());
    t.extend(1u32.to_le_bytes());
    t.extend(o.to_le_bytes());
    t.extend([0, 0]);
    t.extend(0u32.to_le_bytes());
    t
}

/// A 3 × 2 RGB PNG of distinct greys (10, 20, ..., 60 by rows), recording Orientation
/// `o` in an `eXIf` chunk.
fn oriented_png(o: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 3, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_chunk(png::chunk::ChunkType(*b"eXIf"), &exif_orientation(o))
        .unwrap();
    let data: Vec<u8> = (1..=6u8).flat_map(|v| [v * 10; 3]).collect();
    writer.write_image_data(&data).unwrap();
    writer.finish().unwrap();
    out
}

/// Where a pixel of the upright picture comes from in the stored one, for each EXIF
/// Orientation, as the EXIF standard defines them (`w`, `h`: the stored size).
fn exif_source(o: u16, x: usize, y: usize, w: usize, h: usize) -> (usize, usize) {
    match o {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (y, h - 1 - x),
        7 => (w - 1 - y, h - 1 - x),
        8 => (w - 1 - y, x),
        _ => (x, y),
    }
}

#[test]
fn every_exif_orientation_is_turned_upright() {
    let dir = fixtures::TempDir::new("orientation");
    let (w, h) = (3usize, 2usize);
    for o in 1..=8u16 {
        let path = write(dir.path(), &format!("o{o}.png"), &oriented_png(o));
        let meta = PngDecoder.read_metadata(&path).unwrap();
        assert_eq!(meta.orientation, o);
        let d = PngDecoder.decode(&path, full(), &NeverCancel).unwrap();
        let (dw, dh) = if o >= 5 { (h, w) } else { (w, h) };
        assert_eq!(
            (d.image.width() as usize, d.image.height() as usize),
            (dw, dh),
            "orientation {o}"
        );
        assert_eq!(
            (d.info.full_width as usize, d.info.full_height as usize),
            (dw, dh)
        );
        assert_eq!(d.info.orientation, o);
        // The library's size agrees.
        assert_eq!(
            (meta.width, meta.height),
            (Some(dw as u32), Some(dh as u32))
        );
        for y in 0..dh {
            for x in 0..dw {
                let (sx, sy) = exif_source(o, x, y, w, h);
                let stored = ((sy * w + sx + 1) * 10) as u8;
                assert_eq!(
                    d.image.data()[(y * dw + x) * 3],
                    linear8(stored),
                    "orientation {o} at {x}, {y}"
                );
            }
        }
        // The thumbnail likewise.
        let preview = PngDecoder
            .display_preview(&path, 1, &NeverCancel)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                preview.embedded_width as usize,
                preview.embedded_height as usize
            ),
            (dw, dh)
        );
    }
    // Without an orientation: as stored.
    let plain = write(
        dir.path(),
        "plain.png",
        &png(3, 2, png::ColorType::Rgb, png::BitDepth::Eight, &[0; 18]),
    );
    assert_eq!(
        PngDecoder
            .decode(&plain, full(), &NeverCancel)
            .unwrap()
            .info
            .orientation,
        1
    );
}
