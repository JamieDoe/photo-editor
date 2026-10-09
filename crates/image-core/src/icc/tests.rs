use super::*;

/// An ICC profile of colour space `space` holding `tags`, laid out as the format asks.
fn profile(space: &[u8; 4], tags: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut header = vec![0u8; 128];
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(space);
    header[20..24].copy_from_slice(b"XYZ ");
    header[36..40].copy_from_slice(b"acsp");
    let table_len = 4 + 12 * tags.len();
    let mut data = Vec::new();
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    for (sig, body) in tags {
        let offset = 128 + table_len + data.len();
        table.extend_from_slice(*sig);
        table.extend((offset as u32).to_be_bytes());
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

fn s15f16(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz(v: [f64; 3]) -> Vec<u8> {
    [
        b"XYZ ".to_vec(),
        vec![0; 4],
        v.iter().flat_map(|&x| s15f16(x)).collect(),
    ]
    .concat()
}

fn gamma(g: f64) -> Vec<u8> {
    [
        b"curv".to_vec(),
        vec![0; 4],
        1u32.to_be_bytes().to_vec(),
        ((g * 256.0).round() as u16).to_be_bytes().to_vec(),
    ]
    .concat()
}

fn para(kind: u16, p: &[f64]) -> Vec<u8> {
    [
        b"para".to_vec(),
        vec![0; 4],
        kind.to_be_bytes().to_vec(),
        vec![0; 2],
        p.iter().flat_map(|&x| s15f16(x)).collect(),
    ]
    .concat()
}

fn desc(text: &str) -> Vec<u8> {
    [
        b"desc".to_vec(),
        vec![0; 4],
        ((text.len() + 1) as u32).to_be_bytes().to_vec(),
        text.as_bytes().to_vec(),
        vec![0],
    ]
    .concat()
}

/// sRGB's and Adobe RGB's colorants, Bradford-adapted to D50.
const SRGB_D50: [[f64; 3]; 3] = [
    [0.436_074_7, 0.222_504_5, 0.013_932_2],
    [0.385_064_9, 0.716_878_6, 0.097_104_5],
    [0.143_080_4, 0.060_616_9, 0.714_173_3],
];
const ADOBE_D50: [[f64; 3]; 3] = [
    [0.609_755_9, 0.311_124_2, 0.019_481_1],
    [0.205_240_1, 0.625_656_0, 0.060_890_2],
    [0.149_224_0, 0.063_219_7, 0.744_838_7],
];

fn rgb_profile(colorants: [[f64; 3]; 3], curve: Vec<u8>, name: &str) -> Vec<u8> {
    profile(
        b"RGB ",
        &[
            (b"desc", desc(name)),
            (b"rXYZ", xyz(colorants[0])),
            (b"gXYZ", xyz(colorants[1])),
            (b"bXYZ", xyz(colorants[2])),
            (b"rTRC", curve.clone()),
            (b"gTRC", curve.clone()),
            (b"bTRC", curve),
        ],
    )
}

/// The sRGB curve as a type 3 parametric curve.
fn srgb_curve() -> Vec<u8> {
    para(3, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.040_45])
}

#[test]
fn an_srgb_profile_is_recognised_as_srgb() {
    let p = parse(&rgb_profile(SRGB_D50, srgb_curve(), "sRGB IEC61966-2.1")).unwrap();
    assert_eq!(p.kind, Kind::Rgb);
    assert_eq!(p.description.as_deref(), Some("sRGB IEC61966-2.1"));
    assert!(p.is_srgb());
    for v in [0.02, 0.2, 0.5, 0.9] {
        assert!(
            (p.linearise(1, v) - crate::color::srgb_to_linear(v)).abs() < 1e-4,
            "{v}"
        );
    }
}

#[test]
fn adobe_rgb_colours_become_their_srgb_equivalents() {
    let p = parse(&rgb_profile(
        ADOBE_D50,
        gamma(563.0 / 256.0),
        "Adobe RGB (1998)",
    ))
    .unwrap();
    assert!(!p.is_srgb());
    // Its curve: a 2.2 power.
    assert!((p.linearise(0, 0.5) - 0.5f32.powf(2.199_219)).abs() < 1e-4);
    // A colour inside both gamuts: Adobe RGB to sRGB is [1.3982, -0.3982, 0; 0, 1, 0;
    // 0, -0.0429, 1.0429].
    let s = p.to_srgb([0.2, 0.3, 0.4]);
    let expected = [
        1.398_2 * 0.2 - 0.398_2 * 0.3,
        0.3,
        -0.042_9 * 0.3 + 1.042_9 * 0.4,
    ];
    for (a, b) in s.iter().zip(expected) {
        assert!((a - b).abs() < 0.003, "{s:?} against {expected:?}");
    }
    // Greys and white stay grey.
    for g in [0.0, 0.18, 1.0] {
        let s = p.to_srgb([g; 3]);
        assert!(s.iter().all(|c| (c - g).abs() < 0.002), "{s:?}");
    }
    // Its green, beyond sRGB, comes in green: red and blue small, not far negative.
    let s = p.to_srgb([0.0, 1.0, 0.0]);
    assert!(s[1] > 0.9 && s[0] > -0.1 && s[2] > -0.1, "{s:?}");
}

#[test]
fn grey_profiles_have_one_curve() {
    let p = parse(&profile(b"GRAY", &[(b"kTRC", gamma(1.8))])).unwrap();
    assert_eq!(p.kind, Kind::Grey);
    // 1.8 as the format stores it: 461/256.
    assert!((p.linearise(2, 0.5) - 0.5f32.powf(461.0 / 256.0)).abs() < 1e-5);
    assert_eq!(p.to_srgb([0.3, 0.3, 0.3]), [0.3, 0.3, 0.3]);
}

#[test]
fn every_curve_type_is_read() {
    // A table: linear between its points.
    let table = [
        b"curv".to_vec(),
        vec![0; 4],
        3u32.to_be_bytes().to_vec(),
        [0u16, 16384, 65535]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect(),
    ]
    .concat();
    let c = read_curve(&table).unwrap();
    assert!((c.eval(0.25) - 0.125).abs() < 1e-3 && (c.eval(1.0) - 1.0).abs() < 1e-6);
    // Identity, and each parametric type.
    assert_eq!(
        read_curve(&[b"curv".to_vec(), vec![0; 8]].concat())
            .unwrap()
            .eval(0.3),
        0.3
    );
    assert!((read_curve(&para(0, &[2.0])).unwrap().eval(0.5) - 0.25).abs() < 1e-5);
    assert!((read_curve(&para(1, &[1.0, 2.0, -0.5])).unwrap().eval(0.5) - 0.5).abs() < 1e-5);
    assert!(
        (read_curve(&para(2, &[1.0, 1.0, 0.0, 0.1]))
            .unwrap()
            .eval(0.5)
            - 0.6)
            .abs()
            < 1e-5
    );
    assert!(
        (read_curve(&para(4, &[1.0, 1.0, 0.0, 0.5, 0.2, 0.0, 0.05]))
            .unwrap()
            .eval(0.1)
            - 0.1)
            .abs()
            < 1e-5
    );
    // A zero `a` is no curve.
    assert!(read_curve(&para(1, &[1.0, 0.0, 0.0])).is_none());
}

#[test]
fn other_profiles_are_refused_with_a_reason() {
    assert_eq!(
        parse(b"not a profile"),
        Err(IccError::Malformed("no profile header"))
    );
    assert!(matches!(
        parse(&profile(b"CMYK", &[])),
        Err(IccError::Unsupported(_))
    ));
    // Lookup tables only: no colorants or curves to read.
    assert!(matches!(
        parse(&profile(b"RGB ", &[(b"A2B0", vec![0; 32])])),
        Err(IccError::Unsupported(_))
    ));
    // A tag pointing beyond the data.
    let mut damaged = rgb_profile(SRGB_D50, srgb_curve(), "x");
    damaged.truncate(200);
    assert!(parse(&damaged).is_err());
}
