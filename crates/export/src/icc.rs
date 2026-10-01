//! A compact sRGB ICC profile (version 2.1, a matrix/TRC display profile), generated
//! here rather than shipped as a file (ADR 0057), so its terms are ours. Embedded in
//! TIFF exports so colour-managed applications (Photoshop, print RIPs) read the pixels
//! as sRGB instead of guessing.
//!
//! The colorants are sRGB's primaries adapted to the profile connection space's D50
//! white with the Bradford transform; the tone curve is the sRGB curve sampled at 1024
//! points; the media white is D65, as version 2 display profiles record it.

/// s15Fixed16 numbers, as ICC writes XYZ values.
fn fixed(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for v in [x, y, z] {
        t.extend(fixed(v));
    }
    t
}

/// The sRGB curve (encoded value to linear light) as a 1024-point table.
fn curve_tag() -> Vec<u8> {
    const POINTS: u32 = 1024;
    let mut t = b"curv\0\0\0\0".to_vec();
    t.extend(POINTS.to_be_bytes());
    for i in 0..POINTS {
        let v = f64::from(i) / f64::from(POINTS - 1);
        let linear = if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        t.extend(((linear * 65535.0).round() as u16).to_be_bytes());
    }
    t
}

/// A version 2 textDescriptionType: ASCII, with empty Unicode and ScriptCode parts.
fn description_tag(text: &str) -> Vec<u8> {
    let mut t = b"desc\0\0\0\0".to_vec();
    t.extend((text.len() as u32 + 1).to_be_bytes());
    t.extend(text.as_bytes());
    t.push(0);
    t.extend([0u8; 4 + 4]); // Unicode language code and count
    t.extend([0u8; 2 + 1 + 67]); // ScriptCode code, count and its fixed 67 bytes
    t
}

fn text_tag(text: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend(text.as_bytes());
    t.push(0);
    t
}

/// The profile's bytes.
pub fn srgb_profile() -> Vec<u8> {
    let curve = curve_tag();
    let tags: [(&[u8; 4], Vec<u8>); 9] = [
        (b"desc", description_tag("sRGB")),
        (b"cprt", text_tag("No copyright, use freely")),
        (b"wtpt", xyz_tag(0.950_455, 1.0, 1.089_06)),
        (b"rXYZ", xyz_tag(0.436_074_7, 0.222_504_5, 0.013_932_2)),
        (b"gXYZ", xyz_tag(0.385_064_9, 0.716_878_6, 0.097_104_5)),
        (b"bXYZ", xyz_tag(0.143_080_4, 0.060_616_9, 0.714_173_3)),
        (b"rTRC", curve.clone()),
        (b"gTRC", curve.clone()),
        (b"bTRC", curve),
    ];
    // The three curves are the same data: written once, pointed at three times.
    let mut data: Vec<u8> = Vec::new();
    let table_len = 4 + 12 * tags.len();
    let mut entries = Vec::with_capacity(tags.len());
    let mut curve_at: Option<(u32, u32)> = None;
    for (sig, body) in &tags {
        let is_curve = sig.ends_with(b"TRC");
        if let (true, Some(at)) = (is_curve, curve_at) {
            entries.push((*sig, at.0, at.1));
            continue;
        }
        let offset = (128 + table_len + data.len()) as u32;
        let size = body.len() as u32;
        data.extend(body);
        // Tag data starts on four-byte boundaries.
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
        if is_curve {
            curve_at = Some((offset, size));
        }
        entries.push((*sig, offset, size));
    }
    let size = (128 + table_len + data.len()) as u32;

    let mut p = Vec::with_capacity(size as usize);
    p.extend(size.to_be_bytes());
    p.extend([0u8; 4]); // preferred CMM: none
    p.extend([0x02, 0x10, 0, 0]); // version 2.1
    p.extend(b"mntr"); // display device
    p.extend(b"RGB ");
    p.extend(b"XYZ ");
    // Creation date (fixed, so the profile is the same bytes every time).
    for v in [2026u16, 10, 1, 0, 0, 0] {
        p.extend(v.to_be_bytes());
    }
    p.extend(b"acsp");
    p.extend([0u8; 4 + 4 + 4 + 4 + 8]); // platform, flags, manufacturer, model, attributes
    p.extend([0u8; 4]); // perceptual intent
    // The profile connection space's illuminant, D50.
    for v in [0.964_2, 1.0, 0.824_9] {
        p.extend(fixed(v));
    }
    p.extend([0u8; 4 + 16 + 28]); // creator, profile ID (none in version 2), reserved
    debug_assert_eq!(p.len(), 128);
    p.extend((entries.len() as u32).to_be_bytes());
    for (sig, offset, len) in entries {
        p.extend(sig);
        p.extend(offset.to_be_bytes());
        p.extend(len.to_be_bytes());
    }
    p.extend(data);
    debug_assert_eq!(p.len(), size as usize);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(p: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(p[at..at + 4].try_into().unwrap())
    }

    /// The tag `sig`'s data.
    fn tag<'a>(p: &'a [u8], sig: &[u8; 4]) -> Option<&'a [u8]> {
        let count = u32_at(p, 128) as usize;
        (0..count).find_map(|i| {
            let e = 132 + i * 12;
            (&p[e..e + 4] == sig).then(|| {
                let (offset, len) = (u32_at(p, e + 4) as usize, u32_at(p, e + 8) as usize);
                &p[offset..offset + len]
            })
        })
    }

    #[test]
    fn is_a_well_formed_srgb_display_profile() {
        let p = srgb_profile();
        assert_eq!(u32_at(&p, 0) as usize, p.len(), "size in the header");
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(
            (&p[12..16], &p[16..20], &p[20..24]),
            (&b"mntr"[..], &b"RGB "[..], &b"XYZ "[..])
        );
        // Every required tag of an RGB display profile, inside the profile.
        for sig in [
            b"desc", b"cprt", b"wtpt", b"rXYZ", b"gXYZ", b"bXYZ", b"rTRC", b"gTRC", b"bTRC",
        ] {
            let t =
                tag(&p, sig).unwrap_or_else(|| panic!("{} missing", String::from_utf8_lossy(sig)));
            assert!(!t.is_empty());
        }
        // The colorants add up to D50, the connection space's white.
        let sum = |c: usize| -> f64 {
            [b"rXYZ", b"gXYZ", b"bXYZ"]
                .iter()
                .map(|s| {
                    f64::from(i32::from_be_bytes(
                        tag(&p, s).unwrap()[8 + c * 4..12 + c * 4]
                            .try_into()
                            .unwrap(),
                    )) / 65536.0
                })
                .sum()
        };
        assert!(
            (sum(0) - 0.9642).abs() < 0.001
                && (sum(1) - 1.0).abs() < 0.001
                && (sum(2) - 0.8249).abs() < 0.001
        );
        // The curve: sRGB 0.5 is about 21.4% linear.
        let curve = tag(&p, b"rTRC").unwrap();
        assert_eq!(&curve[..4], b"curv");
        let n = u32_at(curve, 8) as usize;
        let mid = u16::from_be_bytes(
            curve[12 + (n / 2) * 2..14 + (n / 2) * 2]
                .try_into()
                .unwrap(),
        );
        assert!((f64::from(mid) / 65535.0 - 0.2144).abs() < 0.002, "{mid}");
        // The same bytes every time.
        assert_eq!(srgb_profile(), p);
    }
}
