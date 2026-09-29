//! Library thumbnail integration tests: real files, real disk cache, engine jobs.

use std::path::{Path, PathBuf};

use app_core::{
    Engine, EngineConfig, ErrorKind, JobError, THUMBNAIL_LONG_EDGE, Thumbnail, ThumbnailSource,
};

struct Setup {
    dir: fixtures::TempDir,
    engine: Engine,
}

fn setup(label: &str, cache: bool) -> Setup {
    let dir = fixtures::TempDir::new(label);
    let config = EngineConfig {
        thumbnail_cache_dir: cache.then(|| dir.path().join("thumbs")),
        ..EngineConfig::default()
    };
    Setup {
        engine: Engine::new(config),
        dir,
    }
}

impl Setup {
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path.canonicalize().unwrap()
    }

    fn thumb(&self, path: &Path) -> Thumbnail {
        self.engine
            .thumbnail(path.to_path_buf())
            .wait()
            .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()))
    }
}

/// Width and height from a JPEG's start-of-frame marker.
fn jpeg_size(bytes: &[u8]) -> (u32, u32) {
    let mut i = 2;
    while i + 9 < bytes.len() {
        let (marker, len) = (
            bytes[i + 1],
            usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]])),
        );
        if matches!(marker, 0xC0..=0xC2) {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
            return (u32::from(w), u32::from(h));
        }
        i += 2 + len;
    }
    panic!("no SOF marker");
}

#[test]
fn jpeg_thumbnail_is_generated_once_then_served_from_the_cache() {
    let s = setup("thumb-jpeg", true);
    let path = s.file("large.jpg", &fixtures::chart_jpeg(2048, 1536, 90));
    let first = s.thumb(&path);
    assert_eq!(first.source, ThumbnailSource::Embedded);
    assert_eq!(jpeg_size(&first.jpeg), (THUMBNAIL_LONG_EDGE, 384));

    let second = s.thumb(&path);
    assert_eq!(second.source, ThumbnailSource::Cache);
    assert_eq!(second.jpeg, first.jpeg);
    let stats = s.engine.thumbnail_cache_stats().unwrap();
    assert_eq!((stats.entries, stats.bytes), (1, first.jpeg.len() as u64));
}

#[test]
fn changing_the_file_makes_a_new_thumbnail() {
    let s = setup("thumb-changed", true);
    let path = s.file("photo.jpg", &fixtures::chart_jpeg(1024, 768, 90));
    s.thumb(&path);
    std::fs::write(&path, fixtures::solid_jpeg(1200, 800, [200, 30, 30])).unwrap();
    let after = s.thumb(&path);
    assert_ne!(after.source, ThumbnailSource::Cache);
    assert_eq!(jpeg_size(&after.jpeg), (512, 341));
}

#[test]
fn a_damaged_cache_entry_is_regenerated() {
    let s = setup("thumb-damaged", true);
    let path = s.file("photo.jpg", &fixtures::chart_jpeg(1024, 768, 90));
    s.thumb(&path);
    // Simulate a write cut short by a power loss.
    let cache_dir = s.dir.path().join("thumbs");
    for shard in std::fs::read_dir(&cache_dir).unwrap().flatten() {
        for entry in std::fs::read_dir(shard.path()).unwrap().flatten() {
            std::fs::write(entry.path(), [0xFF, 0xD8, 0, 0, 0, 0]).unwrap();
        }
    }
    let again = s.thumb(&path);
    assert_eq!(again.source, ThumbnailSource::Embedded);
    assert_eq!(s.thumb(&path).source, ThumbnailSource::Cache);
}

#[test]
fn small_images_are_not_upscaled() {
    let s = setup("thumb-small", false);
    let path = s.file("small.jpg", &fixtures::chart_jpeg(300, 200, 90));
    let t = s.thumb(&path);
    // Too small to be a display preview at thumbnail size: decoded and rendered.
    assert_eq!(t.source, ThumbnailSource::Rendered);
    assert_eq!(jpeg_size(&t.jpeg), (300, 200));
}

#[test]
fn without_a_cache_directory_thumbnails_are_regenerated() {
    let s = setup("thumb-nocache", false);
    let path = s.file("photo.jpg", &fixtures::chart_jpeg(1024, 768, 90));
    assert_eq!(s.thumb(&path).source, ThumbnailSource::Embedded);
    assert_eq!(s.thumb(&path).source, ThumbnailSource::Embedded);
    assert!(s.engine.thumbnail_cache_stats().is_none());
}

#[test]
fn missing_files_report_not_found() {
    let s = setup("thumb-missing", true);
    let err = s
        .engine
        .thumbnail(s.dir.path().join("gone.jpg"))
        .wait()
        .unwrap_err();
    match err {
        JobError::Failed(e) => assert_eq!(e.kind, ErrorKind::NotFound),
        other => panic!("unexpected {other:?}"),
    }
}

/// A DNG without an embedded preview goes through decode and render.
#[cfg(feature = "libraw")]
#[test]
fn raw_without_embedded_preview_is_rendered() {
    let s = setup("thumb-dng", true);
    let path = s.file("chart.dng", &fixtures::chart_dng(1200, 800));
    let t = s.thumb(&path);
    assert_eq!(t.source, ThumbnailSource::Rendered);
    assert_eq!(jpeg_size(&t.jpeg), (512, 341));
    assert_eq!(s.thumb(&path).source, ThumbnailSource::Cache);
}

/// Real camera files (git-ignored local fixtures) use their embedded previews, oriented
/// like the photo. Skips when the fixtures are absent.
#[cfg(feature = "libraw")]
#[test]
fn camera_files_use_their_embedded_previews() {
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/local");
    let Ok(entries) = std::fs::read_dir(&local) else {
        return;
    };
    let s = setup("thumb-camera", false);
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        let name = path.to_string_lossy().to_lowercase();
        let is_camera_raw = ["nef", "cr3", "raf", "arw", "dng"]
            .iter()
            .any(|e| name.ends_with(e))
            && !name.contains("synthetic");
        if !is_camera_raw {
            continue;
        }
        let t = s.thumb(&path);
        assert_eq!(t.source, ThumbnailSource::Embedded, "{name}");
        let (w, h) = jpeg_size(&t.jpeg);
        assert_eq!(w.max(h), THUMBNAIL_LONG_EDGE, "{name}");
        let meta = raw::DecoderRegistry::with_defaults()
            .read_metadata(&path)
            .unwrap();
        if let (Some(mw), Some(mh)) = (meta.width, meta.height) {
            assert_eq!(w > h, mw > mh, "{name}: orientation matches the photo");
        }
    }
}

#[test]
fn pregeneration_fills_the_cache_and_skips_what_is_there() {
    let s = setup("thumb-pregen", true);
    let paths: Vec<PathBuf> = (0..6)
        .map(|i| {
            s.file(
                &format!("p{i}.jpg"),
                &fixtures::chart_jpeg(800 + i * 16, 600, 90),
            )
        })
        .collect();
    // One is already cached from browsing.
    s.thumb(&paths[0]);

    let summary = s
        .engine
        .pregenerate_thumbnails("root", paths.clone())
        .wait();
    assert_eq!(
        summary,
        app_core::BatchSummary {
            made: 5,
            already_cached: 1,
            failed: 0,
            cancelled: 0
        }
    );
    for p in &paths {
        assert_eq!(s.thumb(p).source, ThumbnailSource::Cache);
    }
    let again = s.engine.pregenerate_thumbnails("root", paths).wait();
    assert_eq!(again.already_cached, 6);
}

#[test]
fn a_new_batch_for_the_same_key_cancels_the_old_one() {
    let s = setup("thumb-pregen-cancel", true);
    let paths: Vec<PathBuf> = (0..40)
        .map(|i| {
            s.file(
                &format!("p{i}.jpg"),
                &fixtures::chart_jpeg(1600 + i, 1200, 90),
            )
        })
        .collect();
    let first = s.engine.pregenerate_thumbnails("root", paths.clone());
    let second = s.engine.pregenerate_thumbnails("root", paths[..2].to_vec());
    let first = first.wait();
    assert!(first.cancelled > 0, "{first:?}");
    assert_eq!(first.made + first.already_cached + first.cancelled, 40);
    assert_eq!(first.failed, 0);
    let second = second.wait();
    assert_eq!(second.made + second.already_cached, 2);
}

#[test]
fn unreadable_files_count_as_failed_not_fatal() {
    let s = setup("thumb-pregen-fail", true);
    let good = s.file("good.jpg", &fixtures::chart_jpeg(800, 600, 90));
    let bad = s.file("bad.jpg", b"not a jpeg");
    let summary = s
        .engine
        .pregenerate_thumbnails("root", vec![bad, good.clone()])
        .wait();
    assert_eq!((summary.made, summary.failed), (1, 1));
    assert_eq!(s.thumb(&good).source, ThumbnailSource::Cache);
}

#[test]
fn without_a_cache_pregeneration_does_nothing() {
    let s = setup("thumb-pregen-nocache", false);
    let p = s.file("a.jpg", &fixtures::chart_jpeg(800, 600, 90));
    let batch = s.engine.pregenerate_thumbnails("root", vec![p]);
    assert!(batch.is_empty());
}
