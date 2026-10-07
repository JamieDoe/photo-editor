//! Metadata written into exports (ADR 0063): the design's "Keep metadata" and "Strip
//! location" switches.
//!
//! The photo's capture facts (camera, lens, exposure, capture time and, unless
//! stripped, where it was taken) are written as EXIF: an APP1 segment in JPEGs, an
//! `eXIf` chunk in PNGs, and Exif and GPS directories in TIFFs. The same entries feed
//! all three, so the formats cannot drift apart.
//!
//! Only facts are copied, never the source file's maker notes or thumbnails: those can
//! hold serial numbers and other details nobody chose to share, and describe the
//! original rather than the export.
//!
//! The photographer's own marks (ADR 0067), the star rating, a reject and the colour
//! label, go with them as XMP, where other photo tools read them: an APP1 segment in
//! JPEGs, an `iTXt` chunk in PNGs, tag 700 in TIFFs.

use crate::colour::ExportColourSpace;

/// What an export carries, from the dialog's switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetadataChoice {
    /// Camera, lens, exposure, capture time and location.
    #[default]
    All,
    /// All of it except the location.
    WithoutLocation,
    /// Nothing: no EXIF at all (the colour profile is still embedded).
    None,
}

impl MetadataChoice {
    /// The dialog's two switches; with metadata off, location goes with it.
    pub fn from_switches(keep_metadata: bool, strip_location: bool) -> Self {
        match (keep_metadata, strip_location) {
            (false, _) => Self::None,
            (true, true) => Self::WithoutLocation,
            (true, false) => Self::All,
        }
    }
}

/// The capture facts of the source photo. Every field is optional: cameras record
/// different subsets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaptureFacts {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// "2026-09-24T06:41:12": local wall-clock time, as the camera recorded it.
    pub captured_at: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// Decimal degrees, north and east positive.
    pub gps: Option<(f64, f64)>,
}

/// The photographer's marks on a photo, as other photo tools read them from XMP
/// (ADR 0067).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Judgements {
    /// 0 (unrated) to 5 stars.
    pub rating: u8,
    /// Rejected: written as a rating of -1, as Bridge and Lightroom do.
    pub rejected: bool,
    pub label: Option<LabelName>,
}

/// A colour label, by the names Lightroom and Bridge write and read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelName {
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl LabelName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
        }
    }
}

/// The XMP packet for `judgements`, or `None` when there is nothing to say (unrated,
/// not rejected, unlabelled). Pick flags have no common XMP property and are not
/// written.
pub fn xmp_packet(judgements: &Judgements) -> Option<String> {
    let rating = if judgements.rejected {
        Some(-1)
    } else {
        (judgements.rating > 0).then(|| i32::from(judgements.rating.min(5)))
    };
    if rating.is_none() && judgements.label.is_none() {
        return None;
    }
    let mut properties = String::new();
    if let Some(r) = rating {
        properties.push_str(&format!(" xmp:Rating=\"{r}\""));
    }
    if let Some(l) = judgements.label {
        properties.push_str(&format!(" xmp:Label=\"{}\"", l.as_str()));
    }
    Some(format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n \
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  \
         <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"{properties}/>\n \
         </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>"
    ))
}

/// The JPEG APP1 segment's signature for XMP.
const XMP_SIGNATURE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// `jpeg` with `packet` as an APP1 XMP segment, after SOI and the JFIF APP0 segment.
pub fn jpeg_with_xmp(jpeg: &[u8], packet: &str) -> Vec<u8> {
    let mut segment = vec![0xFF, 0xE1];
    segment.extend(((2 + XMP_SIGNATURE.len() + packet.len()) as u16).to_be_bytes());
    segment.extend(XMP_SIGNATURE);
    segment.extend(packet.as_bytes());
    crate::colour::insert_after_app0(jpeg, &segment)
}

/// An EXIF field value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Ascii(String),
    Byte(Vec<u8>),
    Short(u16),
    Long(u32),
    /// Unsigned rationals, numerator and denominator.
    Rational(Vec<(u32, u32)>),
    Undefined(Vec<u8>),
}

/// The fields of the three directories an export can carry, each sorted by tag.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entries {
    /// The main directory (IFD0): make, model, orientation.
    pub main: Vec<(u16, Value)>,
    /// The Exif directory: exposure, capture time, lens, colour space, size.
    pub exif: Vec<(u16, Value)>,
    /// The GPS directory; empty when there is no location or it is stripped.
    pub gps: Vec<(u16, Value)>,
    /// The photographer's marks as an XMP packet (ADR 0067), if any.
    pub xmp: Option<String>,
}

pub mod tag {
    pub const MAKE: u16 = 271;
    pub const MODEL: u16 = 272;
    pub const ORIENTATION: u16 = 274;
    pub const EXIF_IFD: u16 = 0x8769;
    pub const GPS_IFD: u16 = 0x8825;
    pub const EXPOSURE_TIME: u16 = 33434;
    pub const F_NUMBER: u16 = 33437;
    pub const ISO: u16 = 34855;
    pub const EXIF_VERSION: u16 = 36864;
    pub const DATE_TIME_ORIGINAL: u16 = 36867;
    pub const FOCAL_LENGTH: u16 = 37386;
    pub const COLOR_SPACE: u16 = 40961;
    pub const PIXEL_X: u16 = 40962;
    pub const PIXEL_Y: u16 = 40963;
    pub const LENS_MODEL: u16 = 42036;
    pub const GPS_VERSION: u16 = 0;
    pub const GPS_LATITUDE_REF: u16 = 1;
    pub const GPS_LATITUDE: u16 = 2;
    pub const GPS_LONGITUDE_REF: u16 = 3;
    pub const GPS_LONGITUDE: u16 = 4;
}

/// The fields for an export of `width`×`height` in `space`, or `None` when the choice
/// is to write no metadata.
pub fn entries(
    facts: &CaptureFacts,
    judgements: &Judgements,
    choice: MetadataChoice,
    width: u32,
    height: u32,
    space: ExportColourSpace,
) -> Option<Entries> {
    if choice == MetadataChoice::None {
        return None;
    }
    let mut main = Vec::new();
    if let Some(make) = text(&facts.camera_make) {
        main.push((tag::MAKE, Value::Ascii(make.to_owned())));
    }
    if let Some(model) = text(&facts.camera_model) {
        main.push((tag::MODEL, Value::Ascii(model.to_owned())));
    }
    // The pixels are written upright.
    main.push((tag::ORIENTATION, Value::Short(1)));

    let mut exif = vec![(tag::EXIF_VERSION, Value::Undefined(b"0232".to_vec()))];
    if let Some(t) = facts.shutter_seconds.and_then(exposure_time) {
        exif.push((tag::EXPOSURE_TIME, Value::Rational(vec![t])));
    }
    if let Some(f) = facts.aperture.and_then(|v| tenths(f64::from(v))) {
        exif.push((tag::F_NUMBER, Value::Rational(vec![f])));
    }
    if let Some(iso) = facts.iso.filter(|&v| v > 0) {
        exif.push((tag::ISO, Value::Short(iso.min(65535) as u16)));
    }
    if let Some(date) = facts.captured_at.as_deref().and_then(exif_date) {
        exif.push((tag::DATE_TIME_ORIGINAL, Value::Ascii(date)));
    }
    if let Some(f) = facts.focal_length_mm.and_then(|v| tenths(f64::from(v))) {
        exif.push((tag::FOCAL_LENGTH, Value::Rational(vec![f])));
    }
    // 1 is sRGB; any other space is "uncalibrated", described by the embedded profile.
    let colour = if space == ExportColourSpace::Srgb {
        1
    } else {
        0xFFFF
    };
    exif.push((tag::COLOR_SPACE, Value::Short(colour)));
    exif.push((tag::PIXEL_X, Value::Long(width)));
    exif.push((tag::PIXEL_Y, Value::Long(height)));
    if let Some(lens) = text(&facts.lens) {
        exif.push((tag::LENS_MODEL, Value::Ascii(lens.to_owned())));
    }

    let mut gps = Vec::new();
    if let (MetadataChoice::All, Some((lat, lon))) = (choice, facts.gps)
        && lat.is_finite()
        && lon.is_finite()
        && lat.abs() <= 90.0
        && lon.abs() <= 180.0
    {
        gps.push((tag::GPS_VERSION, Value::Byte(vec![2, 3, 0, 0])));
        let hemisphere =
            |v: f64, pos: &str, neg: &str| (if v < 0.0 { neg } else { pos }).to_owned();
        gps.push((
            tag::GPS_LATITUDE_REF,
            Value::Ascii(hemisphere(lat, "N", "S")),
        ));
        gps.push((tag::GPS_LATITUDE, Value::Rational(degrees(lat))));
        gps.push((
            tag::GPS_LONGITUDE_REF,
            Value::Ascii(hemisphere(lon, "E", "W")),
        ));
        gps.push((tag::GPS_LONGITUDE, Value::Rational(degrees(lon))));
    }
    Some(Entries {
        main,
        exif,
        gps,
        xmp: xmp_packet(judgements),
    })
}

fn text(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Exposure time as a rational: 1/250 for fast shutters, tenths of a second for slow.
fn exposure_time(seconds: f32) -> Option<(u32, u32)> {
    let s = f64::from(seconds);
    if !(s.is_finite() && s > 0.0) {
        return None;
    }
    if s < 1.0 {
        let denominator = (1.0 / s).round();
        // Exact fractions (1/3 s is not 1/3.0) when they are not whole numbers.
        if ((1.0 / denominator) - s).abs() / s < 0.01 {
            return Some((1, denominator as u32));
        }
        return Some(((s * 10_000.0).round() as u32, 10_000));
    }
    tenths(s)
}

fn tenths(v: f64) -> Option<(u32, u32)> {
    (v.is_finite() && v > 0.0 && v < 400_000_000.0).then(|| ((v * 10.0).round() as u32, 10))
}

/// Degrees, minutes and seconds (to a thousandth of a second, about 3 cm).
fn degrees(v: f64) -> Vec<(u32, u32)> {
    let v = v.abs();
    let d = v.trunc();
    let m = ((v - d) * 60.0).trunc();
    let s = ((v - d) * 60.0 - m) * 60.0;
    vec![
        (d as u32, 1),
        (m as u32, 1),
        ((s * 1000.0).round() as u32, 1000),
    ]
}

/// "2026-09-24T06:41:12" as EXIF's "2026:09:24 06:41:12".
fn exif_date(iso: &str) -> Option<String> {
    let b = iso.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    let ok = b.len() >= 19
        && digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19);
    ok.then(|| {
        format!(
            "{}:{}:{} {}",
            &iso[0..4],
            &iso[5..7],
            &iso[8..10],
            &iso[11..19]
        )
    })
}

/// The entries as a standalone EXIF block (a little-endian TIFF structure): what a JPEG
/// APP1 segment holds after "Exif\0\0", and what a PNG `eXIf` chunk holds.
pub fn exif_block(entries: &Entries) -> Vec<u8> {
    // Layout: header, IFD0, Exif IFD, GPS IFD, then the values too long to sit in
    // their entries. Each IFD: count, 12-byte entries, next-IFD offset.
    let ifd_len = |n: usize| 2 + 12 * n + 4;
    let has_gps = !entries.gps.is_empty();
    let main_len = entries.main.len() + 1 + usize::from(has_gps);
    let main_at = 8;
    let exif_at = main_at + ifd_len(main_len);
    let gps_at = exif_at + ifd_len(entries.exif.len());
    let mut data_at = gps_at
        + if has_gps {
            ifd_len(entries.gps.len())
        } else {
            0
        };

    let mut main = entries.main.clone();
    main.push((tag::EXIF_IFD, Value::Long(exif_at as u32)));
    if has_gps {
        main.push((tag::GPS_IFD, Value::Long(gps_at as u32)));
    }
    main.sort_by_key(|(t, _)| *t);

    let mut out = b"II*\0".to_vec();
    out.extend(8u32.to_le_bytes());
    let mut data = Vec::new();
    let mut ifd = |fields: &[(u16, Value)], out: &mut Vec<u8>| {
        let mut fields = fields.to_vec();
        fields.sort_by_key(|(t, _)| *t);
        out.extend((fields.len() as u16).to_le_bytes());
        for (tag, value) in &fields {
            let (kind, count, bytes) = encode(value);
            out.extend(tag.to_le_bytes());
            out.extend(kind.to_le_bytes());
            out.extend(count.to_le_bytes());
            if bytes.len() <= 4 {
                let mut inline = bytes.clone();
                inline.resize(4, 0);
                out.extend(inline);
            } else {
                out.extend((data_at as u32).to_le_bytes());
                data_at += bytes.len() + bytes.len() % 2;
                data.extend(&bytes);
                if bytes.len() % 2 == 1 {
                    data.push(0);
                }
            }
        }
        out.extend(0u32.to_le_bytes());
    };
    ifd(&main, &mut out);
    ifd(&entries.exif, &mut out);
    if has_gps {
        ifd(&entries.gps, &mut out);
    }
    out.extend(data);
    out
}

/// TIFF field type, count and little-endian bytes of a value.
fn encode(value: &Value) -> (u16, u32, Vec<u8>) {
    match value {
        Value::Byte(b) => (1, b.len() as u32, b.clone()),
        Value::Ascii(s) => {
            let mut b = s.as_bytes().to_vec();
            b.push(0);
            (2, b.len() as u32, b)
        }
        Value::Short(v) => (3, 1, v.to_le_bytes().to_vec()),
        Value::Long(v) => (4, 1, v.to_le_bytes().to_vec()),
        Value::Rational(r) => (
            5,
            r.len() as u32,
            r.iter()
                .flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes()))
                .collect(),
        ),
        Value::Undefined(b) => (7, b.len() as u32, b.clone()),
    }
}

/// `jpeg` with `block` (from [`exif_block`]) as an APP1 Exif segment, right after SOI
/// and the JFIF APP0 segment if there is one.
pub fn jpeg_with_exif(jpeg: &[u8], block: &[u8]) -> Vec<u8> {
    let mut segment = vec![0xFF, 0xE1];
    segment.extend(((2 + 6 + block.len()) as u16).to_be_bytes());
    segment.extend(b"Exif\0\0");
    segment.extend(block);
    crate::colour::insert_after_app0(jpeg, &segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> CaptureFacts {
        CaptureFacts {
            camera_make: Some("NIKON CORPORATION".into()),
            camera_model: Some("NIKON Z 6".into()),
            lens: Some("NIKKOR Z 24-70mm f/4 S".into()),
            captured_at: Some("2026-09-24T06:41:12".into()),
            iso: Some(400),
            aperture: Some(5.6),
            shutter_seconds: Some(1.0 / 250.0),
            focal_length_mm: Some(35.0),
            gps: Some((54.4609, -3.0886)),
        }
    }

    fn read(block: &[u8]) -> exif::Exif {
        exif::Reader::new()
            .read_raw(block.to_vec())
            .expect("a valid EXIF block")
    }

    #[test]
    fn marks_are_written_as_other_tools_read_them() {
        let packet = |rating, rejected, label| {
            xmp_packet(&Judgements {
                rating,
                rejected,
                label,
            })
        };
        assert_eq!(packet(0, false, None), None, "nothing to say");
        let p = packet(3, false, Some(LabelName::Red)).unwrap();
        assert!(
            p.contains(r#"xmp:Rating="3""#) && p.contains(r#"xmp:Label="Red""#),
            "{p}"
        );
        assert!(p.starts_with("<?xpacket begin=") && p.ends_with(r#"<?xpacket end="w"?>"#));
        assert!(p.contains(r#"xmlns:xmp="http://ns.adobe.com/xap/1.0/""#));
        // A reject is -1, whatever the stars; a label alone has no rating.
        assert!(
            packet(5, true, None)
                .unwrap()
                .contains(r#"xmp:Rating="-1""#)
        );
        let label_only = packet(0, false, Some(LabelName::Green)).unwrap();
        assert!(!label_only.contains("xmp:Rating") && label_only.contains(r#"xmp:Label="Green""#));
    }

    #[test]
    fn jpegs_get_xmp_after_jfif() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        jpeg.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        jpeg.extend([0xFF, 0xDB, 0x00, 0x02]);
        let p = xmp_packet(&Judgements {
            rating: 2,
            ..Default::default()
        })
        .unwrap();
        let out = jpeg_with_xmp(&jpeg, &p);
        let at = 2 + 2 + 16;
        assert_eq!(&out[at..at + 2], &[0xFF, 0xE1]);
        let len = usize::from(u16::from_be_bytes([out[at + 2], out[at + 3]]));
        assert_eq!(&out[at + 4..at + 4 + XMP_SIGNATURE.len()], XMP_SIGNATURE);
        assert_eq!(
            &out[at + 4 + XMP_SIGNATURE.len()..at + 2 + len],
            p.as_bytes()
        );
        assert_eq!(&out[out.len() - 4..], &[0xFF, 0xDB, 0x00, 0x02]);
    }

    #[test]
    fn switches_choose_what_is_written() {
        assert_eq!(
            MetadataChoice::from_switches(true, false),
            MetadataChoice::All
        );
        assert_eq!(
            MetadataChoice::from_switches(true, true),
            MetadataChoice::WithoutLocation
        );
        assert_eq!(
            MetadataChoice::from_switches(false, false),
            MetadataChoice::None
        );
        assert_eq!(
            MetadataChoice::from_switches(false, true),
            MetadataChoice::None
        );
        assert!(
            entries(
                &facts(),
                &Judgements::default(),
                MetadataChoice::None,
                10,
                10,
                ExportColourSpace::Srgb
            )
            .is_none()
        );
    }

    #[test]
    fn every_fact_reads_back() {
        use exif::{In, Tag};
        let e = entries(
            &facts(),
            &Judgements::default(),
            MetadataChoice::All,
            1350,
            899,
            ExportColourSpace::Srgb,
        )
        .unwrap();
        let x = read(&exif_block(&e));
        let shown = |t: Tag| {
            x.get_field(t, In::PRIMARY)
                .map(|f| f.display_value().with_unit(&x).to_string())
                .unwrap_or_default()
        };
        assert_eq!(shown(Tag::Make), "\"NIKON CORPORATION\"");
        assert_eq!(shown(Tag::Model), "\"NIKON Z 6\"");
        assert_eq!(shown(Tag::LensModel), "\"NIKKOR Z 24-70mm f/4 S\"");
        assert_eq!(shown(Tag::DateTimeOriginal), "2026-09-24 06:41:12");
        assert_eq!(shown(Tag::ExposureTime), "1/250 s");
        assert_eq!(shown(Tag::FNumber), "f/5.6");
        assert_eq!(shown(Tag::PhotographicSensitivity), "400");
        assert_eq!(shown(Tag::FocalLength), "35 mm");
        assert_eq!(shown(Tag::Orientation), "row 0 at top and column 0 at left");
        assert_eq!(shown(Tag::ColorSpace), "sRGB");
        assert_eq!(shown(Tag::PixelXDimension), "1350 pixels");
        assert_eq!(shown(Tag::PixelYDimension), "899 pixels");
        // 54.4609° N, 3.0886° W.
        let lat = x.get_field(Tag::GPSLatitude, In::PRIMARY).unwrap();
        let exif::Value::Rational(r) = &lat.value else {
            panic!()
        };
        let deg = r[0].to_f64() + r[1].to_f64() / 60.0 + r[2].to_f64() / 3600.0;
        assert!((deg - 54.4609).abs() < 1e-6, "{deg}");
        assert_eq!(shown(Tag::GPSLatitudeRef), "N");
        assert_eq!(shown(Tag::GPSLongitudeRef), "W");
    }

    #[test]
    fn stripping_location_leaves_everything_else() {
        use exif::{In, Tag};
        let e = entries(
            &facts(),
            &Judgements::default(),
            MetadataChoice::WithoutLocation,
            100,
            100,
            ExportColourSpace::AdobeRgb,
        )
        .unwrap();
        assert!(e.gps.is_empty());
        let x = read(&exif_block(&e));
        assert!(x.get_field(Tag::GPSLatitude, In::PRIMARY).is_none());
        assert!(x.get_field(Tag::Make, In::PRIMARY).is_some());
        // A wide space is "uncalibrated": the embedded profile describes it.
        assert_eq!(
            x.get_field(Tag::ColorSpace, In::PRIMARY)
                .unwrap()
                .value
                .get_uint(0),
            Some(0xFFFF)
        );
    }

    #[test]
    fn missing_facts_are_left_out() {
        use exif::{In, Tag};
        let e = entries(
            &CaptureFacts::default(),
            &Judgements::default(),
            MetadataChoice::All,
            64,
            48,
            ExportColourSpace::Srgb,
        )
        .unwrap();
        let x = read(&exif_block(&e));
        for t in [
            Tag::Make,
            Tag::Model,
            Tag::ExposureTime,
            Tag::FNumber,
            Tag::GPSLatitude,
            Tag::LensModel,
        ] {
            assert!(x.get_field(t, In::PRIMARY).is_none(), "{t}");
        }
        assert!(x.get_field(Tag::PixelXDimension, In::PRIMARY).is_some());
    }

    #[test]
    fn exposure_times_are_written_as_photographers_read_them() {
        assert_eq!(exposure_time(1.0 / 250.0), Some((1, 250)));
        assert_eq!(exposure_time(1.0 / 8000.0), Some((1, 8000)));
        assert_eq!(exposure_time(0.3), Some((3000, 10_000)));
        assert_eq!(exposure_time(2.5), Some((25, 10)));
        assert_eq!(exposure_time(30.0), Some((300, 10)));
        assert_eq!(exposure_time(0.0), None);
        assert_eq!(exposure_time(f32::NAN), None);
    }

    #[test]
    fn dates_and_degrees_convert() {
        assert_eq!(
            exif_date("2026-09-24T06:41:12").as_deref(),
            Some("2026:09:24 06:41:12")
        );
        assert_eq!(exif_date("2026-09-24"), None);
        assert_eq!(exif_date("garbage-in-a-long-string"), None);
        let d = degrees(-3.0886);
        assert_eq!(d[0], (3, 1));
        assert_eq!(d[1], (5, 1));
        assert_eq!(d[2], (18960, 1000));
    }

    #[test]
    fn jpegs_get_exif_after_jfif() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        jpeg.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        jpeg.extend([0xFF, 0xDB, 0x00, 0x02]);
        let e = entries(
            &facts(),
            &Judgements::default(),
            MetadataChoice::All,
            8,
            8,
            ExportColourSpace::Srgb,
        )
        .unwrap();
        let block = exif_block(&e);
        let out = jpeg_with_exif(&jpeg, &block);
        let at = 2 + 2 + 16;
        assert_eq!(&out[at..at + 2], &[0xFF, 0xE1]);
        assert_eq!(&out[at + 4..at + 10], b"Exif\0\0");
        let x = exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(&out))
            .unwrap();
        assert!(x.get_field(exif::Tag::Model, exif::In::PRIMARY).is_some());
    }
}
