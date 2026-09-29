//! End-to-end golden test through the RAW path: synthetic DNG -> LibRaw -> renderer.
//!
//! Tolerance is wider than the renderer-only goldens because demosaicing and colour
//! handling belong to LibRaw and may shift slightly between LibRaw versions.
//! Regenerate with: UPDATE_GOLDEN=1 cargo test -p app-core --test golden_raw

#![cfg(feature = "libraw")]

use std::path::PathBuf;

use app_core::{EditRecipe, Engine, EngineConfig, Look, PreviewQuality, PreviewRequest};

const TOLERANCE: u8 = 4;

#[test]
fn raw_pipeline_matches_golden() {
    let dir = fixtures::TempDir::new("golden-raw");
    let src = dir.path().join("chart.dng");
    std::fs::write(&src, fixtures::chart_dng(1200, 800)).unwrap();
    let engine = Engine::new(EngineConfig {
        preview_source_min_edge: 600,
        ..EngineConfig::default()
    });
    let id = engine.open(&src).wait().unwrap().id;
    let golden_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/golden/raw");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();

    // The first two predate the Standard look: flat, unchanged images. None of the
    // first three has the default sharpening, which came later (ADR 0027).
    let flat = EditRecipe {
        look: Look::Flat,
        sharpening: 0.0,
        ..EditRecipe::default()
    };
    for (name, recipe) in [
        ("dng_identity", flat),
        (
            "dng_edited",
            EditRecipe {
                exposure: 0.5,
                contrast: 30.0,
                temperature: 25.0,
                saturation: 20.0,
                ..flat
            },
        ),
        (
            "dng_standard",
            EditRecipe {
                sharpening: 0.0,
                ..EditRecipe::default()
            },
        ),
        ("dng_default", EditRecipe::default()),
    ] {
        let frame = engine
            .render_preview(PreviewRequest {
                image: id,
                recipe,
                quality: PreviewQuality::Detail,
                target_long_edge: 600,
            })
            .wait()
            .unwrap();
        let rgb: Vec<u8> = frame
            .image
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let path = golden_dir.join(format!("{name}.png"));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = png::Encoder::new(
                std::io::BufWriter::new(file),
                frame.image.width(),
                frame.image.height(),
            );
            enc.set_color(png::ColorType::Rgb);
            enc.write_header().unwrap().write_image_data(&rgb).unwrap();
            continue;
        }
        let file = std::fs::File::open(&path)
            .unwrap_or_else(|_| panic!("missing {} (run with UPDATE_GOLDEN=1)", path.display()));
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .unwrap();
        let mut expected = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut expected).unwrap();
        let max = rgb
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= TOLERANCE, "{name}: max channel diff {max}");
    }
}
