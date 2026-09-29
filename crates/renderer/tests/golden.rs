//! Golden image tests: fixed synthetic source + known recipes = expected PNGs.
//!
//! The source is generated procedurally (no decoder involved) so these tests detect
//! renderer changes only. Regenerate intentionally with:
//!
//!     UPDATE_GOLDEN=1 cargo test -p renderer --test golden
//!
//! and bump `RENDERER_VERSION` when output changes on purpose.

use std::path::PathBuf;

use image_core::{NeverCancel, PixelFormat};
use renderer::{CpuRenderer, EditRecipe, Look, RenderBackend, RenderPlan};

/// Allowed per-channel difference, absorbing libm differences across platforms.
const TOLERANCE: u8 = 1;

fn cases() -> Vec<(&'static str, EditRecipe)> {
    // Single adjustments on the flat look (no base curve), so each image shows just
    // that adjustment; these images predate the Standard look and are unchanged.
    let r = EditRecipe {
        look: Look::Flat,
        ..EditRecipe::default()
    };
    let standard = EditRecipe::default();
    vec![
        ("standard_identity", standard),
        (
            "standard_combined",
            EditRecipe {
                exposure: 0.4,
                contrast: 25.0,
                temperature: 20.0,
                saturation: 15.0,
                ..standard
            },
        ),
        ("identity", r),
        ("exposure_plus1", EditRecipe { exposure: 1.0, ..r }),
        (
            "exposure_minus1",
            EditRecipe {
                exposure: -1.0,
                ..r
            },
        ),
        (
            "contrast_plus60",
            EditRecipe {
                contrast: 60.0,
                ..r
            },
        ),
        (
            "contrast_minus60",
            EditRecipe {
                contrast: -60.0,
                ..r
            },
        ),
        (
            "temperature_warm60",
            EditRecipe {
                temperature: 60.0,
                ..r
            },
        ),
        (
            "temperature_cool60",
            EditRecipe {
                temperature: -60.0,
                ..r
            },
        ),
        ("tint_plus60", EditRecipe { tint: 60.0, ..r }),
        (
            "vibrance_plus80",
            EditRecipe {
                vibrance: 80.0,
                ..r
            },
        ),
        (
            "vibrance_minus80",
            EditRecipe {
                vibrance: -80.0,
                ..r
            },
        ),
        (
            "saturation_minus100",
            EditRecipe {
                saturation: -100.0,
                ..r
            },
        ),
        (
            "saturation_plus60",
            EditRecipe {
                saturation: 60.0,
                ..r
            },
        ),
        (
            "highlights_minus60",
            EditRecipe {
                highlights: -60.0,
                ..r
            },
        ),
        ("shadows_plus60", EditRecipe { shadows: 60.0, ..r }),
        (
            "whites_plus50_blacks_minus50",
            EditRecipe {
                whites: 50.0,
                blacks: -50.0,
                ..r
            },
        ),
        (
            "combined",
            EditRecipe {
                exposure: 0.4,
                contrast: 25.0,
                temperature: 20.0,
                saturation: 15.0,
                ..r
            },
        ),
    ]
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/golden/renderer")
}

#[test]
fn renderer_matches_golden_images() {
    let source = fixtures::chart_linear(480, 320);
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let dir = golden_dir();
    let mut failures = Vec::new();

    for (name, recipe) in cases() {
        let plan = RenderPlan::from_recipe(&recipe, None);
        let out = CpuRenderer
            .render(&plan, &source, PixelFormat::Rgb8, &NeverCancel)
            .unwrap();
        let path = dir.join(format!("{name}.png"));
        // Keep the recipe next to the image so goldens are self-describing.
        let recipe_path = dir.join(format!("{name}.recipe.json"));

        if update {
            std::fs::create_dir_all(&dir).unwrap();
            write_png(&path, out.width(), out.height(), out.data());
            std::fs::write(&recipe_path, recipe.to_json()).unwrap();
            continue;
        }

        let Some((w, h, expected)) = read_png(&path) else {
            failures.push(format!(
                "{name}: missing golden {} (run with UPDATE_GOLDEN=1)",
                path.display()
            ));
            continue;
        };
        let stored = std::fs::read_to_string(&recipe_path).unwrap_or_default();
        assert_eq!(
            EditRecipe::from_json(&stored).ok(),
            Some(recipe),
            "{name}: recipe drift"
        );
        if (w, h) != (out.width(), out.height()) {
            failures.push(format!(
                "{name}: size {w}x{h} != {}x{}",
                out.width(),
                out.height()
            ));
            continue;
        }
        let max = out
            .data()
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if max > TOLERANCE {
            let actual_path = std::env::temp_dir().join(format!("golden-actual-{name}.png"));
            write_png(&actual_path, out.width(), out.height(), out.data());
            failures.push(format!(
                "{name}: max channel diff {max} (actual written to {})",
                actual_path.display()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "golden mismatches:\n{}",
        failures.join("\n")
    );
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgb: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgb).unwrap();
}

fn read_png(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((info.width, info.height, buf))
}
