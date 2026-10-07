//! Compact ICC profiles (version 2.1, matrix/TRC display profiles) for the export
//! colour spaces, generated here rather than shipped as files (ADRs 0057, 0061), so
//! their terms are ours. Embedded in exports so colour-managed applications
//! (Photoshop, print RIPs, browsers) read the pixels in the right space instead of
//! guessing.
//!
//! A space is its primaries' and white's chromaticities and its tone curve. The
//! colorants are worked out from those and adapted to the profile connection space's
//! D50 white with the Bradford transform; the media white is the space's own (D65),
//! as version 2 display profiles record it.

/// An RGB colour space as a profile describes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Space {
    pub name: &'static str,
    /// x, y chromaticities of red, green and blue.
    pub primaries: [[f64; 2]; 3],
    /// x, y of the white.
    pub white: [f64; 2],
    pub curve: Curve,
}

/// A space's tone curve (encoded value to linear light).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// The sRGB curve (also Display P3's).
    Srgb,
    /// A pure power, as Adobe RGB's 563/256.
    Gamma(f64),
}

const D65: [f64; 2] = [0.3127, 0.3290];

pub const SRGB: Space = Space {
    name: "sRGB",
    primaries: [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]],
    white: D65,
    curve: Curve::Srgb,
};

pub const DISPLAY_P3: Space = Space {
    name: "Display P3",
    primaries: [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
    white: D65,
    curve: Curve::Srgb,
};

pub const ADOBE_RGB: Space = Space {
    name: "Adobe RGB (1998) compatible",
    primaries: [[0.64, 0.33], [0.21, 0.71], [0.15, 0.06]],
    white: D65,
    curve: Curve::Gamma(563.0 / 256.0),
};

fn xyz_of([x, y]: [f64; 2]) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

fn mul3(a: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| a[r][0] * v[0] + a[r][1] * v[1] + a[r][2] * v[2])
}

fn mat_mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

pub(crate) fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let co = [e * i - f * h, f * g - d * i, d * h - e * g];
    let det = a * co[0] + b * co[1] + c * co[2];
    [
        [co[0] / det, (c * h - b * i) / det, (b * f - c * e) / det],
        [co[1] / det, (a * i - c * g) / det, (c * d - a * f) / det],
        [co[2] / det, (b * g - a * h) / det, (a * e - b * d) / det],
    ]
}

/// The space's linear RGB to XYZ (its own white at Y = 1): columns are the primaries,
/// scaled so that they add up to the white.
pub(crate) fn rgb_to_xyz(space: &Space) -> [[f64; 3]; 3] {
    let p = space.primaries.map(xyz_of);
    let m = [0, 1, 2].map(|r| [p[0][r], p[1][r], p[2][r]]);
    let s = mul3(&invert(&m), xyz_of(space.white));
    [0, 1, 2].map(|r| [m[r][0] * s[0], m[r][1] * s[1], m[r][2] * s[2]])
}

/// The colorants (XYZ of red, green and blue) adapted to D50 with Bradford.
fn colorants(space: &Space) -> [[f64; 3]; 3] {
    const BRADFORD: [[f64; 3]; 3] = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    // The D50 white the published colorants are adapted to.
    let d50 = [0.964_22, 1.0, 0.825_21];
    let (src, dst) = (mul3(&BRADFORD, xyz_of(space.white)), mul3(&BRADFORD, d50));
    let scale = [
        [dst[0] / src[0], 0.0, 0.0],
        [0.0, dst[1] / src[1], 0.0],
        [0.0, 0.0, dst[2] / src[2]],
    ];
    let adapt = mat_mul(&invert(&BRADFORD), &mat_mul(&scale, &BRADFORD));
    let m = mat_mul(&adapt, &rgb_to_xyz(space));
    [0, 1, 2].map(|c| [m[0][c], m[1][c], m[2][c]])
}

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

/// The space's curve: the sRGB curve as a 1024-point table, or a gamma as one number.
fn curve_tag(curve: Curve) -> Vec<u8> {
    const POINTS: u32 = 1024;
    let mut t = b"curv\0\0\0\0".to_vec();
    if let Curve::Gamma(g) = curve {
        // One entry: the gamma as u8Fixed8 (563/256 exactly for Adobe RGB).
        t.extend(1u32.to_be_bytes());
        t.extend(((g * 256.0).round() as u16).to_be_bytes());
        return t;
    }
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
#[cfg(test)]
pub fn srgb_profile() -> Vec<u8> {
    profile(&SRGB)
}

/// `space`'s profile.
pub fn profile(space: &Space) -> Vec<u8> {
    let curve = curve_tag(space.curve);
    let [r, g, b] = colorants(space);
    let w = xyz_of(space.white);
    let tags: [(&[u8; 4], Vec<u8>); 9] = [
        (b"desc", description_tag(space.name)),
        (b"cprt", text_tag("No copyright, use freely")),
        (b"wtpt", xyz_tag(w[0], w[1], w[2])),
        (b"rXYZ", xyz_tag(r[0], r[1], r[2])),
        (b"gXYZ", xyz_tag(g[0], g[1], g[2])),
        (b"bXYZ", xyz_tag(b[0], b[1], b[2])),
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

    #[test]
    fn colorants_are_worked_out_as_published() {
        // sRGB's Bradford-adapted colorants, as every sRGB profile has them.
        let c = colorants(&SRGB);
        let published = [
            [0.436_074_7, 0.222_504_5, 0.013_932_2],
            [0.385_064_9, 0.716_878_6, 0.097_104_5],
            [0.143_080_4, 0.060_616_9, 0.714_173_3],
        ];
        for (got, want) in c.iter().zip(published) {
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 2e-4, "{c:?}");
            }
        }
        // Display P3's, likewise (as Apple's profile has them).
        let p3 = colorants(&DISPLAY_P3);
        assert!(
            (p3[0][0] - 0.5151).abs() < 2e-4 && (p3[1][1] - 0.6922).abs() < 2e-4,
            "{p3:?}"
        );
    }

    #[test]
    fn every_space_makes_a_well_formed_profile() {
        for space in [SRGB, DISPLAY_P3, ADOBE_RGB] {
            let p = profile(&space);
            assert_eq!(u32_at(&p, 0) as usize, p.len(), "{}", space.name);
            assert_eq!(&p[36..40], b"acsp");
            // Colorants add up to D50 whatever the space.
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
                (sum(0) - 0.9642).abs() < 0.001 && (sum(2) - 0.8249).abs() < 0.001,
                "{}",
                space.name
            );
        }
        // Adobe RGB's curve: one gamma, 563/256 exactly.
        let p = profile(&ADOBE_RGB);
        let curve = tag(&p, b"rTRC").unwrap();
        assert_eq!(
            (u32_at(curve, 8), u16::from_be_bytes([curve[12], curve[13]])),
            (1, 563)
        );
        // Different spaces, different profiles.
        assert_ne!(profile(&DISPLAY_P3), profile(&SRGB));
    }
}
