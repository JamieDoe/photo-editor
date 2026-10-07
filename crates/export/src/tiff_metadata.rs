//! Metadata in TIFFs (ADR 0063): the Exif and GPS directories, written through the
//! `tiff` crate from the same entries as the JPEG and PNG EXIF block.

use std::borrow::Cow;
use std::io::{Seek, Write};

use tiff::TiffResult;
use tiff::encoder::{DirectoryEncoder, TiffEncoder, TiffKindStandard, TiffValue};
use tiff::tags::{Tag, Type};

use crate::metadata::{Entries, Value};

/// Where the Exif and GPS directories were written.
pub(crate) struct Pointers {
    exif: u32,
    gps: Option<u32>,
}

/// Writes the Exif directory, and the GPS directory when there is a location.
pub(crate) fn write_directories<W: Write + Seek>(
    encoder: &mut TiffEncoder<W, TiffKindStandard>,
    entries: &Entries,
) -> TiffResult<Pointers> {
    let mut dir = encoder.extra_directory()?;
    write_fields(&mut dir, &entries.exif)?;
    let exif = dir.finish_with_offsets()?.offset;
    let gps = if entries.gps.is_empty() {
        None
    } else {
        let mut dir = encoder.extra_directory()?;
        write_fields(&mut dir, &entries.gps)?;
        Some(dir.finish_with_offsets()?.offset)
    };
    Ok(Pointers { exif, gps })
}

/// The main-directory fields (make, model, orientation) and the pointers to the others.
pub(crate) fn write_main<W: Write + Seek>(
    dir: &mut DirectoryEncoder<'_, W, TiffKindStandard>,
    entries: &Entries,
    pointers: &Pointers,
) -> TiffResult<()> {
    write_fields(dir, &entries.main)?;
    // LONG, as the Exif standard defines these pointers (readers reject the IFD type).
    dir.write_tag(Tag::ExifDirectory, pointers.exif)?;
    if let Some(gps) = pointers.gps {
        dir.write_tag(Tag::GpsDirectory, gps)?;
    }
    // The photographer's marks as XMP (ADR 0067): tag 700, XMLPacket, as bytes.
    if let Some(packet) = &entries.xmp {
        align(dir)?;
        dir.write_tag(Tag::from_u16_exhaustive(XMP_PACKET), packet.as_bytes())?;
        align(dir)?;
    }
    Ok(())
}

/// TIFF's XMLPacket tag.
const XMP_PACKET: u16 = 700;

fn write_fields<W: Write + Seek>(
    dir: &mut DirectoryEncoder<'_, W, TiffKindStandard>,
    fields: &[(u16, Value)],
) -> TiffResult<()> {
    for (tag, value) in fields {
        align(dir)?;
        let tag = Tag::from_u16_exhaustive(*tag);
        match value {
            Value::Ascii(s) => dir.write_tag(tag, s.as_str())?,
            Value::Byte(b) => dir.write_tag(tag, &b[..])?,
            Value::Short(v) => dir.write_tag(tag, *v)?,
            Value::Long(v) => dir.write_tag(tag, *v)?,
            Value::Rational(r) => dir.write_tag(tag, Rationals(r))?,
            Value::Undefined(b) => dir.write_tag(tag, Undefined(b))?,
        }
    }
    // The directory itself is written where the last value ended, and TIFF requires
    // it (and readers check) to start on a word boundary.
    align(dir)
}

/// Pads the file to an even offset: values are written straight into the file as their
/// tags are added, and an odd-length string would leave the next one misaligned.
fn align<W: Write + Seek>(dir: &mut DirectoryEncoder<'_, W, TiffKindStandard>) -> TiffResult<()> {
    let at = dir.write_data(&[0u8; 0][..])?;
    if at % 2 == 1 {
        dir.write_data(0u8)?;
    }
    Ok(())
}

/// Several RATIONALs in one field (the crate has a single `Rational` only), in the
/// file's (native) byte order.
struct Rationals<'a>(&'a [(u32, u32)]);

impl TiffValue for Rationals<'_> {
    const BYTE_LEN: u8 = 8;
    const FIELD_TYPE: Type = Type::RATIONAL;

    fn count(&self) -> usize {
        self.0.len()
    }

    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Owned(
            self.0
                .iter()
                .flat_map(|(n, d)| n.to_ne_bytes().into_iter().chain(d.to_ne_bytes()))
                .collect(),
        )
    }
}

/// An UNDEFINED field (ExifVersion).
struct Undefined<'a>(&'a [u8]);

impl TiffValue for Undefined<'_> {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: Type = Type::UNDEFINED;

    fn count(&self) -> usize {
        self.0.len()
    }

    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.0)
    }
}
