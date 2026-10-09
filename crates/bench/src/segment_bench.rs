//! `bench --segment [DIR]`: generated masks (ADR 0074) on each camera file, as the
//! editor makes them, with the time and the share of the photo covered. With a
//! directory, each photo is written there with its masks tinted over it.

use std::path::{Path, PathBuf};

use app_core::{EditRecipe, Engine, EngineConfig, PreviewQuality, PreviewRequest};
use image_core::{OutputImage, PixelFormat};
use serde_json::{Value, json};

pub fn run(files: &[PathBuf], out: Option<&Path>) -> Value {
    let rows: Vec<Value> = files.iter().map(|f| one(f, out)).collect();
    json!({ "files": rows })
}

fn one(path: &Path, out: Option<&Path>) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let engine = Engine::new(EngineConfig::default());
    let kinds = engine.mask_kinds();
    if kinds.is_empty() {
        return json!({ "file": name, "error": "no masks on this computer" });
    }
    let summary = match engine.open(path).wait() {
        Ok(s) => s,
        Err(e) => return json!({ "file": name, "error": format!("{e:?}") }),
    };
    let mut row = serde_json::Map::new();
    row.insert("file".into(), json!(name));
    let mut masks = Vec::new();
    for kind in kinds {
        // The first run loads the system's model; the second is what a photo costs.
        let _ = engine.segment(summary.id, kind).wait();
        match engine.segment(summary.id, kind).wait() {
            Ok(mask) => {
                row.insert(
                    format!("{kind:?}").to_lowercase(),
                    json!({
                        "ms": mask.as_ref().map(|m| (m.ms * 10.0).round() / 10.0),
                        "share": mask.as_ref().map(|m| (m.coverage.share() * 1000.0).round() / 1000.0),
                        "size": mask.as_ref().map(|m| format!("{}x{}", m.coverage.width, m.coverage.height)),
                    }),
                );
                masks.push((kind, mask));
            }
            Err(e) => {
                row.insert(
                    format!("{kind:?}").to_lowercase(),
                    json!({ "error": format!("{e:?}") }),
                );
            }
        }
    }
    if let Some(dir) = out {
        let photo = engine
            .render_preview(PreviewRequest {
                image: summary.id,
                recipe: EditRecipe::default(),
                quality: PreviewQuality::Detail,
                target_long_edge: 1200,
                window: None,
            })
            .wait()
            .expect("render")
            .image;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let write = |name: String, image: &OutputImage| {
            let bytes = export::encode_in(
                image,
                export::ExportFormat::Jpeg { quality: 85 },
                export::colour::ExportColourSpace::Srgb,
                None,
            )
            .expect("encode");
            std::fs::write(dir.join(name.to_lowercase()), bytes).expect("write");
        };
        // The photo itself, to judge where a mask should have been.
        write(format!("photo-{stem}.jpg"), &tint(&photo, |_, _| 0.0));
        for (kind, mask) in masks {
            let Some(mask) = mask else { continue };
            write(
                format!("mask-{stem}-{kind:?}.jpg"),
                &tint(&photo, |x, y| mask.coverage.at(x, y)),
            );
        }
    }
    Value::Object(row)
}

/// `photo` (RGBA) as RGB with `coverage` (by fractions of the photo) tinted amber.
fn tint(photo: &OutputImage, coverage: impl Fn(f32, f32) -> f32) -> OutputImage {
    let (w, h) = (photo.width(), photo.height());
    let ch = photo.format().channels();
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = coverage((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32) * 0.55;
            let i = ((y * w + x) as usize) * ch;
            for (k, amber) in [240.0f32, 180.0, 94.0].into_iter().enumerate() {
                let v = f32::from(photo.data()[i + k]);
                data.push((v + (amber - v) * c).round() as u8);
            }
        }
    }
    OutputImage::from_raw(w, h, PixelFormat::Rgb8, data).expect("sized")
}
