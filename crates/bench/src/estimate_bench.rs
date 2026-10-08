//! `bench --estimate`: export size estimates (ADR 0068) against real exports, per
//! format and size, on each camera file. Prints the exponent each real export implies
//! (`bytes ∝ pixels ^ exponent`, from the sample) and the estimate's error, for fitting
//! `export::estimate`.

use std::path::{Path, PathBuf};

use app_core::{
    EditRecipe, Engine, EngineConfig, ExportColourSpace, ExportFormat, FileExport, Judgements,
    MetadataChoice, OutputSharpening,
};
use serde_json::{Value, json};

pub fn run(files: &[PathBuf]) -> Value {
    let rows: Vec<Value> = files.iter().map(PathBuf::as_path).map(one).collect();
    json!({ "files": rows })
}

fn one(path: &Path) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let engine = Engine::new(EngineConfig::default());
    let summary = match engine.open(path).wait() {
        Ok(s) => s,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    let dir = fixtures::TempDir::new("estimate-bench");
    let recipe = EditRecipe::default();
    let mut cases = Vec::new();
    for (label, format) in [
        ("jpeg85", ExportFormat::Jpeg { quality: 85 }),
        ("jpeg95", ExportFormat::Jpeg { quality: 95 }),
        ("png", ExportFormat::Png),
        ("tiff", ExportFormat::Tiff),
    ] {
        for long_edge in [Some(1350), Some(2048), None] {
            let estimate = engine
                .estimate_export(
                    summary.id,
                    &recipe,
                    format,
                    long_edge,
                    OutputSharpening::Screen,
                    ExportColourSpace::Srgb,
                )
                .wait()
                .expect("estimate");
            let actual = engine
                .export_file(
                    FileExport {
                        source: path.to_path_buf(),
                        recipe: recipe.clone(),
                        destination: dir.path().join(format!(
                            "out-{label}-{}.{}",
                            long_edge.unwrap_or(0),
                            format.extensions()[0]
                        )),
                        format,
                        long_edge,
                        sharpening: OutputSharpening::Screen,
                        colour_space: ExportColourSpace::Srgb,
                        metadata: MetadataChoice::None,
                        judgements: Judgements::default(),
                        watermark: None,
                    },
                    |_| {},
                )
                .wait()
                .expect("export")
                .bytes as f64;
            let target = f64::from(estimate.width) * f64::from(estimate.height);
            let ratio = target / estimate.sample_pixels as f64;
            let implied = if (ratio - 1.0).abs() > 1e-3 {
                (actual / estimate.sample_bytes as f64).ln() / ratio.ln()
            } else {
                f64::NAN
            };
            cases.push(json!({
                "format": label,
                "long_edge": long_edge,
                "size": format!("{}x{}", estimate.width, estimate.height),
                "actual": actual,
                "estimate": estimate.bytes,
                "error_pct": ((estimate.bytes as f64 / actual - 1.0) * 1000.0).round() / 10.0,
                "implied_exponent": (implied * 1000.0).round() / 1000.0,
                "sample_pixels": estimate.sample_pixels,
                "sample_bytes": estimate.sample_bytes,
                "target_pixels": target,
            }));
        }
    }
    json!({ "file": name, "cases": cases })
}
