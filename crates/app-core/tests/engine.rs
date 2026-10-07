//! Engine integration tests: open -> preview -> edit -> export, with real decoders.

use std::path::{Path, PathBuf};
// Only the RAW-dependent tests measure time.
#[cfg_attr(not(feature = "libraw"), allow(unused_imports))]
use std::time::{Duration, Instant};

use app_core::{
    EditRecipe, Engine, EngineConfig, ErrorKind, ExportFormat, ExportRequest, FileExport, ImageId,
    PreviewQuality, PreviewRequest, PreviewSlot,
};

fn engine() -> Engine {
    Engine::new(EngineConfig {
        preview_source_min_edge: 300,
        ..EngineConfig::default()
    })
}

fn write(dir: &Path, name: &str, bytes: Vec<u8>) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn preview(
    engine: &Engine,
    id: ImageId,
    recipe: EditRecipe,
    quality: PreviewQuality,
) -> app_core::PreviewFrame {
    engine
        .render_preview(PreviewRequest {
            image: id,
            recipe,
            quality,
            target_long_edge: 1000,
        })
        .wait()
        .unwrap()
}

#[test]
#[cfg(feature = "libraw")] // needs the RAW decoder
fn opens_raw_at_preview_resolution_and_builds_pyramid() {
    let dir = fixtures::TempDir::new("engine-open");
    let path = write(dir.path(), "chart.dng", fixtures::chart_dng(1200, 800));
    let engine = engine();
    let s = engine.open(&path).wait().unwrap();
    assert_eq!(s.decoder, "libraw");
    assert_eq!((s.full_width, s.full_height), (1200, 800));
    // Half-size decode (600 >= 300), then halving to the 256 thumbnail minimum.
    assert_eq!(s.levels, vec![(600, 400), (300, 200)]);
    assert!(s.camera.contains(fixtures::DNG_MODEL));
}

#[test]
fn preview_qualities_pick_different_levels() {
    let dir = fixtures::TempDir::new("engine-quality");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(3200, 2000, 90),
    );
    let limits = renderer::QualityLimits {
        thumbnail_long_edge: 256,
        interactive_max_long_edge: 900,
    };
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 1600,
        limits,
        ..EngineConfig::default()
    });
    let s = engine.open(&path).wait().unwrap();
    assert_eq!(s.levels, vec![(1600, 1000), (800, 500), (400, 250)]);
    let request = |quality| PreviewRequest {
        image: s.id,
        recipe: EditRecipe::default(),
        quality,
        target_long_edge: 4000,
    };
    let thumb = engine
        .render_preview(request(PreviewQuality::Thumbnail))
        .wait()
        .unwrap();
    let inter = engine
        .render_preview(request(PreviewQuality::Interactive))
        .wait()
        .unwrap();
    let detail = engine
        .render_preview(request(PreviewQuality::Detail))
        .wait()
        .unwrap();
    assert_eq!(thumb.image.width(), 400);
    // Target 900; 800 is within the interactive undersample tolerance.
    assert_eq!(inter.image.width(), 800);
    assert_eq!(detail.image.width(), 1600);
}

#[test]
#[cfg(feature = "libraw")] // needs the RAW decoder
fn repeated_request_is_a_cache_hit_and_edits_miss() {
    let dir = fixtures::TempDir::new("engine-cache");
    let path = write(dir.path(), "chart.dng", fixtures::chart_dng(800, 600));
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let r = EditRecipe {
        exposure: 0.5,
        ..Default::default()
    };
    let first = preview(&engine, id, r.clone(), PreviewQuality::Interactive);
    let second = preview(&engine, id, r.clone(), PreviewQuality::Interactive);
    assert!(!first.cache_hit);
    assert!(second.cache_hit);
    assert_eq!(first.image.data(), second.image.data());
    let edited = preview(
        &engine,
        id,
        EditRecipe {
            exposure: 0.6,
            ..r.clone()
        },
        PreviewQuality::Interactive,
    );
    assert!(!edited.cache_hit);
    assert_ne!(edited.image.data(), first.image.data());
    let stats = engine.preview_cache_stats();
    assert_eq!((stats.hits, stats.entries), (1, 2));
}

#[test]
fn newer_preview_supersedes_older_ones() {
    let dir = fixtures::TempDir::new("engine-supersede");
    let path = write(dir.path(), "big.jpg", fixtures::chart_jpeg(4000, 3000, 85));
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 4000,
        ..EngineConfig::default()
    });
    let id = engine.open(&path).wait().unwrap().id;

    // Simulate a slider drag: many requests in quick succession.
    let handles: Vec<_> = (0..20)
        .map(|i| {
            engine.render_preview(PreviewRequest {
                image: id,
                recipe: EditRecipe {
                    exposure: i as f32 * 0.05,
                    ..Default::default()
                },
                quality: PreviewQuality::Detail,
                target_long_edge: 4000,
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.wait()).collect();
    let cancelled = results
        .iter()
        .filter(|r| matches!(r, Err(e) if *e == app_core::JobError::Cancelled))
        .count();
    assert!(
        results.last().unwrap().is_ok(),
        "latest request must complete"
    );
    assert!(
        cancelled >= 15,
        "only {cancelled}/19 obsolete renders were cancelled"
    );
}

#[test]
fn files_export_fitted_to_a_long_edge_without_being_open() {
    let dir = fixtures::TempDir::new("engine-export-file");
    let path = write(dir.path(), "big.jpg", fixtures::chart_jpeg(3000, 2000, 90));
    let engine = engine();
    let export = |long_edge, recipe: EditRecipe, name: &str| {
        engine
            .export_file(
                FileExport {
                    source: path.clone(),
                    recipe,
                    destination: dir.path().join(name),
                    format: ExportFormat::Jpeg { quality: 85 },
                    sharpening: app_core::OutputSharpening::None,
                    colour_space: app_core::ExportColourSpace::Srgb,
                    metadata: app_core::MetadataChoice::All,
                    long_edge,
                },
                |_| {},
            )
            .wait()
            .unwrap()
    };
    let web = export(Some(2048), EditRecipe::default(), "web.jpg");
    assert_eq!((web.width, web.height), (2048, 1365));
    let full = export(None, EditRecipe::default(), "full.jpg");
    assert_eq!((full.width, full.height), (3000, 2000));
    // Cropped to the left half: still fills the long edge (decoded large enough).
    let half = EditRecipe {
        geometry: Some(renderer::Geometry {
            crop: renderer::CropRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let cropped = export(Some(1350), half, "cropped.jpg");
    assert_eq!(cropped.width.max(cropped.height), 1350);
    // Smaller than the long edge already: never enlarged.
    let small = export(Some(4000), EditRecipe::default(), "small.jpg");
    assert_eq!((small.width, small.height), (3000, 2000));
}

#[test]
fn compare_renders_have_their_own_slot() {
    let dir = fixtures::TempDir::new("engine-compare-slot");
    let path = write(dir.path(), "big.jpg", fixtures::chart_jpeg(4000, 3000, 85));
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 4000,
        ..EngineConfig::default()
    });
    let id = engine.open(&path).wait().unwrap().id;
    let request = |exposure: f32| PreviewRequest {
        image: id,
        recipe: EditRecipe {
            exposure,
            ..Default::default()
        },
        quality: PreviewQuality::Detail,
        target_long_edge: 4000,
    };
    // The before image, then the live edit changing while it renders: the edit's
    // requests supersede each other, not the before image.
    let before = engine.render_preview_in(request(0.0), PreviewSlot::Compare);
    let edits: Vec<_> = (1..6)
        .map(|i| engine.render_preview(request(i as f32 * 0.1)))
        .collect();
    let before = before.wait().expect("the before image is not cancelled");
    let results: Vec<_> = edits.into_iter().map(|h| h.wait()).collect();
    assert!(results.last().unwrap().is_ok());
    assert_ne!(
        before.image.data(),
        results.last().unwrap().as_ref().unwrap().image.data()
    );
    // A newer compare request still supersedes an older one.
    let old = engine.render_preview_in(request(-0.3), PreviewSlot::Compare);
    let new = engine.render_preview_in(request(-0.4), PreviewSlot::Compare);
    assert!(new.wait().is_ok());
    assert!(matches!(
        old.wait(),
        Ok(_) | Err(app_core::JobError::Cancelled)
    ));
}

#[test]
#[cfg(feature = "libraw")] // needs the RAW decoder
fn export_writes_full_resolution_and_never_touches_source() {
    let dir = fixtures::TempDir::new("engine-export");
    let path = write(dir.path(), "chart.dng", fixtures::chart_dng(1200, 800));
    let before = std::fs::read(&path).unwrap();
    let before_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let dest = dir.path().join("out.jpg");
    let stages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&stages);
    let summary = engine
        .export(
            ExportRequest {
                image: id,
                recipe: EditRecipe {
                    exposure: 0.3,
                    contrast: 20.0,
                    ..Default::default()
                },
                destination: dest.clone(),
                format: ExportFormat::Jpeg { quality: 90 },
                sharpening: app_core::OutputSharpening::None,
                colour_space: app_core::ExportColourSpace::Srgb,
                metadata: app_core::MetadataChoice::All,
            },
            move |p| seen.lock().unwrap().push(p.stage),
        )
        .wait()
        .unwrap();
    assert_eq!((summary.width, summary.height), (1200, 800));
    assert!(std::fs::metadata(&dest).unwrap().len() as usize == summary.bytes);
    assert_eq!(stages.lock().unwrap().len(), 4);
    assert_eq!(std::fs::read(&path).unwrap(), before, "source modified");
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before_mtime
    );
}

#[test]
fn export_over_source_is_rejected() {
    let dir = fixtures::TempDir::new("engine-export-src");
    let path = write(dir.path(), "chart.jpg", fixtures::chart_jpeg(400, 300, 90));
    let before = std::fs::read(&path).unwrap();
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let err = engine
        .export(
            ExportRequest {
                image: id,
                recipe: EditRecipe::default(),
                destination: path.clone(),
                format: ExportFormat::Jpeg { quality: 90 },
                sharpening: app_core::OutputSharpening::None,
                colour_space: app_core::ExportColourSpace::Srgb,
                metadata: app_core::MetadataChoice::All,
            },
            |_| {},
        )
        .wait()
        .unwrap_err();
    assert!(
        matches!(err, app_core::JobError::Failed(ref e) if e.kind == ErrorKind::InvalidDestination),
        "{err:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
#[cfg(feature = "libraw")] // needs the RAW decoder
fn interactive_preview_is_not_blocked_by_running_export() {
    let dir = fixtures::TempDir::new("engine-lanes");
    let path = write(dir.path(), "big.dng", fixtures::chart_dng(4000, 3000));
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let export = engine.export(
        ExportRequest {
            image: id,
            recipe: EditRecipe::default(),
            destination: dir.path().join("out.jpg"),
            format: ExportFormat::Jpeg { quality: 90 },
            sharpening: app_core::OutputSharpening::None,
            colour_space: app_core::ExportColourSpace::Srgb,
            metadata: app_core::MetadataChoice::All,
        },
        |_| {},
    );
    let t0 = Instant::now();
    let frame = preview(
        &engine,
        id,
        EditRecipe {
            saturation: 20.0,
            ..Default::default()
        },
        PreviewQuality::Interactive,
    );
    let preview_latency = t0.elapsed();
    let summary = export.wait().unwrap();
    assert!(!frame.cache_hit);
    // The preview must not wait for the export to finish.
    assert!(
        preview_latency < Duration::from_secs_f64(summary.total_ms / 1000.0),
        "preview took {preview_latency:?}, export {} ms",
        summary.total_ms
    );
}

#[test]
fn closed_or_unknown_images_report_not_open() {
    let dir = fixtures::TempDir::new("engine-close");
    let path = write(dir.path(), "chart.jpg", fixtures::chart_jpeg(400, 300, 90));
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    preview(
        &engine,
        id,
        EditRecipe::default(),
        PreviewQuality::Interactive,
    );
    engine.close(id);
    assert_eq!(engine.preview_cache_stats().entries, 0);
    let err = engine
        .render_preview(PreviewRequest {
            image: id,
            recipe: EditRecipe::default(),
            quality: PreviewQuality::Interactive,
            target_long_edge: 500,
        })
        .wait()
        .unwrap_err();
    assert!(matches!(err, app_core::JobError::Failed(ref e) if e.kind == ErrorKind::ImageNotOpen));
}

#[test]
fn open_errors_are_user_facing() {
    let dir = fixtures::TempDir::new("engine-errors");
    let engine = engine();
    let err = engine
        .open(dir.path().join("missing.nef"))
        .wait()
        .unwrap_err();
    assert!(
        matches!(err, app_core::JobError::Failed(ref e) if e.kind == ErrorKind::DecodeFailed || e.kind == ErrorKind::NotFound),
        "{err:?}"
    );

    let bad = write(dir.path(), "bad.cr3", vec![7u8; 10_000]);
    let err = app_core::EngineError::from_job(engine.open(&bad).wait().unwrap_err());
    assert!(
        matches!(err.kind, ErrorKind::Unsupported | ErrorKind::DecodeFailed),
        "{err:?}"
    );
    assert!(
        !err.message.contains("LibRaw"),
        "technical detail leaked: {}",
        err.message
    );

    let txt = write(dir.path(), "notes.txt", b"hello".to_vec());
    let err = app_core::EngineError::from_job(engine.open(&txt).wait().unwrap_err());
    assert_eq!(err.kind, ErrorKind::Unsupported);
}

#[test]
fn open_images_are_bounded() {
    let dir = fixtures::TempDir::new("engine-bounded");
    let engine = Engine::new(EngineConfig {
        max_open_images: 2,
        preview_source_min_edge: 300,
        ..EngineConfig::default()
    });
    let ids: Vec<_> = (0..3)
        .map(|i| {
            let p = write(
                dir.path(),
                &format!("{i}.jpg"),
                fixtures::chart_jpeg(400 + i * 8, 300, 90),
            );
            engine.open(&p).wait().unwrap().id
        })
        .collect();
    let err = engine
        .render_preview(PreviewRequest {
            image: ids[0],
            recipe: EditRecipe::default(),
            quality: PreviewQuality::Interactive,
            target_long_edge: 500,
        })
        .wait();
    assert!(err.is_err(), "oldest image should have been evicted");
    preview(
        &engine,
        ids[2],
        EditRecipe::default(),
        PreviewQuality::Interactive,
    );
}

#[test]
fn long_drag_does_not_evict_settled_renders() {
    let dir = fixtures::TempDir::new("engine-admission");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(1600, 1000, 90),
    );
    // Budget for ~16 frames of this size: the interactive pool holds only ~2.
    let frame_bytes = 1600 * 1000 * 4;
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 1600,
        preview_cache_bytes: frame_bytes * 16,
        ..EngineConfig::default()
    });
    let id = engine.open(&path).wait().unwrap().id;
    let settled = EditRecipe {
        contrast: 30.0,
        ..Default::default()
    };
    let detail = |r| PreviewRequest {
        image: id,
        recipe: r,
        quality: PreviewQuality::Detail,
        target_long_edge: 1600,
    };
    assert!(
        !engine
            .render_preview(detail(settled.clone()))
            .wait()
            .unwrap()
            .cache_hit
    );

    for i in 0..40 {
        let r = EditRecipe {
            exposure: i as f32 * 0.05,
            ..Default::default()
        };
        let req = PreviewRequest {
            image: id,
            recipe: r,
            quality: PreviewQuality::Interactive,
            target_long_edge: 1600,
        };
        engine.render_preview(req).wait().unwrap();
    }
    assert!(
        engine
            .render_preview(detail(settled.clone()))
            .wait()
            .unwrap()
            .cache_hit,
        "settled render was evicted by drag frames"
    );
    assert!(engine.preview_cache_stats().used_bytes <= frame_bytes * 16);
}

#[test]
#[cfg(feature = "libraw")] // needs the RAW decoder
fn embedded_preview_arrives_before_open_completes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/local/nikon-z6-14bit-lossless.nef");
    if !path.exists() {
        eprintln!("local camera fixture missing; skipping");
        return;
    }
    let engine = Engine::new(EngineConfig::default());
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = engine.open_with_preview(&path, move |frame| {
        tx.send((Instant::now(), frame)).unwrap();
    });
    let summary = handle.wait().unwrap();
    let done = Instant::now();
    let (arrived, frame) = rx.try_recv().expect("embedded preview delivered");
    assert!(arrived < done);
    assert!(frame.image.width().max(frame.image.height()) >= 1024);
    let ms = summary
        .embedded_preview_ms
        .expect("summary records extraction time");
    assert!(
        ms < summary.decode_ms,
        "extraction ({ms} ms) should beat decode ({} ms)",
        summary.decode_ms
    );
}

#[test]
fn files_without_embedded_preview_open_normally() {
    let dir = fixtures::TempDir::new("engine-no-embedded");
    let engine = engine();
    let mut files = vec![("chart.jpg", fixtures::chart_jpeg(800, 600, 90))];
    if cfg!(feature = "libraw") {
        files.push(("chart.dng", fixtures::chart_dng(800, 600)));
    }
    for (name, bytes) in files {
        let path = write(dir.path(), name, bytes);
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&called);
        let summary = engine
            .open_with_preview(&path, move |_| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst)
            })
            .wait()
            .unwrap();
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst), "{name}");
        assert_eq!(summary.embedded_preview_ms, None, "{name}");
    }
}

#[test]
fn preview_cache_budget_can_shrink_at_runtime() {
    let dir = fixtures::TempDir::new("engine-cache-budget");
    let path = write(dir.path(), "chart.jpg", fixtures::chart_jpeg(800, 600, 90));
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    for i in 0..4 {
        let r = EditRecipe {
            exposure: i as f32 * 0.1,
            ..Default::default()
        };
        preview(&engine, id, r, PreviewQuality::Detail);
    }
    assert!(engine.preview_cache_stats().entries >= 4);
    engine.set_preview_cache_budget(800 * 600 * 4 * 2);
    let stats = engine.preview_cache_stats();
    assert!(stats.used_bytes <= stats.budget_bytes, "{stats:?}");
    assert_eq!(stats.budget_bytes, 800 * 600 * 4 * 2);
}

#[test]
fn viewer_frames_carry_their_histogram() {
    let dir = fixtures::TempDir::new("engine-histogram");
    let path = write(dir.path(), "chart.jpg", fixtures::chart_jpeg(800, 500, 90));
    let engine = engine();
    let s = engine.open(&path).wait().unwrap();
    for quality in [PreviewQuality::Interactive, PreviewQuality::Detail] {
        let frame = preview(&engine, s.id, EditRecipe::default(), quality);
        let h = frame.histogram.expect("a histogram");
        let pixels = u64::from(frame.image.width() * frame.image.height());
        assert_eq!(h.total(), pixels, "{quality:?}");
        // A cache hit brings one too.
        let again = preview(&engine, s.id, EditRecipe::default(), quality);
        assert!(again.cache_hit && again.histogram.as_deref() == Some(&*h));
    }
    let thumb = preview(
        &engine,
        s.id,
        EditRecipe::default(),
        PreviewQuality::Thumbnail,
    );
    assert!(thumb.histogram.is_none());
}
