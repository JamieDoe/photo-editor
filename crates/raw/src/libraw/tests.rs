use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use image_core::NeverCancel;

use super::*;

fn synthetic_dng(dir: &Path, w: u32, h: u32) -> PathBuf {
    let path = dir.join("chart.dng");
    std::fs::write(&path, fixtures::chart_dng(w, h)).unwrap();
    path
}

#[test]
fn reports_libraw_version() {
    assert!(LibRawDecoder::libraw_version().starts_with("0."));
}

#[test]
fn decodes_synthetic_dng_full() {
    let dir = fixtures::TempDir::new("libraw-full");
    let path = synthetic_dng(dir.path(), 640, 400);
    let opts = DecodeOptions::new(DecodeScale::Full);
    let out = LibRawDecoder.decode(&path, opts, &NeverCancel).unwrap();
    assert_eq!((out.image.width(), out.image.height()), (640, 400));
    assert_eq!(out.info.kind, SourceKind::CameraRaw);
    assert_eq!(out.info.make, fixtures::DNG_MAKE);
}

#[test]
fn decodes_synthetic_dng_half_size_for_previews() {
    let dir = fixtures::TempDir::new("libraw-half");
    let path = synthetic_dng(dir.path(), 640, 400);
    let opts = DecodeOptions::new(DecodeScale::AtLeast(300));
    let out = LibRawDecoder.decode(&path, opts, &NeverCancel).unwrap();
    assert_eq!((out.image.width(), out.image.height()), (320, 200));
    assert_eq!((out.info.full_width, out.info.full_height), (640, 400));

    // A minimum above half size forces a full decode.
    let opts = DecodeOptions::new(DecodeScale::AtLeast(400));
    let out = LibRawDecoder.decode(&path, opts, &NeverCancel).unwrap();
    assert_eq!(out.image.width(), 640);
}

#[test]
fn synthetic_dng_colours_survive_decode() {
    // The chart's white patch is scene-linear 0.9 and the grey ramp is neutral:
    // camera WB + colour matrix must bring them back to (approximately) neutral.
    let dir = fixtures::TempDir::new("libraw-colour");
    let path = synthetic_dng(dir.path(), 640, 400);
    let opts = DecodeOptions::new(DecodeScale::Full);
    let out = LibRawDecoder.decode(&path, opts, &NeverCancel).unwrap();
    let (x, y) = fixtures::chart_probe_neutral(640, 400);
    let i = (y as usize * 640 + x as usize) * 3;
    let px: Vec<f32> = out.image.data()[i..i + 3]
        .iter()
        .map(|&v| f32::from(v) / 65535.0)
        .collect();
    let expected = fixtures::chart_neutral_value();
    for c in &px {
        assert!(
            (c - expected).abs() < 0.03,
            "pixel {px:?} expected ~{expected}"
        );
    }
}

#[test]
fn cancellation_aborts_decode() {
    let dir = fixtures::TempDir::new("libraw-cancel");
    let path = synthetic_dng(dir.path(), 640, 400);
    let cancelled = AtomicBool::new(true);
    let opts = DecodeOptions::new(DecodeScale::Full);
    let err = LibRawDecoder.decode(&path, opts, &cancelled).unwrap_err();
    assert!(matches!(err, DecodeError::Cancelled), "{err}");
}

#[test]
fn garbage_file_is_unsupported_or_corrupt() {
    let dir = fixtures::TempDir::new("libraw-garbage");
    let path = dir.path().join("bad.nef");
    std::fs::write(&path, vec![0x42u8; 4096]).unwrap();
    let opts = DecodeOptions::new(DecodeScale::Full);
    let err = LibRawDecoder.decode(&path, opts, &NeverCancel).unwrap_err();
    assert!(
        matches!(err, DecodeError::Unsupported(_) | DecodeError::Corrupt(_)),
        "{err}"
    );
}

#[test]
fn missing_file_is_not_found() {
    let opts = DecodeOptions::new(DecodeScale::Full);
    let err = LibRawDecoder.decode(Path::new("/nonexistent/x.nef"), opts, &NeverCancel);
    assert!(matches!(err, Err(DecodeError::NotFound(_))));
}

#[test]
fn synthetic_dng_without_embedded_preview_returns_none() {
    let dir = fixtures::TempDir::new("libraw-nothumb");
    let path = synthetic_dng(dir.path(), 640, 400);
    assert!(
        LibRawDecoder
            .embedded_preview(&path, 512, &NeverCancel)
            .unwrap()
            .is_none()
    );
}

/// Real camera files (git-ignored local fixtures): every sample has an embedded JPEG
/// preview, oriented and sized like the decoded image. Skips when fixtures are absent.
#[test]
fn camera_files_yield_oriented_embedded_previews() {
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/local");
    let Ok(entries) = std::fs::read_dir(&local) else {
        return;
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        let is_raw = LibRawDecoder.handles(&path) && !path.to_string_lossy().contains("synthetic");
        if !is_raw {
            continue;
        }
        let preview = LibRawDecoder
            .embedded_preview(&path, 1024, &NeverCancel)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .unwrap_or_else(|| panic!("{}: no embedded preview", path.display()));
        let decoded = LibRawDecoder
            .decode(
                &path,
                DecodeOptions::new(DecodeScale::AtLeast(1024)),
                &NeverCancel,
            )
            .unwrap();
        let (pw, ph) = (preview.image.width() as f32, preview.image.height() as f32);
        let (dw, dh) = (decoded.image.width() as f32, decoded.image.height() as f32);
        // Same orientation (landscape vs portrait) and roughly the same aspect ratio.
        assert_eq!(
            pw >= ph,
            dw >= dh,
            "{}: orientation differs",
            path.display()
        );
        assert!(
            ((pw / ph) - (dw / dh)).abs() < 0.1,
            "{}: aspect {pw}x{ph} vs {dw}x{dh}",
            path.display()
        );
        assert!(
            pw.max(ph) >= 512.0,
            "{}: preview too small ({pw}x{ph})",
            path.display()
        );
    }
}

#[test]
fn camera_files_report_metadata() {
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/local");
    let Ok(entries) = std::fs::read_dir(&local) else {
        return;
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        if !LibRawDecoder.handles(&path) || path.to_string_lossy().contains("synthetic") {
            continue;
        }
        let m = LibRawDecoder
            .read_metadata(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.display();
        assert!(
            m.camera_make.is_some() && m.camera_model.is_some(),
            "{name}: {m:?}"
        );
        assert!(
            m.width.unwrap_or(0) > 1000 && m.height.unwrap_or(0) > 1000,
            "{name}: {m:?}"
        );
        let date = m
            .captured_at
            .as_deref()
            .unwrap_or_else(|| panic!("{name}: no capture time"));
        assert_eq!(date.len(), 19, "{name}: {date}");
        assert!(
            m.iso.is_some() && m.aperture.is_some() && m.shutter_seconds.is_some(),
            "{name}: {m:?}"
        );
    }
}

#[test]
fn synthetic_dng_metadata_has_camera_and_size() {
    let dir = fixtures::TempDir::new("libraw-meta");
    let path = synthetic_dng(dir.path(), 640, 400);
    let m = LibRawDecoder.read_metadata(&path).unwrap();
    assert_eq!(m.camera_make.as_deref(), Some(fixtures::DNG_MAKE));
    assert_eq!((m.width, m.height), (Some(640), Some(400)));
    assert_eq!(m.captured_at, None);
}

#[test]
fn reports_the_as_shot_light() {
    // The fixture's AsShotNeutral is the camera's response to a D65 white, so the
    // as-shot light must come back as D65.
    let dir = fixtures::TempDir::new("libraw-as-shot");
    let path = synthetic_dng(dir.path(), 64, 40);
    for scale in [DecodeScale::Full, DecodeScale::AtLeast(20)] {
        let out = LibRawDecoder
            .decode(&path, DecodeOptions::new(scale), &NeverCancel)
            .unwrap();
        let c = out.info.as_shot_white.expect("as-shot light");
        let d65 = image_core::Chromaticity::D65;
        assert!(
            (c.x - d65.x).abs() < 1e-3 && (c.y - d65.y).abs() < 1e-3,
            "{c:?}"
        );
    }
}

// --- X-Trans determinism (ADR 0061) ---
//
// LibRaw's OpenMP code races on X-Trans sensors (half-size binning and the strip-parallel
// demosaic), so the same file used to decode to different pixels each time. The shim
// must give identical bytes on every decode and for any thread count.

fn decode_samples(path: &Path, scale: DecodeScale, threads: Option<usize>) -> Vec<u16> {
    let mut opts = DecodeOptions::new(scale);
    if let Some(n) = threads {
        opts = opts.with_max_threads(n);
    }
    let out = LibRawDecoder.decode(path, opts, &NeverCancel).unwrap();
    out.image.data().to_vec()
}

fn assert_identical(a: &[u16], b: &[u16], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: sizes differ");
    let differing = a.iter().zip(b).filter(|(x, y)| x != y).count();
    assert_eq!(
        differing,
        0,
        "{what}: {differing} of {} samples differ",
        a.len()
    );
}

/// 1700 x 988 makes the shim's block grid cover all its cases: a remainder merged into
/// the last column of blocks and a last row of blocks pulled back to one LibRaw tile.
fn synthetic_xtrans(dir: &Path) -> PathBuf {
    let path = dir.join("xtrans.dng");
    std::fs::write(&path, fixtures::chart_xtrans_dng(1700, 988)).unwrap();
    path
}

#[test]
fn synthetic_xtrans_decodes_are_deterministic() {
    let dir = fixtures::TempDir::new("libraw-xtrans-determinism");
    let path = synthetic_xtrans(dir.path());
    for scale in [DecodeScale::AtLeast(400), DecodeScale::Full] {
        let reference = decode_samples(&path, scale, None);
        for threads in [None, None, Some(1), Some(3)] {
            let again = decode_samples(&path, scale, threads);
            assert_identical(
                &reference,
                &again,
                &format!("{scale:?}, {threads:?} threads"),
            );
        }
    }
}

#[test]
fn synthetic_xtrans_full_decode_matches_the_chart() {
    // The block-parallel demosaic must stitch blocks back in the right place: compare
    // every 24 x 24 region against the scene (the texture averages out).
    let dir = fixtures::TempDir::new("libraw-xtrans-chart");
    let path = synthetic_xtrans(dir.path());
    let (w, h) = (1700u32, 988u32);
    let out = LibRawDecoder
        .decode(&path, DecodeOptions::new(DecodeScale::Full), &NeverCancel)
        .unwrap();
    assert_eq!((out.image.width(), out.image.height()), (w, h));
    let data = out.image.data();
    let mut worst = (0.0f32, 0u32, 0u32);
    for ry in (8..h - 8 - 24).step_by(24) {
        for rx in (8..w - 8 - 24).step_by(24) {
            let mut err = 0.0f32;
            for y in ry..ry + 24 {
                for x in rx..rx + 24 {
                    let i = (y as usize * w as usize + x as usize) * 3;
                    let scene = fixtures::sample(x, y, w, h);
                    for c in 0..3 {
                        err += (f32::from(data[i + c]) / 65535.0 - scene[c]).abs();
                    }
                }
            }
            let mean = err / (24.0 * 24.0 * 3.0);
            if mean > worst.0 {
                worst = (mean, rx, ry);
            }
        }
    }
    assert!(
        worst.0 < 0.03,
        "region at {:?} has mean error {}",
        (worst.1, worst.2),
        worst.0
    );
}

/// The real camera file that exposed the races (git-ignored local fixture; skips when
/// absent): 3.4% of preview pixels used to differ between two decodes.
#[test]
fn fujifilm_xtrans_decodes_are_deterministic() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/local/fujifilm-xt3-compressed.raf");
    if !path.exists() {
        return;
    }
    for scale in [DecodeScale::AtLeast(1500), DecodeScale::Full] {
        let first = decode_samples(&path, scale, None);
        let second = decode_samples(&path, scale, None);
        assert_identical(&first, &second, &format!("{scale:?}"));
    }
}
