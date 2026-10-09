use super::*;

/// A `w` × `h` picture coloured by `colour` at fractions of it.
fn picture(w: u32, h: u32, colour: impl Fn(f32, f32) -> [u8; 3]) -> Vec<u8> {
    (0..w * h)
        .flat_map(|k| {
            let (x, y) = ((k % w) as f32 / w as f32, (k / w) as f32 / h as f32);
            let [r, g, b] = colour(x, y);
            [r, g, b, 255]
        })
        .collect()
}

/// A seaside: blue sky (lighter towards the horizon at 45 %) with a white cloud, a
/// darker blue sea to 60 %, then grass, its blades striped.
fn seaside(x: f32, y: f32) -> [u8; 3] {
    if (x - 0.3).hypot((y - 0.18) * 1.6) < 0.08 {
        [245, 245, 248]
    } else if y < 0.45 {
        let t = y / 0.45;
        [
            (110.0 + 80.0 * t) as u8,
            (150.0 + 60.0 * t) as u8,
            (215.0 + 20.0 * t) as u8,
        ]
    } else if y < 0.6 {
        [60, 85, 120]
    } else if ((x * 400.0) as u32).is_multiple_of(2) {
        [70, 120, 40]
    } else {
        [110, 150, 60]
    }
}

fn find(rgba: &[u8], w: u32, h: u32, scene_ev: Option<f32>) -> Option<Coverage> {
    SkyFinder
        .segment(
            Picture {
                width: w,
                height: h,
                rgba,
                scene_ev,
            },
            MaskKind::Sky,
        )
        .unwrap()
}

#[test]
fn finds_a_sky_with_its_clouds_and_stops_at_the_horizon() {
    let (w, h) = (900, 600);
    let rgba = picture(w, h, seaside);
    let sky = find(&rgba, w, h, Some(13.0)).expect("a sky");
    // The sky, near the top and just above the horizon, and the cloud.
    for (x, y) in [(0.5, 0.05), (0.8, 0.4), (0.3, 0.18)] {
        assert!(sky.at(x, y) > 0.9, "sky at {x}, {y}: {}", sky.at(x, y));
    }
    // Not the sea, as blue as it is, nor the grass.
    for (x, y) in [(0.5, 0.5), (0.2, 0.58), (0.5, 0.8)] {
        assert!(sky.at(x, y) < 0.1, "not sky at {x}, {y}: {}", sky.at(x, y));
    }
    // The edge at the horizon is sharp.
    assert!(sky.at(0.5, 0.44) > 0.8 && sky.at(0.5, 0.46) < 0.2);
    assert!((0.4..0.5).contains(&sky.share()), "{}", sky.share());
}

#[test]
fn without_the_exposure_it_goes_by_brightness() {
    let (w, h) = (600, 400);
    let rgba = picture(w, h, seaside);
    assert!(find(&rgba, w, h, None).is_some());
}

#[test]
fn a_bright_wall_indoors_is_not_sky() {
    // The seaside's colours, photographed in a room: far too dark a scene for sky.
    let (w, h) = (600, 400);
    let rgba = picture(w, h, seaside);
    assert!(find(&rgba, w, h, Some(5.5)).is_none());
}

#[test]
fn foliage_and_warm_walls_are_not_sky() {
    let (w, h) = (600, 400);
    let leaves = picture(w, h, |x, y| {
        if ((x * 300.0) as u32 + (y * 200.0) as u32).is_multiple_of(3) {
            [90, 140, 50]
        } else {
            [150, 190, 90]
        }
    });
    assert!(find(&leaves, w, h, Some(13.0)).is_none());
    let sandstone = picture(w, h, |_, _| [200, 160, 140]);
    assert!(find(&sandstone, w, h, Some(13.0)).is_none());
}

#[test]
fn finds_white_sky_between_twigs_and_not_the_twigs() {
    // An overcast sky over the top half, crossed by thin dark twigs every 24 px in its
    // lower part; a lawn below.
    let (w, h) = (960, 640);
    let rgba = picture(w, h, |x, y| {
        let px = (x * 960.0) as u32;
        if y < 0.5 && y > 0.2 && px % 24 < 3 {
            [70, 62, 58]
        } else if y < 0.5 {
            [240, 242, 245]
        } else {
            [80, 120, 50]
        }
    });
    let sky = find(&rgba, w, h, Some(11.0)).expect("a sky");
    // Between the twigs (a twig at 480..483 px), and the twigs themselves.
    assert!(
        sky.at(492.0 / 960.0, 0.35) > 0.8,
        "{}",
        sky.at(492.0 / 960.0, 0.35)
    );
    assert!(
        sky.at(481.5 / 960.0, 0.35) < 0.5,
        "{}",
        sky.at(481.5 / 960.0, 0.35)
    );
    assert!(sky.at(0.5, 0.75) < 0.05);
}

#[test]
fn waves_below_the_horizon_are_not_sky() {
    // A pale sky down to 40 %, then a sea of dark blue troughs and pale crests.
    let (w, h) = (900, 600);
    let rgba = picture(w, h, |x, y| {
        if y < 0.4 {
            [200, 215, 235]
        } else if ((x * 900.0) as u32 / 3 + (y * 600.0) as u32 / 2).is_multiple_of(4) {
            [190, 205, 225]
        } else {
            [50, 75, 115]
        }
    });
    let sky = find(&rgba, w, h, Some(13.0)).expect("a sky");
    assert!(sky.at(0.5, 0.2) > 0.9);
    for y in [0.45, 0.6, 0.9] {
        assert!(sky.at(0.5, y) < 0.1, "sea at {y}: {}", sky.at(0.5, y));
    }
}

#[test]
fn without_the_exposure_a_grey_sky_is_not_found() {
    // An overcast sky over grass: found with the camera's exposure, not without (it
    // could as well be a wall).
    let (w, h) = (600, 400);
    let rgba = picture(w, h, |x, y| {
        if y < 0.5 {
            [235, 236, 238]
        } else if (x * 600.0) as u32 % 4 < 2 {
            [70, 120, 40]
        } else {
            [110, 150, 60]
        }
    });
    assert!(find(&rgba, w, h, Some(12.0)).is_some());
    assert!(find(&rgba, w, h, None).is_none());
}

#[test]
fn a_grey_wall_in_daylight_is_not_sky() {
    // Bright enough outdoors, neutral and smooth, but not blue, and far from the
    // brightest thing in the photo.
    let (w, h) = (600, 400);
    let rgba = picture(w, h, |_, y| {
        if y < 0.6 {
            [140, 140, 142]
        } else {
            [250, 250, 250]
        }
    });
    assert!(find(&rgba, w, h, Some(12.0)).is_none());
}

#[test]
fn it_makes_only_sky_masks() {
    let rgba = picture(8, 8, |_, _| [128, 128, 128]);
    let p = Picture {
        width: 8,
        height: 8,
        rgba: &rgba,
        scene_ev: None,
    };
    assert_eq!(
        SkyFinder.segment(p, MaskKind::Subject),
        Err(AiError::Unsupported(MaskKind::Subject))
    );
    assert!(!SkyFinder.supports(MaskKind::People));
}
