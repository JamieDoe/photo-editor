use super::*;

const SKIN: [f32; 3] = [0.35, 0.2, 0.15];
const WHITE: [f32; 3] = [0.8, 0.8, 0.8];
const RED_PUPIL: [f32; 3] = [0.6, 0.03, 0.03];
const LIPS: [f32; 3] = [0.5, 0.04, 0.05];

/// A 160 × 100 face: skin, an eye at (80, 40) (white, a red pupil of radius 6 and a
/// white catchlight at (82, 38)), and red lips at (40, 80).
fn face() -> LinearImage {
    let (w, h) = (160usize, 100usize);
    let data = (0..w * h)
        .flat_map(|k| {
            let (x, y) = ((k % w) as f32 + 0.5, (k / w) as f32 + 0.5);
            let eye = (x - 80.0).hypot(y - 40.0);
            let c = if (x - 82.0).abs() < 1.0 && (y - 38.0).abs() < 1.0 {
                [1.0; 3]
            } else if eye < 6.0 {
                RED_PUPIL
            } else if eye < 14.0 {
                WHITE
            } else if (x - 40.0).hypot((y - 80.0) * 2.0) < 10.0 {
                LIPS
            } else {
                SKIN
            };
            c.map(|v| (v * 65535.0) as u16)
        })
        .collect();
    LinearImage::new(w as u32, h as u32, data).unwrap()
}

fn at(img: &LinearImage, x: usize, y: usize) -> [f32; 3] {
    let i = (y * img.width() as usize + x) * 3;
    [0, 1, 2].map(|c| f32::from(img.data()[i + c]) / 65535.0)
}

#[test]
fn finds_the_red_pupil_near_a_click() {
    let img = face();
    // A click a little off the pupil, searching a few pupils' width.
    let eye = find(&img, [83.0 / 160.0, 38.0 / 100.0], 0.08).expect("an eye");
    assert!(
        (eye.x * 160.0 - 80.0).abs() < 1.0 && (eye.y * 100.0 - 40.0).abs() < 1.0,
        "{eye:?}"
    );
    // About 1.5 pupils across, in fractions of the long edge.
    let r = eye.radius * 160.0;
    assert!((8.0..12.0).contains(&r), "{r}");
    // Nothing red near the cheek.
    assert!(find(&img, [130.0 / 160.0, 50.0 / 100.0], 0.05).is_none());
}

#[test]
fn the_pupil_becomes_dark_and_neutral_and_nothing_else_changes() {
    let img = face();
    let eye = find(&img, [80.0 / 160.0, 40.0 / 100.0], 0.08).unwrap();
    let fixed = apply(&img, &[eye]);
    // The pupil: no longer red, and darker.
    let p = at(&fixed, 78, 42);
    assert!(redness(p) < 0.1, "{p:?}");
    assert!(p[0] < 0.05, "{p:?}");
    // The catchlight, the white of the eye, the skin and the lips are as they were.
    for (x, y) in [(82, 38), (80, 29), (80, 70), (40, 80), (100, 40)] {
        let (a, b) = (at(&img, x, y), at(&fixed, x, y));
        assert!(
            a.iter().zip(b).all(|(u, v)| (u - v).abs() < 0.002),
            "({x}, {y}): {a:?} -> {b:?}"
        );
    }
}

#[test]
fn darken_and_pupil_size_do_what_they_say() {
    let img = face();
    let eye = find(&img, [80.0 / 160.0, 40.0 / 100.0], 0.08).unwrap();
    let darker = apply(
        &img,
        &[RedEye {
            darken: 100.0,
            ..eye
        }],
    );
    let lighter = apply(&img, &[RedEye { darken: 0.0, ..eye }]);
    assert!(at(&darker, 80, 40)[1] < at(&lighter, 80, 40)[1]);
    // A pale red (redness 0.5) counts as pupil only at a large pupil size.
    let pale =
        LinearImage::new(1, 1, [0.4, 0.2, 0.2].map(|v| (v * 65535.0) as u16).to_vec()).unwrap();
    let small = apply(
        &pale,
        &[RedEye {
            x: 0.5,
            y: 0.5,
            radius: 0.5,
            pupil: 0.0,
            darken: 50.0,
        }],
    );
    let large = apply(
        &pale,
        &[RedEye {
            x: 0.5,
            y: 0.5,
            radius: 0.5,
            pupil: 100.0,
            darken: 50.0,
        }],
    );
    assert!(redness(at(&small, 0, 0)) > 0.45);
    assert!(redness(at(&large, 0, 0)) < 0.3);
}

#[test]
fn corrections_are_sanitised() {
    let e = RedEye {
        x: f32::NAN,
        y: 2.0,
        radius: 9.0,
        pupil: -5.0,
        darken: 300.0,
    }
    .sanitized();
    assert_eq!(
        (e.x, e.y, e.radius, e.pupil, e.darken),
        (0.5, 1.0, MAX_RADIUS, 0.0, 100.0)
    );
}
