//! Photo metadata read from file headers (no image decoding).

use std::io::Read;
use std::path::Path;

use crate::DecodeError;

/// Facts about a photograph from its file. Every field is optional: cameras and
/// editing tools record different subsets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhotoMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// Capture time as the camera recorded it: local wall-clock, no time zone
    /// ("2026-09-24T06:41:12"). Sorts correctly as text.
    pub captured_at: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// Dimensions as displayed (after orientation).
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Clockwise rotation needed to display the image upright: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Decimal degrees (north and east positive).
    pub gps: Option<(f64, f64)>,
}

/// Rotation for a dcraw/LibRaw `flip` value (3 = 180°, 5 = 90° CCW, 6 = 90° CW).
pub fn rotation_from_flip(flip: i32) -> u16 {
    match flip {
        3 => 180,
        5 => 270,
        6 => 90,
        _ => 0,
    }
}

/// Rotation for an EXIF Orientation value (mirrored variants map to their rotation).
pub fn rotation_from_exif(orientation: u32) -> u16 {
    match orientation {
        3 | 4 => 180,
        5 | 6 => 90,
        7 | 8 => 270,
        _ => 0,
    }
}

pub(crate) fn non_empty(s: &str) -> Option<String> {
    let t = s.trim().trim_matches('\0').trim();
    (!t.is_empty()).then(|| t.to_owned())
}

pub(crate) fn positive(v: f32) -> Option<f32> {
    (v.is_finite() && v > 0.0).then_some(v)
}

/// Reads EXIF from a JPEG. Only the file's start is read: EXIF (APP1) and the frame
/// header come before the image data.
pub(crate) fn read_jpeg(path: &Path) -> Result<PhotoMetadata, DecodeError> {
    const HEAD: u64 = 256 * 1024;
    let mut head = Vec::new();
    std::fs::File::open(path)?
        .take(HEAD)
        .read_to_end(&mut head)?;

    let mut meta = PhotoMetadata::default();
    let (mut width, mut height) = jpeg_dimensions(&head).unzip();
    if let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&head)) {
        use exif::{In, Tag, Value};
        let text = |tag: Tag| {
            exif.get_field(tag, In::PRIMARY)
                .and_then(|f| match &f.value {
                    Value::Ascii(v) => v
                        .first()
                        .and_then(|b| non_empty(&String::from_utf8_lossy(b))),
                    _ => None,
                })
        };
        let rational = |tag: Tag| {
            exif.get_field(tag, In::PRIMARY)
                .and_then(|f| match &f.value {
                    Value::Rational(v) => v.first().map(|r| r.to_f64() as f32).and_then(positive),
                    _ => None,
                })
        };
        let uint = |tag: Tag| {
            exif.get_field(tag, In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        };

        meta.camera_make = text(Tag::Make);
        meta.camera_model = text(Tag::Model);
        meta.lens = text(Tag::LensModel);
        // EXIF dates are "YYYY:MM:DD HH:MM:SS".
        meta.captured_at = text(Tag::DateTimeOriginal)
            .or_else(|| text(Tag::DateTime))
            .and_then(|d| exif_date(&d));
        meta.iso = uint(Tag::PhotographicSensitivity);
        meta.aperture = rational(Tag::FNumber);
        meta.shutter_seconds = rational(Tag::ExposureTime);
        meta.focal_length_mm = rational(Tag::FocalLength);
        meta.rotation = uint(Tag::Orientation).map_or(0, rotation_from_exif);
        width = width.or_else(|| uint(Tag::PixelXDimension));
        height = height.or_else(|| uint(Tag::PixelYDimension));
        meta.gps = gps(&exif);
    }
    if meta.rotation % 180 == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    meta.width = width;
    meta.height = height;
    Ok(meta)
}

fn exif_date(s: &str) -> Option<String> {
    // "2026:09:24 06:41:12" -> "2026-09-24T06:41:12"
    let b = s.as_bytes();
    if b.len() < 19 || !b[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}",
        &s[0..4],
        &s[5..7],
        &s[8..10],
        &s[11..19]
    ))
}

fn gps(exif: &exif::Exif) -> Option<(f64, f64)> {
    use exif::{In, Tag, Value};
    let coord = |tag: Tag, ref_tag: Tag, negative: u8| -> Option<f64> {
        let f = exif.get_field(tag, In::PRIMARY)?;
        let Value::Rational(v) = &f.value else {
            return None;
        };
        if v.len() < 3 {
            return None;
        }
        let deg = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0;
        let r = exif.get_field(ref_tag, In::PRIMARY);
        let neg = matches!(r.map(|f| &f.value), Some(Value::Ascii(a)) if a.first().and_then(|x| x.first()) == Some(&negative));
        Some(if neg { -deg } else { deg })
    };
    Some((
        coord(Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S')?,
        coord(Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W')?,
    ))
}

/// Width and height from the first SOF marker.
fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2; // after SOI
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return (w > 0 && h > 0).then_some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_mappings() {
        assert_eq!([0, 3, 5, 6].map(rotation_from_flip), [0, 180, 270, 90]);
        assert_eq!(
            [1, 3, 6, 8, 2].map(rotation_from_exif),
            [0, 180, 90, 270, 0]
        );
    }

    #[test]
    fn exif_dates_convert() {
        assert_eq!(
            exif_date("2026:09:24 06:41:12").as_deref(),
            Some("2026-09-24T06:41:12")
        );
        assert_eq!(exif_date("    :  :     :  :  "), None);
        assert_eq!(exif_date("short"), None);
    }

    #[test]
    fn jpeg_without_exif_still_has_dimensions() {
        let dir = fixtures::TempDir::new("meta-jpeg");
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, fixtures::chart_jpeg(321, 123, 90)).unwrap();
        let m = read_jpeg(&path).unwrap();
        assert_eq!((m.width, m.height), (Some(321), Some(123)));
        assert_eq!(m.camera_model, None);
        assert_eq!(m.rotation, 0);
    }

    #[test]
    fn empty_strings_are_absent() {
        assert_eq!(non_empty("  \0"), None);
        assert_eq!(non_empty(" NIKON "), Some("NIKON".into()));
        assert_eq!(positive(0.0), None);
    }
}
