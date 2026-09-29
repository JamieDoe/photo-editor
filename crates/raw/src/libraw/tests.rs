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
