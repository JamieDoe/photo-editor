use super::*;
use std::io::Write;

/// A TIFF whose IFD0 points to one SubIFD holding `tags` (tag, SSHORT values), in
/// little- or big-endian order.
fn tiff(little: bool, tags: &[(u16, Vec<i16>)]) -> Vec<u8> {
    let u16b = |v: u16| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let u32b = |v: u32| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let mut out = Vec::new();
    out.extend_from_slice(if little { b"II*\0" } else { b"MM\0*" });
    out.extend_from_slice(&u32b(8));
    // IFD0 at 8: one entry, SubIFDs -> 26.
    out.extend_from_slice(&u16b(1));
    out.extend_from_slice(&u16b(SUB_IFDS));
    out.extend_from_slice(&u16b(4));
    out.extend_from_slice(&u32b(1));
    out.extend_from_slice(&u32b(26));
    out.extend_from_slice(&u32b(0));
    assert_eq!(out.len(), 26);
    // The SubIFD, its values after it.
    let values_at = 26 + 2 + 12 * tags.len() as u32 + 4;
    out.extend_from_slice(&u16b(tags.len() as u16));
    let mut values = Vec::new();
    for (tag, v) in tags {
        out.extend_from_slice(&u16b(*tag));
        out.extend_from_slice(&u16b(8));
        out.extend_from_slice(&u32b(v.len() as u32));
        out.extend_from_slice(&u32b(values_at + values.len() as u32));
        for x in v {
            values.extend_from_slice(&u16b(*x as u16));
        }
    }
    out.extend_from_slice(&u32b(0));
    out.extend_from_slice(&values);
    out
}

fn read_bytes(bytes: &[u8]) -> Option<LensProfile> {
    let dir = fixtures::TempDir::new("lens-profile");
    let path = dir.path().join("photo.arw");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
    read(&path)
}

/// A count, then 16 values rising by `step`.
fn curve(step: i16) -> Vec<i16> {
    std::iter::once(16)
        .chain((0..16).map(|i| i * step))
        .collect()
}

#[test]
fn reads_sonys_distortion_and_vignetting_in_either_byte_order() {
    for little in [true, false] {
        let profile = read_bytes(&tiff(
            little,
            &[(SONY_DISTORTION, curve(-48)), (SONY_VIGNETTING, curve(800))],
        ))
        .expect("a profile");
        assert_eq!(profile.source, "Sony");
        assert_eq!(profile.knots.len(), 16);
        assert_eq!((profile.knots[0], profile.knots[15]), (0.0, 1.0));
        let d = profile.distortion.unwrap();
        // No distortion at the centre; barrel at the corner (-720 / 2^14).
        assert_eq!(d[0], 1.0);
        assert!((d[15] - (1.0 - 720.0 / 16384.0)).abs() < 1e-6);
        let v = profile.vignetting.unwrap();
        assert_eq!(v[0], 1.0);
        // 12000: 2^(0.5 - 2^(12000/8192 - 1)) = 0.543 of the centre's brightness.
        assert!((v[15] - 0.5433).abs() < 0.001, "{}", v[15]);
    }
}

#[test]
fn one_curve_is_enough_and_a_mismatch_is_none() {
    let only = read_bytes(&tiff(true, &[(SONY_DISTORTION, curve(10))])).unwrap();
    assert!(only.distortion.is_some() && only.vignetting.is_none());
    let short: Vec<i16> = std::iter::once(8).chain(0..8).collect();
    assert!(
        read_bytes(&tiff(
            true,
            &[(SONY_DISTORTION, curve(10)), (SONY_VIGNETTING, short)]
        ))
        .is_none()
    );
}

#[test]
fn files_without_it_have_none() {
    assert!(read_bytes(&tiff(true, &[(0x7000, vec![2])])).is_none());
    assert!(read_bytes(b"\xff\xd8\xff\xe0 not a tiff").is_none());
    // A count larger than the values.
    assert!(read_bytes(&tiff(true, &[(SONY_DISTORTION, vec![16, 1, 2])])).is_none());
}

#[test]
fn reads_the_camera_files_that_have_it() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/local");
    let a7r4 = dir.join("sony-a7riv-61mp-14bit-compressed.arw");
    if !a7r4.exists() {
        eprintln!("skipped: no local camera fixtures");
        return;
    }
    let p = read(&a7r4).expect("the A7R IV's 24-70 has a profile");
    assert_eq!(p.lens.as_deref(), Some("FE 24-70mm F4 ZA OSS"));
    // Barrel at 24 mm, by 4.4 % at the corners, and 0.94 stop of falloff at f/4.
    let d = p.distortion.unwrap();
    assert!((d[15] - (1.0 - 728.0 / 16384.0)).abs() < 1e-6, "{}", d[15]);
    assert!((0.5..0.55).contains(&p.vignetting.unwrap()[15]));
    for none in [
        "nikon-z6-14bit-lossless.nef",
        "canon-eos-r6-20mp.cr3",
        "fujifilm-xt3-compressed.raf",
        "ricoh-gr3.dng",
    ] {
        assert!(read(&dir.join(none)).is_none(), "{none}");
    }
}
