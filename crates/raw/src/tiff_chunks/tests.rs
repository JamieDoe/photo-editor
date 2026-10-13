//! Decoding by chunks against the whole-image decode it replaces (ADR 0082).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use image_core::NeverCancel;
use tiff::encoder::{Compression, DeflateLevel, TiffEncoder, colortype};
use tiff::tags::Tag;

use crate::rendered::{self, Raster, Samples};
use crate::{
    DecodeError, DecodeOptions, DecodeScale, DecodedImage, Decoder, PhotoMetadata, TiffDecoder,
};

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
    path
}

/// A `width` × `height` 16-bit RGB TIFF, Deflate-compressed in many small strips, with
/// `icc` as its profile.
fn tiff16(width: u32, height: u32, data: &[u16], icc: Option<&[u8]>) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut encoder = TiffEncoder::new(&mut out)
        .unwrap()
        .with_compression(Compression::Deflate(DeflateLevel::Fast));
    let mut image = encoder
        .new_image::<colortype::RGB16>(width, height)
        .unwrap();
    if let Some(icc) = icc {
        image.encoder().write_tag(Tag::IccProfile, icc).unwrap();
    }
    image.rows_per_strip(5).unwrap();
    image.write_data(data).unwrap();
    out.into_inner()
}

/// A gradient with fine detail, so averaging in the wrong place would show.
fn detailed(width: u32, height: u32) -> Vec<u16> {
    (0..width * height * 3)
        .map(|i| {
            let (p, c) = (i / 3, i % 3);
            let (x, y) = (p % width, p / width);
            let ramp = (x * 40 + y * 70 + c * 9000) % 50000;
            let detail = ((x * 7 + y * 13 + c) % 5) * 3000;
            (ramp + detail) as u16
        })
        .collect()
}

/// The image read whole by the `tiff` crate, as before ADR 0082 (pixel by pixel only).
fn read_whole(path: &Path) -> Result<Raster, DecodeError> {
    let mut reader = super::open(path)?;
    let (width, height) = reader.dimensions().unwrap();
    let (channels, _) = super::channels_of(&mut reader)?;
    let icc = reader.get_tag_u8_vec(Tag::IccProfile).ok();
    let samples = match reader.read_image().unwrap() {
        tiff::decoder::DecodingResult::U8(v) => Samples::U8(v),
        tiff::decoder::DecodingResult::U16(v) => Samples::U16(v),
        _ => unreachable!(),
    };
    Ok(Raster::new(width, height, channels, samples)?.with_profile(icc.as_deref()))
}

/// What decoding it whole and halving makes.
fn whole(path: &Path, scale: DecodeScale) -> DecodedImage {
    rendered::decoded(
        &read_whole(path).unwrap(),
        "tiff",
        PhotoMetadata::default(),
        DecodeOptions::new(scale),
        &NeverCancel,
    )
    .unwrap()
}

fn max_difference(a: &[u16], b: &[u16]) -> u16 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

#[test]
fn whole_and_reduced_decodes_match_the_whole_image_halved() {
    let dir = fixtures::TempDir::new("tiff-chunks");
    // Odd sizes: halving cuts off the last column and row.
    let (w, h) = (401, 299);
    let path = write(
        dir.path(),
        "detail.tif",
        &tiff16(w, h, &detailed(w, h), None),
    );
    for scale in [
        DecodeScale::Full,
        DecodeScale::AtLeast(300),
        DecodeScale::AtLeast(100),
        DecodeScale::AtLeast(30),
        DecodeScale::AtLeast(1),
    ] {
        let new = TiffDecoder
            .decode(&path, DecodeOptions::new(scale), &NeverCancel)
            .unwrap();
        let old = whole(&path, scale);
        assert_eq!(
            (new.image.width(), new.image.height()),
            (old.image.width(), old.image.height()),
            "{scale:?}"
        );
        assert_eq!(
            (new.info.full_width, new.info.full_height),
            (w, h),
            "{scale:?}"
        );
        let diff = max_difference(new.image.data(), old.image.data());
        if scale == DecodeScale::Full {
            assert_eq!(diff, 0);
        } else {
            // Halving rounds at every step; this rounds once.
            assert!(diff <= 4, "{scale:?}: {diff}");
        }
    }
}

/// An RGB profile with Adobe RGB's primaries and a 2.2 curve.
fn adobe_rgb() -> Vec<u8> {
    let xyz = |v: [f64; 3]| -> Vec<u8> {
        [b"XYZ ".as_slice(), &[0; 4]]
            .concat()
            .into_iter()
            .chain(
                v.iter()
                    .flat_map(|x| ((x * 65536.0).round() as i32).to_be_bytes()),
            )
            .collect()
    };
    let curve = [
        b"curv".as_slice(),
        &[0; 4],
        &1u32.to_be_bytes(),
        &563u16.to_be_bytes(),
    ]
    .concat();
    let tags: [(&[u8; 4], Vec<u8>); 6] = [
        (b"rXYZ", xyz([0.609_755_9, 0.311_124_2, 0.019_481_1])),
        (b"gXYZ", xyz([0.205_240_1, 0.625_656_0, 0.060_890_2])),
        (b"bXYZ", xyz([0.149_224_0, 0.063_219_7, 0.744_838_7])),
        (b"rTRC", curve.clone()),
        (b"gTRC", curve.clone()),
        (b"bTRC", curve),
    ];
    let mut header = vec![0u8; 128];
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(b"RGB ");
    header[20..24].copy_from_slice(b"XYZ ");
    header[36..40].copy_from_slice(b"acsp");
    let table_len = 4 + 12 * tags.len();
    let (mut table, mut data) = ((tags.len() as u32).to_be_bytes().to_vec(), Vec::new());
    for (sig, body) in &tags {
        table.extend_from_slice(*sig);
        table.extend(((128 + table_len + data.len()) as u32).to_be_bytes());
        table.extend((body.len() as u32).to_be_bytes());
        data.extend(body);
        while data.len() % 4 != 0 {
            data.push(0);
        }
    }
    let mut out = [header, table, data].concat();
    let size = out.len() as u32;
    out[..4].copy_from_slice(&size.to_be_bytes());
    out
}

#[test]
fn a_profiles_colours_are_converted_after_averaging() {
    let dir = fixtures::TempDir::new("tiff-chunks-icc");
    let (w, h) = (120, 90);
    let path = write(
        dir.path(),
        "adobe.tif",
        &tiff16(w, h, &detailed(w, h), Some(&adobe_rgb())),
    );
    let full = TiffDecoder.decode(&path, DecodeOptions::new(DecodeScale::Full), &NeverCancel);
    assert_eq!(
        max_difference(
            full.unwrap().image.data(),
            whole(&path, DecodeScale::Full).image.data()
        ),
        0
    );
    // Each pixel is converted before it is averaged, as when decoded whole: a preview
    // and an export agree, even where colours beyond sRGB are brought in.
    let scale = DecodeScale::AtLeast(30);
    let reduced = TiffDecoder
        .decode(&path, DecodeOptions::new(scale), &NeverCancel)
        .unwrap();
    let diff = max_difference(reduced.image.data(), whole(&path, scale).image.data());
    assert!(diff <= 4, "{diff}");
}

#[test]
fn thumbnails_are_reduced_in_linear_light_and_keep_the_full_size() {
    let dir = fixtures::TempDir::new("tiff-chunks-preview");
    let (w, h) = (400, 300);
    // Smooth, so averaging in linear light and in sRGB agree.
    let smooth: Vec<u16> = (0..w * h * 3)
        .map(|i| ((i / 3 % w) * 60 + (i / 3 / w) * 50 + (i % 3) * 5000) as u16)
        .collect();
    let path = write(dir.path(), "smooth.tif", &tiff16(w, h, &smooth, None));
    let preview = TiffDecoder
        .display_preview(&path, 100, &NeverCancel)
        .unwrap()
        .unwrap();
    assert_eq!((preview.image.width(), preview.image.height()), (100, 75));
    assert_eq!((preview.embedded_width, preview.embedded_height), (w, h));
    let old = rendered::preview_of(&read_whole(&path).unwrap(), 100, 1);
    let diff = preview
        .image
        .data()
        .iter()
        .zip(old.image.data())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(diff <= 2, "{diff}");
}

/// How a hand-made TIFF stores its samples.
enum Storage {
    /// Tiles of this size, padded at the right and bottom edges.
    Tiles(usize, usize),
    /// One strip each for red, green and blue.
    Planes,
}

/// An uncompressed 8-bit RGB TIFF stored as `storage`, with only `chunks` of its
/// chunks listed when given (a damaged file).
fn handmade(w: usize, h: usize, rgb: &[u8], storage: Storage, chunks: Option<usize>) -> Vec<u8> {
    let mut data: Vec<Vec<u8>> = Vec::new();
    let mut tags: Vec<(u16, u16, Vec<u32>)> = vec![
        (256, 4, vec![w as u32]),
        (257, 4, vec![h as u32]),
        (258, 3, vec![8, 8, 8]),
        (259, 3, vec![1]),
        (262, 3, vec![2]),
        (277, 3, vec![3]),
    ];
    let (offsets_tag, counts_tag) = match storage {
        Storage::Tiles(tw, th) => {
            for ty in 0..h.div_ceil(th) {
                for tx in 0..w.div_ceil(tw) {
                    let mut tile = vec![0u8; tw * th * 3];
                    for y in 0..th {
                        for x in 0..tw {
                            let (sx, sy) = (tx * tw + x, ty * th + y);
                            if sx < w && sy < h {
                                tile[(y * tw + x) * 3..][..3]
                                    .copy_from_slice(&rgb[(sy * w + sx) * 3..][..3]);
                            }
                        }
                    }
                    data.push(tile);
                }
            }
            tags.extend([(322, 3, vec![tw as u32]), (323, 3, vec![th as u32])]);
            (324, 325)
        }
        Storage::Planes => {
            for c in 0..3 {
                data.push(rgb.iter().skip(c).step_by(3).copied().collect());
            }
            tags.extend([(278, 4, vec![h as u32]), (284, 3, vec![2])]);
            (273, 279)
        }
    };
    data.truncate(chunks.unwrap_or(data.len()));
    let mut out = b"II*\0\0\0\0\0".to_vec();
    let mut offsets = Vec::new();
    for chunk in &data {
        offsets.push(out.len() as u32);
        out.extend(chunk);
    }
    tags.push((offsets_tag, 4, offsets));
    tags.push((counts_tag, 4, data.iter().map(|c| c.len() as u32).collect()));
    tags.sort_by_key(|t| t.0);
    // Values too long for their entry go before the IFD.
    let mut entries = Vec::new();
    for (tag, kind, values) in &tags {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|&v| {
                if *kind == 3 {
                    (v as u16).to_le_bytes().to_vec()
                } else {
                    v.to_le_bytes().to_vec()
                }
            })
            .collect();
        let field = if bytes.len() <= 4 {
            let mut b = bytes;
            b.resize(4, 0);
            b
        } else {
            let at = out.len() as u32;
            out.extend(bytes);
            at.to_le_bytes().to_vec()
        };
        entries.push((*tag, *kind, values.len() as u32, field));
    }
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let ifd = out.len() as u32;
    out[4..8].copy_from_slice(&ifd.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    for (tag, kind, count, field) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(field);
    }
    out.extend(0u32.to_le_bytes());
    out
}

fn pattern(w: usize, h: usize) -> Vec<u8> {
    (0..w * h * 3).map(|i| (i * 37 % 251) as u8).collect()
}

#[test]
fn tiles_and_planes_decode() {
    let dir = fixtures::TempDir::new("tiff-chunks-tiles");
    let (w, h) = (40, 35);
    let rgb = pattern(w, h);
    let table = image_core::color::srgb8_to_linear16_table();
    let expected: Vec<u16> = rgb.iter().map(|&v| table[usize::from(v)]).collect();
    let tiles = write(
        dir.path(),
        "tiles.tif",
        &handmade(w, h, &rgb, Storage::Tiles(16, 16), None),
    );
    let planes = write(
        dir.path(),
        "planes.tif",
        &handmade(w, h, &rgb, Storage::Planes, None),
    );
    let small = DecodeOptions::new(DecodeScale::AtLeast(10));
    let decode = |path: &Path, options| TiffDecoder.decode(path, options, &NeverCancel).unwrap();
    for path in [&tiles, &planes] {
        let d = decode(path, DecodeOptions::new(DecodeScale::Full));
        assert_eq!(d.image.data(), expected.as_slice(), "{path:?}");
    }
    // Reduced: tiles as halving does it, and planes as tiles.
    let reduced = decode(&tiles, small);
    assert_eq!((reduced.image.width(), reduced.image.height()), (10, 8));
    let diff = max_difference(
        reduced.image.data(),
        whole(&tiles, DecodeScale::AtLeast(10)).image.data(),
    );
    assert!(diff <= 4, "{diff}");
    assert_eq!(decode(&planes, small).image.data(), reduced.image.data());
}

#[test]
fn damage_and_cancelling_are_errors() {
    let dir = fixtures::TempDir::new("tiff-chunks-damage");
    let (w, h) = (40, 35);
    let missing = write(
        dir.path(),
        "missing.tif",
        &handmade(w, h, &pattern(w, h), Storage::Tiles(16, 16), Some(4)),
    );
    let options = DecodeOptions::new(DecodeScale::Full);
    assert!(matches!(
        TiffDecoder.decode(&missing, options, &NeverCancel),
        Err(DecodeError::Corrupt(_))
    ));
    let mut cut = tiff16(200, 150, &detailed(200, 150), None);
    let ifd = u32::from_le_bytes(cut[4..8].try_into().unwrap()) as usize;
    // The encoder writes the strips before the IFD: damage the middle ones.
    cut[ifd / 3..ifd / 2].fill(0xa5);
    let cut = write(dir.path(), "cut.tif", &cut);
    assert!(TiffDecoder.decode(&cut, options, &NeverCancel).is_err());
    assert!(TiffDecoder.display_preview(&cut, 20, &NeverCancel).is_err());

    let fine = write(
        dir.path(),
        "fine.tif",
        &tiff16(200, 150, &detailed(200, 150), None),
    );
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        TiffDecoder.decode(&fine, options, &cancelled),
        Err(DecodeError::Cancelled)
    ));
}

/// Real TIFFs in `tests/fixtures/local` (git-ignored; may have none): by chunks as read
/// whole, and reduced as halved.
#[test]
fn local_tiffs_decode_as_read_whole() {
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/local");
    check_real_tiffs(&local);
}

fn check_real_tiffs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if !crate::has_extension(&path, crate::tiff_image::EXTENSIONS) {
            continue;
        }
        for scale in [DecodeScale::Full, DecodeScale::AtLeast(512)] {
            let new = TiffDecoder
                .decode(&path, DecodeOptions::new(scale), &NeverCancel)
                .unwrap();
            let old = whole(&path, scale);
            let diff = max_difference(new.image.data(), old.image.data());
            let limit = if scale == DecodeScale::Full { 0 } else { 4 };
            assert!(diff <= limit, "{path:?} {scale:?}: {diff}");
        }
    }
}
