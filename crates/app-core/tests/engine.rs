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
            window: None,
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
        window: None,
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
                window: None,
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
                    judgements: Default::default(),
                    watermark: None,
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
        window: None,
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
                judgements: Default::default(),
                watermark: None,
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
                judgements: Default::default(),
                watermark: None,
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
            judgements: Default::default(),
            watermark: None,
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
            window: None,
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
            window: None,
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
        window: None,
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
            window: None,
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

#[test]
fn windows_show_their_part_of_the_photo_at_full_resolution() {
    let dir = fixtures::TempDir::new("engine-window");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(3200, 2000, 90),
    );
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 1600,
        ..EngineConfig::default()
    });
    let id = engine.open(&path).wait().unwrap().id;
    let recipe = EditRecipe {
        exposure: 0.4,
        clarity: 30.0,
        geometry: Some(renderer::geometry::Geometry {
            straighten: 2.0,
            crop: renderer::geometry::CropRect {
                x: 0.1,
                y: 0.1,
                w: 0.8,
                h: 0.8,
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    // At 100 % (the whole output's long edge at the zoom is its own).
    let window = |quality, window| {
        engine
            .render_preview(PreviewRequest {
                image: id,
                recipe: recipe.clone(),
                quality,
                target_long_edge: 3200,
                window: Some(window),
            })
            .wait()
            .unwrap()
    };

    // Before the full resolution is decoded: from the preview source (half size),
    // covering at least the window asked for.
    let early = window(PreviewQuality::Detail, (301, 201, 400, 300));
    let [x, y, w, h] = early.window.unwrap();
    assert!(x <= 301.0 && y <= 201.0 && x + w >= 701.0 && y + h >= 501.0);
    assert_eq!((early.image.width(), early.image.height()), (201, 151));
    assert!(early.histogram.is_none());
    let (ow, oh) = early.full_size;

    engine.prepare_full(id).wait().unwrap();
    let detail = window(PreviewQuality::Detail, (301, 201, 400, 300));
    assert_eq!(detail.window, Some([301.0, 201.0, 400.0, 300.0]));
    assert_eq!((detail.image.width(), detail.image.height()), (400, 300));
    assert_eq!(detail.full_size, (ow, oh));
    // That part of the whole photo rendered at full resolution: the same up to the
    // rounding of clarity's running-sum blur, which starts at each row chunk.
    let whole = window(PreviewQuality::Detail, (0, 0, ow, oh));
    assert_eq!((whole.image.width(), whole.image.height()), (ow, oh));
    let stride = ow as usize * 4;
    let mut differ = 0;
    for row in 0..300 {
        let at = (201 + row) * stride + 301 * 4;
        let a = &detail.image.data()[row * 400 * 4..(row + 1) * 400 * 4];
        let b = &whole.image.data()[at..at + 400 * 4];
        for (p, q) in a.iter().zip(b) {
            assert!(p.abs_diff(*q) <= 1, "row {row}: {p} vs {q}");
            differ += usize::from(p != q);
        }
    }
    assert!(differ < 400 * 300 / 1000, "{differ} values differ");
    assert!(
        window(PreviewQuality::Detail, (301, 201, 400, 300)).cache_hit,
        "a repeated window is cached"
    );
    // While dragging, windows still come from the preview source.
    let dragging = window(PreviewQuality::Interactive, (301, 201, 400, 300));
    assert_eq!(
        (dragging.image.width(), dragging.image.height()),
        (201, 151)
    );
}

#[test]
fn removals_show_one_full_resolution_fill_at_every_size() {
    let dir = fixtures::TempDir::new("engine-fill");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(1600, 1000, 90),
    );
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 800,
        ..EngineConfig::default()
    });
    let id = engine.open(&path).wait().unwrap().id;
    let removal = renderer::remove::Removal {
        strokes: vec![renderer::masks::brush::Stroke {
            erase: false,
            size: 0.03,
            feather: 10.0,
            flow: 100.0,
            points: vec![[0.4, 0.45], [0.5, 0.5]],
        }],
    };
    let recipe = EditRecipe {
        sharpening: 0.0,
        removals: vec![removal.clone()],
        ..Default::default()
    };
    let request = |window: Option<_>| PreviewRequest {
        image: id,
        recipe: recipe.clone(),
        quality: PreviewQuality::Detail,
        // Fit in an 800 px view; a window at 100 %.
        target_long_edge: if window.is_some() { 1600 } else { 800 },
        window,
    };

    // Before the full-resolution fill: filled at the preview's size, and said so.
    let early = engine.render_preview(request(None)).wait().unwrap();
    assert!(early.fill_pending);
    engine.prepare_fill(id, vec![removal]).wait().unwrap();
    let fit = engine.render_preview(request(None)).wait().unwrap();
    assert!(!fit.fill_pending);
    assert!(
        !fit.cache_hit,
        "the stand-in is not taken for the real fill"
    );
    let full = engine
        .render_preview(request(Some((0, 0, 1600, 1000))))
        .wait()
        .unwrap();
    assert!(!full.fill_pending);
    assert_eq!((full.image.width(), full.image.height()), (1600, 1000));

    // The fit view is the 100 % view halved, removals and all: over the removal,
    // they differ about as much as anywhere (8-bit renders at two sizes), and much
    // less than the stand-in, a fill of the preview's own, does.
    let b = full.image.data();
    let difference = |frame: &app_core::PreviewFrame,
                      (x0, y0, x1, y1): (usize, usize, usize, usize)| {
        let a = frame.image.data();
        let (mut sum, mut n) = (0.0f32, 0.0f32);
        for y in y0..y1 {
            for x in x0..x1 {
                for c in 0..3 {
                    let at = |xx: usize, yy: usize| f32::from(b[(yy * 1600 + xx) * 4 + c]);
                    let halved = (at(2 * x, 2 * y)
                        + at(2 * x + 1, 2 * y)
                        + at(2 * x, 2 * y + 1)
                        + at(2 * x + 1, 2 * y + 1))
                        / 4.0;
                    sum += (f32::from(a[(y * 800 + x) * 4 + c]) - halved).abs();
                    n += 1.0;
                }
            }
        }
        sum / n
    };
    assert_eq!((fit.image.width(), fit.image.height()), (800, 500));
    let hole = (330, 215, 390, 245);
    let elsewhere = (100, 60, 160, 90);
    let (inside, outside, stand_in) = (
        difference(&fit, hole),
        difference(&fit, elsewhere),
        difference(&early, hole),
    );
    assert!(
        inside < 2.0 * outside + 1.0 && inside < stand_in / 2.0,
        "over the removal {inside}, elsewhere {outside}, the stand-in {stand_in}"
    );
}

#[test]
fn windows_come_from_the_smallest_source_sharp_enough_for_the_zoom() {
    let dir = fixtures::TempDir::new("engine-window-zoom");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(3200, 2000, 90),
    );
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 1600,
        ..EngineConfig::default()
    });
    let s = engine.open(&path).wait().unwrap();
    assert_eq!(s.levels, vec![(1600, 1000), (800, 500), (400, 250)]);
    engine.prepare_full(s.id).wait().unwrap();
    // A 600 x 400 px part of the photo (in full-resolution pixels), at a zoom given as
    // the whole photo's long edge at it.
    let at = |long_edge: u32| {
        engine
            .render_preview(PreviewRequest {
                image: s.id,
                recipe: EditRecipe::default(),
                quality: PreviewQuality::Detail,
                target_long_edge: long_edge,
                window: Some((1000, 600, 600, 400)),
            })
            .wait()
            .unwrap()
    };
    let size = |f: &app_core::PreviewFrame| (f.image.width(), f.image.height());
    // 100 % and beyond: the full resolution.
    assert_eq!(size(&at(3200)), (600, 400));
    assert_eq!(size(&at(6400)), (600, 400));
    // 50 %: the half-size level; just under it, still that one.
    assert_eq!(size(&at(1600)), (300, 200));
    assert_eq!(size(&at(1700)), (600, 400));
    // 25 %: the next level, and the window it covers is the same.
    let quarter = at(800);
    assert_eq!(size(&quarter), (150, 100));
    assert_eq!(quarter.window, Some([1000.0, 600.0, 600.0, 400.0]));
}

#[test]
fn auto_tone_finds_the_tone_sliders_afresh_from_a_small_sample() {
    let dir = fixtures::TempDir::new("engine-auto-tone");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(1600, 1000, 90),
    );
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let normal = engine.auto_tone(id, &EditRecipe::default()).wait().unwrap();
    // The edit's own tone sliders don't matter: Auto replaces them.
    let edited = EditRecipe {
        exposure: -2.0,
        highlights: 60.0,
        ..Default::default()
    };
    let found = engine.auto_tone(id, &edited).wait().unwrap();
    assert_eq!(found, normal, "the tone sliders are found afresh");
    for v in [
        normal.contrast,
        normal.highlights,
        normal.shadows,
        normal.whites,
        normal.blacks,
        normal.vibrance,
    ] {
        assert!((-100.0..=100.0).contains(&v), "{normal:?}");
    }
}

#[test]
fn auto_for_one_setting_follows_the_rest_of_the_edit() {
    let dir = fixtures::TempDir::new("engine-auto-setting");
    let path = write(
        dir.path(),
        "chart.jpg",
        fixtures::chart_jpeg(1600, 1000, 90),
    );
    let engine = engine();
    let id = engine.open(&path).wait().unwrap().id;
    let whites = |exposure: f32| {
        engine
            .auto_setting(
                id,
                &EditRecipe {
                    exposure,
                    ..Default::default()
                },
                renderer::auto_tone::ToneSetting::Whites,
            )
            .wait()
            .unwrap()
    };
    // The chart's whites are pure white: Whites pulls them in. Darkened two stops,
    // they fall short of white: Whites raises them instead.
    let (plain, darkened) = (whites(0.0), whites(-2.0));
    assert!(plain < 0.0 && darkened > plain, "{plain} vs {darkened}");
}

#[test]
fn a_subject_mask_covers_the_subject() {
    let dir = fixtures::TempDir::new("engine-segment");
    let path = write(dir.path(), "subject.jpg", fixtures::subject_jpeg(900, 600));
    let engine = engine();
    if !engine.mask_kinds().contains(&app_core::MaskKind::Subject) {
        eprintln!("skipped: this computer makes no subject masks");
        return;
    }
    let id = engine.open(&path).wait().unwrap().id;
    let mask = engine
        .segment(id, app_core::MaskKind::Subject)
        .wait()
        .unwrap()
        .expect("a subject");
    let c = &mask.coverage;
    assert!(c.at(0.5, 0.5) > 0.5, "centre {}", c.at(0.5, 0.5));
    assert!(c.at(0.05, 0.05) < 0.5, "corner {}", c.at(0.05, 0.05));
    // The disc is a third of the height across: about 9 % of the picture.
    assert!((0.04..0.2).contains(&c.share()), "share {}", c.share());
    assert!(mask.generator.contains("subject"), "{}", mask.generator);
}

#[test]
fn the_cameras_lens_profile_corrects_unless_turned_off() {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/local/sony-a7riv-61mp-14bit-compressed.arw");
    if !raw.exists() {
        eprintln!("skipped: no local camera fixtures");
        return;
    }
    let engine = engine();
    let summary = engine.open(&raw).wait().unwrap();
    assert_eq!(
        summary.lens_profile.as_deref(),
        Some("FE 24-70mm F4 ZA OSS")
    );
    let on = preview(
        &engine,
        summary.id,
        EditRecipe::default(),
        PreviewQuality::Detail,
    );
    let off = preview(
        &engine,
        summary.id,
        EditRecipe {
            profile_corrections: false,
            ..Default::default()
        },
        PreviewQuality::Detail,
    );
    assert_eq!(
        (on.image.width(), on.image.height()),
        (off.image.width(), off.image.height())
    );
    // The 24-70's falloff at f/4 lifted: the corner brighter, the centre the same.
    let mean = |f: &app_core::PreviewFrame, x0: f32, y0: f32| {
        let (w, h) = (f.image.width() as usize, f.image.height() as usize);
        let mut sum = 0u64;
        let mut n = 0u64;
        for y in (y0 * h as f32) as usize..((y0 + 0.05) * h as f32) as usize {
            for x in (x0 * w as f32) as usize..((x0 + 0.05) * w as f32) as usize {
                let i = (y * w + x) * 4;
                sum += f.image.data()[i..i + 3]
                    .iter()
                    .map(|&v| u64::from(v))
                    .sum::<u64>();
                n += 3;
            }
        }
        sum as f32 / n as f32
    };
    // (The bottom right: the top corners are sky near white, the bottom left hedge
    // near black, where the tone curve hides it.)
    assert!(mean(&on, 0.95, 0.95) > mean(&off, 0.95, 0.95) + 6.0);
    assert!((mean(&on, 0.475, 0.475) - mean(&off, 0.475, 0.475)).abs() < 3.0);
    // A JPEG records no profile.
    let dir = fixtures::TempDir::new("engine-lens");
    let jpeg = write(dir.path(), "plain.jpg", fixtures::subject_jpeg(300, 200));
    assert_eq!(engine.open(&jpeg).wait().unwrap().lens_profile, None);
}

#[test]
fn every_computer_finds_the_sky() {
    let dir = fixtures::TempDir::new("engine-sky");
    let sky = write(dir.path(), "sky.jpg", fixtures::sky_jpeg(900, 600));
    let room = write(dir.path(), "room.jpg", fixtures::subject_jpeg(900, 600));
    let engine = engine();
    assert!(engine.mask_kinds().contains(&app_core::MaskKind::Sky));
    let id = engine.open(&sky).wait().unwrap().id;
    let mask = engine
        .segment(id, app_core::MaskKind::Sky)
        .wait()
        .unwrap()
        .expect("a sky");
    let c = &mask.coverage;
    assert!(c.at(0.5, 0.1) > 0.9 && c.at(0.5, 0.35) > 0.9);
    assert!(c.at(0.5, 0.45) < 0.1 && c.at(0.5, 0.9) < 0.1);
    assert!(mask.generator.starts_with("photo-editor/sky/"));
    // A red disc on grey has no sky.
    let id = engine.open(&room).wait().unwrap().id;
    assert!(
        engine
            .segment(id, app_core::MaskKind::Sky)
            .wait()
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_generated_mask_adjusts_what_it_covers_and_a_missing_one_nothing() {
    use renderer::masks::{GeneratedKind, LocalAdjustments, Mask, MaskShape};
    let dir = fixtures::TempDir::new("engine-generated-mask");
    let path = write(dir.path(), "subject.jpg", fixtures::subject_jpeg(900, 600));
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 300,
        mask_dir: Some(dir.path().join("masks")),
        ..EngineConfig::default()
    });
    if !engine.mask_kinds().contains(&app_core::MaskKind::Subject) {
        eprintln!("skipped: this computer makes no subject masks");
        return;
    }
    let id = engine.open(&path).wait().unwrap().id;
    let made = engine
        .segment(id, app_core::MaskKind::Subject)
        .wait()
        .unwrap()
        .expect("a subject");
    let with_mask = |name: &str| EditRecipe {
        masks: vec![Mask::new(
            1,
            MaskShape::Generated {
                of: GeneratedKind::Subject,
                mask: name.to_owned(),
            },
            LocalAdjustments {
                exposure: 1.0,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let luma_at = |f: &app_core::PreviewFrame, x: f32, y: f32| {
        let (w, h) = (f.image.width() as usize, f.image.height() as usize);
        let i = ((y * h as f32) as usize * w + (x * w as f32) as usize) * 4;
        f.image.data()[i..i + 3]
            .iter()
            .map(|&v| u32::from(v))
            .sum::<u32>()
    };
    let plain = preview(&engine, id, EditRecipe::default(), PreviewQuality::Detail);
    let masked = preview(&engine, id, with_mask(&made.name), PreviewQuality::Detail);
    assert!(engine.missing_masks(id, &with_mask(&made.name)).is_empty());
    // Brighter on the subject, the same on the ground.
    assert!(luma_at(&masked, 0.5, 0.5) > luma_at(&plain, 0.5, 0.5) + 30);
    assert_eq!(luma_at(&masked, 0.05, 0.05), luma_at(&plain, 0.05, 0.05));
    // A mask the store doesn't have: reported, and it changes nothing.
    let unknown = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        engine.missing_masks(id, &with_mask(unknown)),
        vec![unknown.to_owned()]
    );
    let missing = preview(&engine, id, with_mask(unknown), PreviewQuality::Detail);
    assert_eq!(missing.image.data(), plain.image.data());
}

#[test]
fn a_mask_made_from_another_photo_is_made_again_for_this_one() {
    use renderer::masks::{GeneratedKind, LocalAdjustments, Mask, MaskShape};
    let dir = fixtures::TempDir::new("engine-pasted-mask");
    let first = write(dir.path(), "first.jpg", fixtures::subject_jpeg(900, 600));
    // The same subject, smaller, in another photo.
    let second = write(dir.path(), "second.jpg", fixtures::subject_jpeg(600, 900));
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 300,
        mask_dir: Some(dir.path().join("masks")),
        ..EngineConfig::default()
    });
    if !engine.mask_kinds().contains(&app_core::MaskKind::Subject) {
        eprintln!("skipped: this computer makes no subject masks");
        return;
    }
    let a = engine.open(&first).wait().unwrap().id;
    let made = engine
        .segment(a, app_core::MaskKind::Subject)
        .wait()
        .unwrap()
        .expect("a subject");
    // The first photo's edit pasted onto the second.
    let pasted = EditRecipe {
        masks: vec![Mask::new(
            1,
            MaskShape::Generated {
                of: GeneratedKind::Subject,
                mask: made.name.clone(),
            },
            LocalAdjustments {
                exposure: 1.0,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    // In the viewer: missing, so it's remade, and it changes nothing until then.
    let b = engine.open(&second).wait().unwrap().id;
    assert_eq!(engine.missing_masks(b, &pasted), vec![made.name.clone()]);
    let plain = preview(&engine, b, EditRecipe::default(), PreviewQuality::Detail);
    let before = preview(&engine, b, pasted.clone(), PreviewQuality::Detail);
    assert_eq!(before.image.data(), plain.image.data());
    let remade = engine
        .segment(b, app_core::MaskKind::Subject)
        .wait()
        .unwrap()
        .expect("a subject");
    assert_ne!(remade.name, made.name);
    // Exported without being opened in the viewer: made from the photo as it exports.
    let export = |recipe: EditRecipe, name: &str| {
        let destination = dir.path().join(name);
        engine
            .export_file(
                FileExport {
                    source: second.clone(),
                    recipe,
                    destination: destination.clone(),
                    format: ExportFormat::Png,
                    sharpening: app_core::OutputSharpening::None,
                    colour_space: app_core::ExportColourSpace::Srgb,
                    metadata: app_core::MetadataChoice::All,
                    judgements: Default::default(),
                    watermark: None,
                    long_edge: None,
                },
                |_| {},
            )
            .wait()
            .unwrap();
        let mut reader = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(destination).unwrap(),
        ))
        .read_info()
        .unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        let channels = info.color_type.samples();
        move |x: f32, y: f32| {
            let (w, h) = (info.width as usize, info.height as usize);
            let i = ((y * h as f32) as usize * w + (x * w as f32) as usize) * channels;
            data[i..i + 3].iter().map(|&v| u32::from(v)).sum::<u32>()
        }
    };
    let plain = export(EditRecipe::default(), "plain.png");
    let masked = export(pasted, "masked.png");
    assert!(
        masked(0.5, 0.5) > plain(0.5, 0.5) + 30,
        "the subject brighter"
    );
    assert_eq!(
        masked(0.05, 0.05),
        plain(0.05, 0.05),
        "the ground unchanged"
    );
}
